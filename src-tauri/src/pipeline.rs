//! The dictation pipeline and its shortcuts: press -> listen -> (live words) ->
//! release -> transcribe -> clean up -> paste. Command mode shares the same path
//! but sends the spoken instruction and your selection to the AI instead.

use crate::engine::Outcome;
use crate::settings::{AutoSend, Mode, Settings};
use crate::{ai, keyhook, learn, open_main, overlay, paste, text, AppState};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    Idle,
    Recording,
    Working,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Dictate,
    Command,
}

pub struct Pipe {
    pub phase: Phase,
    kind: Kind,
    pressed_at: Instant,
    started_at: Instant,
    /// true once a quick tap turned this recording into "toggle" (press again to stop)
    toggled: bool,
    /// bumps every recording, so a stale live-preview loop knows to quit
    session: u64,
}

impl Default for Pipe {
    fn default() -> Self {
        Pipe { phase: Phase::Idle, kind: Kind::Dictate, pressed_at: Instant::now(), started_at: Instant::now(), toggled: false, session: 0 }
    }
}

#[derive(Serialize, Clone)]
struct OverlayEvent {
    state: &'static str,
    message: Option<String>,
    kind: &'static str,
    sounds: bool,
}

fn state(app: &AppHandle) -> Arc<AppState> {
    app.state::<Arc<AppState>>().inner().clone()
}

pub fn overlay_state(app: &AppHandle, s: &'static str, message: Option<String>) {
    let st = state(app);
    let kind = if st.pipe.lock().kind == Kind::Command { "command" } else { "dictate" };
    let sounds = st.settings.get().sounds;
    let _ = app.emit("overlay-state", OverlayEvent { state: s, message, kind, sounds });
}

const TAP_MS: u128 = 350;
const ESCAPE: &str = "Escape";

pub fn on_shortcut(app: &AppHandle, kind: Kind, pressed: bool) {
    let st = state(app);
    let mode = st.settings.get().mode;
    let mut p = st.pipe.lock();
    if p.phase != Phase::Idle && p.kind != kind {
        return; // the other shortcut is mid-recording
    }
    match (pressed, p.phase) {
        (true, Phase::Idle) => {
            p.pressed_at = Instant::now();
            p.toggled = mode == Mode::Toggle;
            drop(p);
            start(app, &st, kind);
        }
        (true, Phase::Recording) if p.toggled => {
            drop(p);
            stop(app, &st);
        }
        (false, Phase::Recording) if !p.toggled => {
            let quick = p.pressed_at.elapsed().as_millis() < TAP_MS;
            if mode == Mode::Hybrid && quick {
                p.toggled = true; // a tap: keep listening until the next press
            } else {
                drop(p);
                stop(app, &st);
            }
        }
        _ => {}
    }
}

fn fail(app: &AppHandle, s: &Settings, msg: String, page: Option<&str>) {
    overlay::show(app, s);
    overlay_state(app, "error", Some(msg));
    hide_later(app, 2200);
    if page.is_some() {
        open_main(app, page);
    }
}

fn start(app: &AppHandle, st: &Arc<AppState>, kind: Kind) {
    let s = st.settings.get();
    let Some(model_path) = s.model.as_deref().and_then(|id| st.models.path_for(id)) else {
        return fail(app, &s, "Pick a speech model first".into(), Some("models"));
    };
    if kind == Kind::Command && !ai::ready(&s.ai) {
        return fail(app, &s, "Set up AI in Settings to use command mode".into(), Some("ai"));
    }
    if let Err(e) = st.recorder.start(s.microphone.clone()) {
        return fail(app, &s, format!("Mic problem: {e}"), None);
    }
    let session = {
        let mut p = st.pipe.lock();
        p.phase = Phase::Recording;
        p.kind = kind;
        p.started_at = Instant::now();
        p.session += 1;
        p.session
    };
    if s.show_overlay {
        overlay::show(app, &s);
    }
    overlay_state(app, "listen", None);
    if let Ok(sc) = ESCAPE.parse::<Shortcut>() {
        let _ = app.global_shortcut().register(sc);
    }
    // Warm the model while the user talks so it's ready when they stop.
    let engine = st.engine.clone();
    let path = model_path.clone();
    std::thread::spawn(move || {
        if let Err(e) = engine.ensure_loaded(&path) {
            eprintln!("baa: model load failed: {e}");
        }
    });
    if s.live_preview {
        let (app, st) = (app.clone(), st.clone());
        std::thread::spawn(move || live_preview(app, st, session, model_path, s.language));
    }
}

/// Re-transcribes what's been said so far every ~0.7 s and shows it in the bar.
/// On a GPU a pass over a few seconds of audio takes tens of milliseconds.
fn live_preview(app: AppHandle, st: Arc<AppState>, session: u64, path: std::path::PathBuf, language: Option<String>) {
    let mut last = String::new();
    loop {
        std::thread::sleep(Duration::from_millis(700));
        {
            let p = st.pipe.lock();
            if p.phase != Phase::Recording || p.session != session {
                return;
            }
        }
        let pcm = st.recorder.snapshot();
        let t0 = Instant::now();
        if let Some(t) = st.engine.preview(&path, &pcm, language.clone()) {
            if t != last && st.pipe.lock().session == session {
                let _ = app.emit("live-text", &t);
                last = t;
            }
        }
        // a slow (CPU) model shouldn't spend all its time on previews
        if t0.elapsed() > Duration::from_millis(600) {
            std::thread::sleep(t0.elapsed());
        }
    }
}

fn release_escape(app: &AppHandle) {
    if let Ok(sc) = ESCAPE.parse::<Shortcut>() {
        let _ = app.global_shortcut().unregister(sc);
    }
}

fn stop(app: &AppHandle, st: &Arc<AppState>) {
    let (audio_ms, kind) = {
        let mut p = st.pipe.lock();
        if p.phase != Phase::Recording {
            return;
        }
        p.phase = Phase::Working;
        (p.started_at.elapsed().as_millis() as u64, p.kind)
    };
    release_escape(app);
    let pcm = st.recorder.stop();
    overlay_state(app, "think", None);

    let (app, st) = (app.clone(), st.clone());
    std::thread::spawn(move || {
        let s = st.settings.get();
        let focused = paste::focused_app();
        let model_id = s.model.clone().unwrap_or_default();
        let result = match st.models.path_for(&model_id) {
            Some(path) => st.engine.transcribe(&path, &pcm, s.use_vad, s.language.clone(), s.custom_words.clone()),
            None => Err(anyhow::anyhow!("The selected model file is missing")),
        };
        let raw = match result {
            Ok(Outcome::Text(raw)) => raw,
            Ok(Outcome::NoSpeech) => {
                overlay_state(&app, "nothing", None);
                return finish(&app, &st, 1100);
            }
            Err(e) => {
                overlay_state(&app, "error", Some(e.to_string()));
                return finish(&app, &st, 2200);
            }
        };
        let done = match kind {
            Kind::Dictate => dictate(&app, &st, &s, raw, focused, audio_ms, &model_id),
            Kind::Command => command(&app, &st, &s, raw, focused, audio_ms, &model_id),
        };
        match done {
            Ok(()) => finish(&app, &st, 900),
            Err(e) => {
                overlay_state(&app, "error", Some(e.to_string()));
                finish(&app, &st, 2600);
            }
        }
    });
}

fn dictate(
    app: &AppHandle,
    st: &Arc<AppState>,
    s: &Settings,
    raw: String,
    focused: Option<String>,
    audio_ms: u64,
    model_id: &str,
) -> anyhow::Result<()> {
    let profile = focused.as_deref().and_then(|exe| s.app_profiles.iter().find(|p| p.exe.eq_ignore_ascii_case(exe)));
    let title = paste::focused_title();
    let mut spoken = raw.clone();
    if s.ai.cleanup && ai::ready(&s.ai) {
        overlay_state(app, "think", Some("Polishing…".into()));
        match ai::cleanup(&s.ai, &raw, focused.as_deref(), title.as_deref(), &s.custom_words) {
            Ok(t) => spoken = t,
            Err(e) => eprintln!("baa: AI cleanup skipped: {e}"), // paste what was said instead
        }
    }
    let out = text::process(&spoken, s, profile);
    let send = profile.and_then(|p| p.auto_send).unwrap_or(s.auto_send);
    overlay_state(app, "send", None);
    let html = if s.rich_paste && text::wants_rich_text(focused.as_deref(), title.as_deref()) { text::to_html(out.trim_end()) } else { None };
    paste::paste_with_html(&out, html.as_deref(), send, s.restore_clipboard).map_err(|e| anyhow::anyhow!("Couldn't paste: {e}"))?;
    let _ = st.history.add(out.trim_end(), &raw, focused.as_deref(), text::word_count(&out), audio_ms, Some(model_id));
    let _ = app.emit("history-changed", ());
    if s.learn_words {
        let app = app.clone();
        learn::watch(out.trim_end().to_string(), move |words| suggest_words(&app, words));
    }
    Ok(())
}

fn command(
    app: &AppHandle,
    st: &Arc<AppState>,
    s: &Settings,
    instruction: String,
    focused: Option<String>,
    audio_ms: u64,
    model_id: &str,
) -> anyhow::Result<()> {
    let selection = paste::copy_selection().unwrap_or(None);
    overlay_state(app, "think", Some(if selection.is_some() { "Rewriting…" } else { "Writing…" }.into()));
    let result = ai::command(&s.ai, &instruction, selection.as_deref())?;
    overlay_state(app, "send", None);
    // with text selected, pasting replaces it
    paste::paste(&result, AutoSend::Off, s.restore_clipboard)?;
    let note = format!("[{instruction}]");
    let _ = st.history.add(&result, &note, focused.as_deref(), text::word_count(&result), audio_ms, Some(model_id));
    let _ = app.emit("history-changed", ());
    Ok(())
}

fn suggest_words(app: &AppHandle, words: Vec<String>) {
    let st = state(app);
    let res = st.settings.update(|s| {
        for w in words {
            let known = s.custom_words.iter().chain(s.word_suggestions.iter()).any(|k| k.eq_ignore_ascii_case(&w));
            if !known {
                s.word_suggestions.push(w);
            }
        }
        let n = s.word_suggestions.len();
        if n > 30 {
            s.word_suggestions.drain(..n - 30);
        }
    });
    if res.is_ok() {
        let _ = app.emit("settings-changed", ());
    }
}

pub fn cancel(app: &AppHandle) {
    let st = state(app);
    {
        let mut p = st.pipe.lock();
        if p.phase != Phase::Recording {
            return;
        }
        p.phase = Phase::Idle;
    }
    release_escape(app);
    st.recorder.cancel();
    overlay_state(app, "cancel", None);
    hide_later(app, 700);
}

pub fn repaste(st: &AppState) -> anyhow::Result<()> {
    let text = st.history.last_text().ok_or_else(|| anyhow::anyhow!("Nothing dictated yet"))?;
    let s = st.settings.get();
    let text = if s.trailing_space { format!("{text} ") } else { text };
    paste::paste(&text, AutoSend::Off, s.restore_clipboard)
}

fn finish(app: &AppHandle, st: &Arc<AppState>, hide_ms: u64) {
    st.pipe.lock().phase = Phase::Idle;
    hide_later(app, hide_ms);
}

pub fn hide_later(app: &AppHandle, ms: u64) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        if state(&app).pipe.lock().phase == Phase::Idle {
            overlay_state(&app, "idle", None);
            overlay::hide(&app);
        }
    });
}

// ---------- shortcuts ----------

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Dictate,
    Command,
    Repaste,
}

static BOUND: Mutex<Vec<(Shortcut, Action)>> = Mutex::new(Vec::new());

/// Called by the global-shortcut plugin for every registered shortcut.
pub fn on_global_shortcut(app: &AppHandle, sc: &Shortcut, pressed: bool) {
    if ESCAPE.parse::<Shortcut>().ok().as_ref() == Some(sc) {
        if pressed {
            cancel(app);
        }
        return;
    }
    let action = BOUND.lock().iter().find(|(s, _)| s == sc).map(|(_, a)| *a);
    match action {
        Some(Action::Dictate) => on_shortcut(app, Kind::Dictate, pressed),
        Some(Action::Command) => on_shortcut(app, Kind::Command, pressed),
        Some(Action::Repaste) if pressed => {
            if let Err(e) = repaste(&state(app)) {
                eprintln!("baa: repaste: {e}");
            }
        }
        _ => {}
    }
}

/// (Re)binds all shortcuts from settings. A shortcut another app already owns
/// doesn't stop the others from binding; the error names every one that failed.
pub fn register(app: &AppHandle, s: &Settings) -> anyhow::Result<()> {
    let gs = app.global_shortcut();
    gs.unregister_all()?;
    BOUND.lock().clear();
    let mut problems: Vec<String> = vec![];
    let mut bind = |text: &str, action: Action, label: &str| {
        if text.trim().is_empty() {
            return;
        }
        let sc: Shortcut = match text.parse() {
            Ok(sc) => sc,
            Err(e) => return problems.push(format!("{label} shortcut \"{text}\" isn't valid ({e})")),
        };
        match gs.register(sc) {
            Ok(()) => BOUND.lock().push((sc, action)),
            Err(_) => problems.push(format!("{label} shortcut \"{text}\" is already used by another app")),
        }
    };
    // Modifier-only chords (Ctrl+Win) go through our keyboard hook instead.
    if let Some(mask) = keyhook::parse_modifier_only(&s.shortcut) {
        keyhook::set_chord(mask, chord_sender(app));
    } else {
        keyhook::set_chord(0, chord_sender(app));
        bind(&s.shortcut, Action::Dictate, "Dictation");
    }
    bind(&s.command_shortcut, Action::Command, "Command mode");
    bind(&s.repaste_shortcut, Action::Repaste, "Paste last");
    if problems.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("{}. Pick another in Settings.", problems.join("; "))
    }
}

/// The hook must return to Windows within milliseconds, so chord events are queued
/// to one worker thread that runs them in order.
fn chord_sender(app: &AppHandle) -> impl Fn(keyhook::ChordEvent) + Send + Sync + 'static {
    static TX: std::sync::OnceLock<Mutex<std::sync::mpsc::Sender<keyhook::ChordEvent>>> = std::sync::OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<keyhook::ChordEvent>();
        let app = app.clone();
        std::thread::spawn(move || {
            for e in rx {
                match e {
                    keyhook::ChordEvent::Pressed => on_shortcut(&app, Kind::Dictate, true),
                    keyhook::ChordEvent::Released => on_shortcut(&app, Kind::Dictate, false),
                    keyhook::ChordEvent::Aborted => cancel(&app),
                }
            }
        });
        Mutex::new(tx)
    });
    move |e| {
        let _ = tx.lock().send(e);
    }
}
