//! Meeting notes: records your mic and what your speakers play (the other people
//! on the call) side by side, transcribes both every 20 seconds so the transcript
//! grows live, and at the end has the AI write the notes: summary, decisions,
//! action items. Everything lands in Notes.

use crate::engine::{Opts, Outcome};
use crate::{ai, audio, AppState};
use cpal::traits::{DeviceTrait, HostTrait};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const CHUNK: Duration = Duration::from_secs(20);

#[derive(Serialize, Clone, Default)]
pub struct MeetingStatus {
    pub running: bool,
    /// still writing the notes after you pressed stop
    pub finishing: bool,
    pub seconds: u64,
    pub transcript: String,
    /// false when only the mic could be recorded (no speaker loopback)
    pub hears_call: bool,
}

#[derive(Default)]
pub struct Meeting {
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<MeetingStatus>>,
    started: Mutex<Option<Instant>>,
}

impl Meeting {
    pub fn status(&self) -> MeetingStatus {
        let mut s = self.status.lock().clone();
        if s.running {
            s.seconds = self.started.lock().map(|t| t.elapsed().as_secs()).unwrap_or(0);
        }
        s
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub fn start(app: &AppHandle) -> anyhow::Result<()> {
    let st = app.state::<Arc<AppState>>().inner().clone();
    let m = &st.meeting;
    {
        let s = m.status.lock();
        if s.running || s.finishing {
            anyhow::bail!("A meeting is already being recorded");
        }
    }
    let settings = st.settings.get();
    let model = settings
        .model
        .as_deref()
        .and_then(|id| st.models.path_for(id))
        .ok_or_else(|| anyhow::anyhow!("Pick a speech model first"))?;
    m.stop.store(false, Ordering::SeqCst);
    *m.status.lock() = MeetingStatus { running: true, ..Default::default() };
    *m.started.lock() = Some(Instant::now());
    let (stop, status, app) = (m.stop.clone(), m.status.clone(), app.clone());
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<anyhow::Result<()>>();

    std::thread::Builder::new().name("baa-meeting".into()).spawn(move || {
        let mic_buf: Arc<Mutex<Vec<f32>>> = Arc::default();
        let call_buf: Arc<Mutex<Vec<f32>>> = Arc::default();
        let quiet: audio::LevelFn = Arc::new(|_| {});
        // cpal streams can't move between threads, so this thread owns both
        let mic = audio::pick_device(settings.microphone.as_deref())
            .and_then(|d| Ok((d.default_input_config()?, d)))
            .and_then(|(cfg, d)| audio::open_on(&d, cfg, mic_buf.clone(), quiet.clone()));
        let mic = match mic {
            Ok(s) => s,
            Err(e) => {
                status.lock().running = false;
                let _ = ready_tx.send(Err(anyhow::anyhow!("Mic problem: {e}")));
                return;
            }
        };
        let call = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("no speakers"))
            .and_then(|d| Ok((d.default_output_config()?, d)))
            .and_then(|(cfg, d)| audio::open_on(&d, cfg, call_buf.clone(), quiet.clone()));
        if let Err(e) = &call {
            eprintln!("baa: can't hear the call audio ({e}); recording the mic only");
        }
        status.lock().hears_call = call.is_ok();
        let _ = ready_tx.send(Ok(()));
        let _ = app.emit("meeting-changed", ());

        let st = app.state::<Arc<AppState>>().inner().clone();
        let opts = Opts { use_vad: true, language: settings.language.clone(), vocabulary: settings.custom_words.clone(), ..Default::default() };
        let hear = |pcm: &[f32]| -> Option<String> {
            match st.engine.transcribe(&model, pcm, &opts) {
                Ok(Outcome::Text(t)) => Some(t),
                Ok(Outcome::NoSpeech) => None,
                Err(e) => {
                    eprintln!("baa: meeting transcribe: {e}");
                    None
                }
            }
        };
        // one 20-second slice: the other side first (they usually speak, you answer)
        let slice = |status: &Mutex<MeetingStatus>| {
            let mine = audio::resample(&std::mem::take(&mut *mic_buf.lock()), mic.1);
            let theirs = match &call {
                Ok((_, rate)) => audio::resample(&std::mem::take(&mut *call_buf.lock()), *rate),
                Err(_) => vec![],
            };
            let mut lines = vec![];
            if let Some(t) = hear(&theirs) {
                lines.push(format!("Them: {t}"));
            }
            if let Some(t) = hear(&mine) {
                lines.push(format!("Me: {t}"));
            }
            if !lines.is_empty() {
                let mut s = status.lock();
                if !s.transcript.is_empty() {
                    s.transcript.push('\n');
                }
                s.transcript.push_str(&lines.join("\n"));
                drop(s);
                let _ = app.emit("meeting-changed", ());
            }
        };

        let mut last = Instant::now();
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(200));
            if last.elapsed() >= CHUNK {
                last = Instant::now();
                slice(&status);
            }
        }
        slice(&status);
        drop(call);
        drop(mic);
        {
            let mut s = status.lock();
            s.running = false;
            s.finishing = true;
        }
        let _ = app.emit("meeting-changed", ());

        let transcript = status.lock().transcript.clone();
        let when = chrono::Local::now().format("%b %-d, %-I:%M %p").to_string();
        let (title, body) = if transcript.trim().is_empty() {
            (format!("Meeting, {when}"), "Nothing was said.".to_string())
        } else {
            let s = st.settings.get();
            match ai::ready(&s.ai).then(|| ai::meeting_notes(&s.ai, &transcript)) {
                Some(Ok(notes)) => {
                    let (t, rest) = notes.split_once('\n').unwrap_or((notes.as_str(), ""));
                    let t = t.trim().trim_start_matches('#').trim();
                    (format!("{t}"), format!("{}\n\nTranscript\n{transcript}", rest.trim()))
                }
                Some(Err(e)) => {
                    eprintln!("baa: meeting notes: {e}");
                    (format!("Meeting, {when}"), format!("Transcript\n{transcript}"))
                }
                None => (format!("Meeting, {when}"), format!("Transcript\n{transcript}")),
            }
        };
        let id = st.history.save_note(None, &title, &body, "meeting").ok();
        status.lock().finishing = false;
        let _ = app.emit("meeting-changed", ());
        let _ = app.emit("meeting-done", id);
    })?;

    match ready_rx.recv_timeout(Duration::from_secs(8)) {
        Ok(r) => r,
        Err(_) => Ok(()), // slow audio driver; the thread reports its own errors
    }
}
