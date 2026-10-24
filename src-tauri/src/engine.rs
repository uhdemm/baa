//! The speech engine: loads a GGUF model with transcribe.cpp, trims silence with
//! Silero VAD, transcribes, and frees the model again after it sits idle.

use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use transcribe_cpp::{Model, RunOptions, Session, VadOptions, VadSession};

struct Loaded {
    path: PathBuf,
    // Session keeps the model alive; the Model handle is kept for capability checks.
    model: Model,
    session: Session,
}

#[derive(Clone, serde::Serialize)]
pub struct EngineStatus {
    pub loaded: Option<String>,
    pub loading: bool,
    pub backend: Option<String>,
}

pub struct Engine {
    loaded: Mutex<Option<Loaded>>,
    loading: Mutex<bool>,
    vad: Mutex<Option<VadSession>>,
    vad_path: PathBuf,
    last_used: Mutex<Instant>,
}

pub enum Outcome {
    Text(String),
    NoSpeech,
}

impl Engine {
    pub fn new(vad_path: PathBuf) -> Arc<Self> {
        Arc::new(Engine {
            loaded: Mutex::new(None),
            loading: Mutex::new(false),
            vad: Mutex::new(None),
            vad_path,
            last_used: Mutex::new(Instant::now()),
        })
    }

    pub fn status(&self) -> EngineStatus {
        let l = self.loaded.lock();
        EngineStatus {
            loaded: l.as_ref().map(|l| l.path.to_string_lossy().into_owned()),
            loading: *self.loading.lock(),
            backend: l.as_ref().map(|l| l.model.backend()),
        }
    }

    /// Loads `path` if it isn't the model already in memory.
    pub fn ensure_loaded(&self, path: &Path) -> anyhow::Result<()> {
        *self.last_used.lock() = Instant::now();
        let mut slot = self.loaded.lock();
        if slot.as_ref().map(|l| l.path == path).unwrap_or(false) {
            return Ok(());
        }
        *slot = None; // free the old model before loading the new one
        *self.loading.lock() = true;
        let res = (|| {
            let model = Model::load(path)?;
            let session = model.session()?;
            anyhow::Ok(Loaded { path: path.to_path_buf(), model, session })
        })();
        *self.loading.lock() = false;
        *slot = Some(res?);
        Ok(())
    }

    pub fn unload(&self) {
        *self.loaded.lock() = None;
    }

    /// Drop the model if it has sat unused for `after`. Called from a timer.
    pub fn unload_if_idle(&self, after: Duration) -> bool {
        if after.is_zero() || self.last_used.lock().elapsed() < after {
            return false;
        }
        let mut slot = self.loaded.lock();
        if slot.is_some() {
            *slot = None;
            return true;
        }
        false
    }

    /// Cuts silence out of the clip. Returns None when there's no speech at all.
    fn trim_silence(&self, pcm: &[f32]) -> Option<Vec<f32>> {
        let mut vad = self.vad.lock();
        if vad.is_none() {
            match Model::load(&self.vad_path).and_then(|m| m.vad_session()) {
                Ok(s) => *vad = Some(s),
                Err(e) => {
                    eprintln!("baa: VAD unavailable ({e}), using the whole clip");
                    return Some(pcm.to_vec());
                }
            }
        }
        let opts = VadOptions { speech_pad_ms: 200, min_silence_ms: 300, ..Default::default() };
        let res = match vad.as_mut().unwrap().run(pcm, &opts) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("baa: VAD failed ({e})");
                return Some(pcm.to_vec());
            }
        };
        if res.segments.is_empty() {
            return None;
        }
        let mut out = Vec::with_capacity(pcm.len());
        for seg in res.segments {
            let a = (seg.start_sample.max(0) as usize).min(pcm.len());
            let b = (seg.end_sample.max(0) as usize).min(pcm.len());
            out.extend_from_slice(&pcm[a..b]);
            out.extend(std::iter::repeat(0.0).take(1600)); // 0.1 s gap between pieces
        }
        Some(out)
    }

    pub fn transcribe(
        &self,
        path: &Path,
        pcm: &[f32],
        use_vad: bool,
        language: Option<String>,
        vocabulary: Vec<String>,
    ) -> anyhow::Result<Outcome> {
        // Under a quarter second of audio is a mis-press, not speech.
        if pcm.len() < 4000 {
            return Ok(Outcome::NoSpeech);
        }
        let audio = if use_vad {
            match self.trim_silence(pcm) {
                Some(a) => a,
                None => return Ok(Outcome::NoSpeech),
            }
        } else {
            pcm.to_vec()
        };
        self.ensure_loaded(path)?;
        let mut slot = self.loaded.lock();
        let l = slot.as_mut().ok_or_else(|| anyhow::anyhow!("model not loaded"))?;
        let vocab_ok = l.model.supports(transcribe_cpp::Feature::Vocabulary);
        let opts = RunOptions {
            language,
            vocabulary: if vocab_ok { vocabulary } else { vec![] },
            ..Default::default()
        };
        let result = l.session.run(&audio, &opts)?;
        *self.last_used.lock() = Instant::now();
        let text = result.text.trim().to_string();
        Ok(if text.is_empty() { Outcome::NoSpeech } else { Outcome::Text(text) })
    }
}
