//! Meeting notes. Records your mic and what your speakers play (everyone else on
//! the call) side by side:
//! - every 20 seconds both are transcribed so the transcript grows live; the mic is
//!   cleaned of the call's echo first, so "Me" is only ever you (echo.rs);
//! - both tracks are kept on disk, and when you stop, a speaker model (Sortformer)
//!   works out who said what on the call: Speaker 1..4, even over each other. Each
//!   turn is transcribed on its own and merged with yours by time;
//! - the AI names the speakers from the attendee list and what was said, and writes
//!   the notes (summary, decisions, action items) in the chosen template.
//! Everything lands in Notes; the audio is deleted afterwards.

use crate::engine::{Opts, Outcome};
use crate::{ai, audio, echo, AppState};
use cpal::traits::{DeviceTrait, HostTrait};
use parking_lot::Mutex;
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const CHUNK: Duration = Duration::from_secs(20);
const RATE: usize = 16_000;
pub const SPEAKER_MODEL: &str = "sortformer-4spk";

#[derive(Serialize, Clone, Default)]
pub struct MeetingStatus {
    pub running: bool,
    /// still writing the notes after you pressed stop
    pub finishing: bool,
    /// what the finishing step is doing right now
    pub stage: String,
    pub seconds: u64,
    pub transcript: String,
    /// false when only the mic could be recorded (no speaker loopback)
    pub hears_call: bool,
    /// the speaker model is installed, so people will be told apart
    pub speakers: bool,
    /// filled in by you, any time during the meeting
    pub attendees: String,
    pub my_notes: String,
    /// "general" | "one_on_one" | "sales" | "standup" | "interview"
    pub template: String,
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

    /// Attendees, your own notes and the template can change while it records.
    pub fn update(&self, attendees: String, my_notes: String, template: String) {
        let mut s = self.status.lock();
        s.attendees = attendees;
        s.my_notes = my_notes;
        s.template = template;
    }
}

fn stamp(ms: i64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

// ---------- 16-bit WAV on disk, so an hour-long call doesn't sit in memory ----------

struct Wav {
    file: std::fs::File,
    samples: usize,
}

impl Wav {
    fn create(path: &Path) -> std::io::Result<Wav> {
        let mut file = std::fs::File::create(path)?;
        file.write_all(&[0u8; 44])?; // header written on finish
        Ok(Wav { file, samples: 0 })
    }

    fn push(&mut self, pcm: &[f32]) -> std::io::Result<()> {
        let bytes: Vec<u8> = pcm.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        self.samples += pcm.len();
        self.file.write_all(&bytes)
    }

    fn finish(&mut self) -> std::io::Result<()> {
        let data = (self.samples * 2) as u32;
        let mut h = Vec::with_capacity(44);
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&(36 + data).to_le_bytes());
        h.extend_from_slice(b"WAVEfmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes()); // PCM
        h.extend_from_slice(&1u16.to_le_bytes()); // mono
        h.extend_from_slice(&(RATE as u32).to_le_bytes());
        h.extend_from_slice(&(RATE as u32 * 2).to_le_bytes());
        h.extend_from_slice(&2u16.to_le_bytes());
        h.extend_from_slice(&16u16.to_le_bytes());
        h.extend_from_slice(b"data");
        h.extend_from_slice(&data.to_le_bytes());
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&h)?;
        self.file.flush()
    }
}

fn read_wav(path: &Path) -> Vec<f32> {
    let mut b = vec![];
    if std::fs::File::open(path).and_then(|mut f| f.read_to_end(&mut b)).is_err() || b.len() < 44 {
        return vec![];
    }
    b[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect()
}

// ---------- putting the transcript together ----------

/// One stretch of speech: when it starts, who, what.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub t0_ms: i64,
    pub who: String,
    pub text: String,
}

/// Sorts everyone's lines by time and joins back-to-back lines from the same person.
pub fn assemble(mut lines: Vec<Line>) -> String {
    lines.retain(|l| !l.text.trim().is_empty());
    lines.sort_by_key(|l| l.t0_ms);
    let mut merged: Vec<Line> = vec![];
    for l in lines {
        match merged.last_mut() {
            Some(prev) if prev.who == l.who && l.t0_ms - prev.t0_ms < 60_000 => {
                prev.text.push(' ');
                prev.text.push_str(l.text.trim());
            }
            _ => merged.push(Line { text: l.text.trim().to_string(), ..l }),
        }
    }
    merged.iter().map(|l| format!("[{}] {}: {}", stamp(l.t0_ms), l.who, l.text)).collect::<Vec<_>>().join("\n")
}

/// Speaker turns from the diarizer: same person with a short gap becomes one turn.
/// Overlapping turns of different people stay separate (both get written down).
pub fn turns(mut segs: Vec<(i64, i64, i32)>) -> Vec<(i64, i64, i32)> {
    segs.sort_by_key(|s| s.0);
    let mut out: Vec<(i64, i64, i32)> = vec![];
    for (t0, t1, id) in segs {
        if let Some(prev) = out.iter_mut().rev().find(|p| p.2 == id) {
            if t0 - prev.1 < 700 {
                prev.1 = prev.1.max(t1);
                continue;
            }
        }
        out.push((t0, t1, id));
    }
    out.sort_by_key(|s| s.0);
    out.into_iter().filter(|(a, b, _)| b - a >= 300).collect()
}

/// Reads the AI's "SPEAKERS: Speaker 1 = Brad; Speaker 2 = unknown" line.
pub fn parse_speakers(line: &str) -> Vec<(String, String)> {
    let Some(rest) = line.trim().strip_prefix("SPEAKERS:") else { return vec![] };
    rest.split([';', ','])
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            let (k, v) = (k.trim(), v.trim().trim_matches('"'));
            let ok = k.starts_with("Speaker ") && !v.is_empty() && !v.eq_ignore_ascii_case("unknown") && !v.eq_ignore_ascii_case("me");
            ok.then(|| (k.to_string(), v.to_string()))
        })
        .collect()
}

/// Swaps "Speaker 2" for the name everywhere (transcript labels and the notes).
pub fn relabel(text: &str, names: &[(String, String)]) -> String {
    let mut t = text.to_string();
    // longest labels first so "Speaker 1" never eats part of "Speaker 10"
    let mut names = names.to_vec();
    names.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    for (k, v) in names {
        t = t.replace(&k, &v);
    }
    t
}

// ---------- recording ----------

pub fn start(app: &AppHandle) -> anyhow::Result<()> {
    let st = app.state::<Arc<AppState>>().inner().clone();
    let m = &st.meeting;
    let keep = {
        let s = m.status.lock();
        if s.running || s.finishing {
            anyhow::bail!("A meeting is already being recorded");
        }
        (s.attendees.clone(), s.template.clone())
    };
    let settings = st.settings.get();
    let model = settings
        .model
        .as_deref()
        .and_then(|id| st.models.path_for(id))
        .ok_or_else(|| anyhow::anyhow!("Pick a speech model first"))?;
    let dir = st.models.dir.parent().unwrap_or(&st.models.dir).join("meetings").join(chrono::Local::now().format("%Y%m%d-%H%M%S").to_string());
    std::fs::create_dir_all(&dir)?;
    m.stop.store(false, Ordering::SeqCst);
    *m.status.lock() = MeetingStatus {
        running: true,
        speakers: st.models.path_for(SPEAKER_MODEL).is_some(),
        attendees: keep.0,
        template: if keep.1.is_empty() { "general".into() } else { keep.1 },
        ..Default::default()
    };
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
        let (mut me_wav, mut them_wav) = match (Wav::create(&dir.join("me.wav")), Wav::create(&dir.join("them.wav"))) {
            (Ok(a), Ok(b)) => (a, b),
            _ => {
                status.lock().running = false;
                let _ = ready_tx.send(Err(anyhow::anyhow!("Can't save the meeting audio")));
                return;
            }
        };
        let _ = ready_tx.send(Ok(()));
        let _ = app.emit("meeting-changed", ());

        let st = app.state::<Arc<AppState>>().inner().clone();
        let words = |status: &Mutex<MeetingStatus>| {
            let mut v = settings.custom_words.clone();
            v.extend(status.lock().attendees.split([',', '\n']).map(|n| n.trim().to_string()).filter(|n| !n.is_empty()));
            v
        };
        let hear = |pcm: &[f32], vad: bool, vocab: Vec<String>| -> Option<String> {
            let opts = Opts { use_vad: vad, language: settings.language.clone(), vocabulary: vocab, ..Default::default() };
            match st.engine.transcribe(&model, pcm, &opts) {
                Ok(Outcome::Text(t)) => Some(t),
                Ok(Outcome::NoSpeech) => None,
                Err(e) => {
                    eprintln!("baa: meeting transcribe: {e}");
                    None
                }
            }
        };

        // one 20-second slice: echo out of the mic, both tracks to disk, live lines
        let mut gate = echo::Gate::default();
        let mut slice = |status: &Mutex<MeetingStatus>| {
            let mut theirs = match &call {
                Ok((_, rate)) => audio::resample(&std::mem::take(&mut *call_buf.lock()), *rate),
                Err(_) => vec![],
            };
            let raw_mine = audio::resample(&std::mem::take(&mut *mic_buf.lock()), mic.1);
            // keep both tracks the same length so their clocks stay together
            let len = raw_mine.len().max(theirs.len());
            theirs.resize(len, 0.0);
            let mut mine = if call.is_ok() { gate.clean(&raw_mine, &theirs) } else { raw_mine };
            mine.resize(len, 0.0);
            let t0 = (me_wav.samples * 1000 / RATE) as i64;
            let _ = me_wav.push(&mine);
            let _ = them_wav.push(&theirs);
            let vocab = words(status);
            let mut lines = vec![];
            if let Some(t) = hear(&theirs, true, vocab.clone()) {
                lines.push(Line { t0_ms: t0, who: "Them".into(), text: t });
            }
            if let Some(t) = hear(&mine, true, vocab) {
                lines.push(Line { t0_ms: t0 + 1, who: "Me".into(), text: t });
            }
            if !lines.is_empty() {
                let mut s = status.lock();
                if !s.transcript.is_empty() {
                    s.transcript.push('\n');
                }
                s.transcript.push_str(&assemble(lines));
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
        let _ = me_wav.finish();
        let _ = them_wav.finish();
        let set_stage = |stage: &str| {
            let mut s = status.lock();
            s.running = false;
            s.finishing = true;
            s.stage = stage.into();
            drop(s);
            let _ = app.emit("meeting-changed", ());
        };
        set_stage("Working out who said what…");

        let live = status.lock().transcript.clone();
        let hears_call = status.lock().hears_call;
        let mut transcript = match st.models.path_for(SPEAKER_MODEL).filter(|_| hears_call) {
            Some(diar) => match who_said_what(&st, &diar, &dir, |pcm, vocab| hear(pcm, false, vocab), words(&status)) {
                Ok(t) if !t.trim().is_empty() => t,
                Ok(_) => live.clone(),
                Err(e) => {
                    eprintln!("baa: speaker detection failed ({e}); using the live transcript");
                    live.clone()
                }
            },
            None => live.clone(),
        };

        set_stage("Writing your notes…");
        let when = chrono::Local::now().format("%b %-d, %-I:%M %p").to_string();
        let (attendees, my_notes, template) = {
            let s = status.lock();
            (s.attendees.clone(), s.my_notes.clone(), s.template.clone())
        };
        let s = st.settings.get();
        let (title, notes) = if transcript.trim().is_empty() {
            (format!("Meeting, {when}"), "Nothing was said.".to_string())
        } else if ai::ready(&s.ai) {
            match ai::meeting_notes(&s.ai, &transcript, &attendees, &my_notes, &template) {
                Ok(out) => {
                    let mut lines = out.lines();
                    let mut first = lines.next().unwrap_or("").to_string();
                    let names = parse_speakers(&first);
                    if first.trim().starts_with("SPEAKERS:") {
                        first = lines.next().unwrap_or("").to_string();
                        transcript = relabel(&transcript, &names);
                    }
                    let rest = relabel(&lines.collect::<Vec<_>>().join("\n"), &names);
                    let t = first.trim().trim_start_matches('#').trim().to_string();
                    (if t.is_empty() { format!("Meeting, {when}") } else { t }, rest.trim().to_string())
                }
                Err(e) => {
                    eprintln!("baa: meeting notes: {e}");
                    (format!("Meeting, {when}"), String::new())
                }
            }
        } else {
            (format!("Meeting, {when}"), String::new())
        };
        let mut body = String::new();
        if !my_notes.trim().is_empty() && notes.is_empty() {
            body.push_str(&format!("My notes\n{}\n\n", my_notes.trim()));
        }
        if !notes.is_empty() {
            body.push_str(&notes);
            body.push_str("\n\n");
        }
        body.push_str("Transcript\n");
        body.push_str(&transcript);
        let id = st.history.save_note(None, &title, &body, "meeting").ok();
        let _ = std::fs::remove_dir_all(&dir); // the audio was only needed to build this
        {
            let mut s = status.lock();
            s.finishing = false;
            s.stage.clear();
            s.my_notes.clear();
        }
        let _ = app.emit("meeting-changed", ());
        let _ = app.emit("meeting-done", id);
    })?;

    match ready_rx.recv_timeout(Duration::from_secs(8)) {
        Ok(r) => r,
        Err(_) => Ok(()), // slow audio driver; the thread reports its own errors
    }
}

/// The full-quality pass: Sortformer splits the call into speakers, every turn is
/// transcribed on its own, and your own speech is cut into turns by the VAD.
fn who_said_what(
    st: &AppState,
    diar_path: &Path,
    dir: &PathBuf,
    hear: impl Fn(&[f32], Vec<String>) -> Option<String>,
    vocab: Vec<String>,
) -> anyhow::Result<String> {
    use transcribe_cpp::{DiarizeExtension, DiarizeOptions, Model, SortformerDiarizeOptions, SortformerPreset};
    let them = read_wav(&dir.join("them.wav"));
    let me = read_wav(&dir.join("me.wav"));
    let ms = |i: usize| (i * 1000 / RATE) as i64;
    let at = |t: i64| ((t.max(0) as usize) * RATE / 1000).min(them.len());
    let mut lines: Vec<Line> = vec![];

    if them.iter().any(|s| s.abs() > 0.01) {
        let model = Model::load(diar_path)?;
        let mut session = model.diarize_session()?;
        let opts = DiarizeOptions {
            // the whole call is already recorded: use the most accurate setting
            family: Some(DiarizeExtension::Sortformer(SortformerDiarizeOptions { preset: Some(SortformerPreset::VeryHighLatency) })),
        };
        let segs = session.run(&them, &opts)?;
        drop(session);
        drop(model);
        for (t0, t1, id) in turns(segs.iter().map(|s| (s.t0_ms, s.t1_ms, s.speaker_id)).collect()) {
            // long turns go in ~25 s pieces; a little padding keeps first and last words
            let mut a = t0;
            while a < t1 {
                let b = (a + 25_000).min(t1);
                if let Some(text) = hear(&them[at(a - 150)..at(b + 150)], vocab.clone()) {
                    lines.push(Line { t0_ms: a, who: format!("Speaker {id}"), text });
                }
                a = b;
            }
        }
    }

    // your side: speech spans, close ones joined, each transcribed
    let mut spans: Vec<(usize, usize)> = vec![];
    for (a, b) in st.engine.speech_spans(&me, 0.5).unwrap_or_default() {
        match spans.last_mut() {
            Some(p) if a - p.1 < RATE * 8 / 10 && b - p.0 < RATE * 25 => p.1 = b,
            _ => spans.push((a, b)),
        }
    }
    for (a, b) in spans {
        if let Some(text) = hear(&me[a..b], vocab.clone()) {
            lines.push(Line { t0_ms: ms(a), who: "Me".into(), text });
        }
    }
    Ok(assemble(lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_is_in_time_order_and_merged() {
        let l = |t, w: &str, x: &str| Line { t0_ms: t, who: w.into(), text: x.into() };
        let out = assemble(vec![l(5_000, "Me", "yes."), l(1_000, "Speaker 1", "Hi."), l(2_500, "Speaker 1", "Ready?"), l(65_000, "Speaker 2", "Hello.")]);
        assert_eq!(out, "[0:01] Speaker 1: Hi. Ready?\n[0:05] Me: yes.\n[1:05] Speaker 2: Hello.");
    }

    #[test]
    fn turns_join_short_gaps_and_keep_overlaps() {
        let t = turns(vec![(0, 1000, 1), (1300, 2000, 1), (1500, 2600, 2), (5000, 5100, 1)]);
        assert_eq!(t, vec![(0, 2000, 1), (1500, 2600, 2)]);
    }

    #[test]
    fn names_from_the_ai() {
        let names = parse_speakers("SPEAKERS: Speaker 1 = Brad; Speaker 2 = unknown; Speaker 3 = Sarah");
        assert_eq!(names, vec![("Speaker 1".to_string(), "Brad".to_string()), ("Speaker 3".to_string(), "Sarah".to_string())]);
        assert_eq!(relabel("[0:01] Speaker 1: hi\n- Speaker 3 sends it", &names), "[0:01] Brad: hi\n- Sarah sends it");
    }
}
