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
    fn trim_silence(&self, pcm: &[f32], threshold: f64) -> Option<Vec<f32>> {
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
        let opts = VadOptions { threshold, speech_pad_ms: 200, min_silence_ms: 300, ..Default::default() };
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

    pub fn transcribe(&self, path: &Path, pcm: &[f32], o: &Opts) -> anyhow::Result<Outcome> {
        // Under a quarter second of audio is a mis-press, not speech.
        if pcm.len() < 4000 {
            return Ok(Outcome::NoSpeech);
        }
        let pcm = normalize(pcm, o.whisper);
        let audio = if o.use_vad {
            match self.trim_silence(&pcm, if o.whisper { 0.35 } else { 0.5 }) {
                Some(a) => a,
                None => return Ok(Outcome::NoSpeech),
            }
        } else {
            pcm
        };
        self.ensure_loaded(path)?;
        let mut slot = self.loaded.lock();
        let l = slot.as_mut().ok_or_else(|| anyhow::anyhow!("model not loaded"))?;
        let vocab_ok = l.model.supports(transcribe_cpp::Feature::Vocabulary);
        let mut opts = RunOptions {
            language: o.language.clone(),
            vocabulary: if vocab_ok { o.vocabulary.clone() } else { vec![] },
            ..Default::default()
        };
        if o.translate {
            let caps = l.model.capabilities()?;
            if !caps.supports_translate {
                anyhow::bail!("This model can't translate. Pick Whisper or Canary in Models, or turn translation off.");
            }
            opts.task = transcribe_cpp::Task::Translate;
            opts.target_language = Some("en".into());
        }
        let result = l.session.run(&audio, &opts)?;
        *self.last_used.lock() = Instant::now();
        let text = result.text.trim().to_string();
        Ok(if text.is_empty() { Outcome::NoSpeech } else { Outcome::Text(text) })
    }
}

/// What to do with one recording.
#[derive(Default, Clone)]
pub struct Opts {
    pub use_vad: bool,
    pub language: Option<String>,
    pub vocabulary: Vec<String>,
    /// translate any language into English (Whisper, Canary)
    pub translate: bool,
    /// quiet speech: boost harder and listen for softer voices
    pub whisper: bool,
}

/// Auto-volume: brings quiet recordings up to a healthy level before the model
/// hears them. Whisper mode allows a much bigger boost for soft speech.
pub fn normalize(pcm: &[f32], whisper: bool) -> Vec<f32> {
    let peak = pcm.iter().fold(0f32, |m, s| m.max(s.abs()));
    if peak < 1e-4 {
        return pcm.to_vec();
    }
    let max_gain = if whisper { 12.0 } else { 3.0 };
    let gain = (0.6 / peak).clamp(1.0, max_gain);
    pcm.iter().map(|s| (s * gain).clamp(-1.0, 1.0)).collect()
}

impl Engine {
    /// Quick look at the words so far, for the live preview. Never waits: if the
    /// model is still loading or busy, it just skips this round.
    pub fn preview(&self, path: &Path, pcm: &[f32], language: Option<String>) -> Option<String> {
        if pcm.len() < 8000 {
            return None;
        }
        let mut slot = self.loaded.try_lock()?;
        let l = slot.as_mut().filter(|l| l.path == path)?;
        let opts = RunOptions { language, ..Default::default() };
        let text = l.session.run(pcm, &opts).ok()?.text.trim().to_string();
        *self.last_used.lock() = Instant::now();
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 16-bit mono 16 kHz WAV -> f32 samples (test fixture only).
    fn read_wav(p: &str) -> Vec<f32> {
        let b = std::fs::read(p).unwrap();
        let data = b.windows(4).position(|w| w == b"data").unwrap() + 8;
        b[data..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect()
    }

    /// cargo test --lib -- --ignored   (needs BAA_TEST_MODEL=path\to\model.gguf)
    #[test]
    #[ignore]
    fn transcribes_a_wav() {
        let model = std::env::var("BAA_TEST_MODEL").expect("set BAA_TEST_MODEL");
        let engine = Engine::new(PathBuf::from("resources/silero-vad.bin"));
        let pcm = read_wav("tests/hello.wav");
        let t0 = Instant::now();
        let out = engine.transcribe(Path::new(&model), &pcm, &Opts { use_vad: true, ..Default::default() }).unwrap();
        let first = t0.elapsed();
        let t1 = Instant::now();
        let again = engine.transcribe(Path::new(&model), &pcm, &Opts { use_vad: true, ..Default::default() }).unwrap();
        println!("backend: {:?}", engine.status().backend);
        println!("cold (load + run): {first:?}, warm run: {:?}", t1.elapsed());
        match (out, again) {
            (Outcome::Text(t), Outcome::Text(_)) => {
                println!("TEXT: {t}");
                assert!(t.to_lowercase().contains("sheep"));
            }
            _ => panic!("no speech detected"),
        }
        // silence must come back as NoSpeech, not hallucinated text
        assert!(matches!(engine.transcribe(Path::new(&model), &vec![0.0; 32000], &Opts { use_vad: true, ..Default::default() }).unwrap(), Outcome::NoSpeech));
    }
}
