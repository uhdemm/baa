//! Baa: hold a shortcut, talk, and the words land in whatever app you're using.

mod audio;
mod engine;
mod history;
mod keyhook;
mod models;
mod overlay;
mod paste;
mod settings;
mod text;

use engine::{Engine, Outcome};
use history::History;
use models::Models;
use parking_lot::Mutex;
use serde::Serialize;
use settings::{Mode, Settings, SettingsStore};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Idle,
    Recording,
    Working,
}

struct Pipe {
    phase: Phase,
    pressed_at: Instant,
    started_at: Instant,
    /// true once a quick tap turned this recording into "toggle" (press again to stop)
    toggled: bool,
}

pub struct AppState {
    settings: SettingsStore,
    engine: Arc<Engine>,
    models: Arc<Models>,
    history: History,
    recorder: audio::Recorder,
    pipe: Mutex<Pipe>,
}

type S<'a> = State<'a, Arc<AppState>>;

#[derive(Serialize, Clone)]
struct OverlayEvent {
    state: &'static str,
    message: Option<String>,
}

fn overlay_state(app: &AppHandle, state: &'static str, message: Option<String>) {
    let _ = app.emit("overlay-state", OverlayEvent { state, message });
}

const TAP_MS: u128 = 350;
const ESCAPE: &str = "Escape";

// ---------- the dictation pipeline ----------

fn on_shortcut(app: &AppHandle, pressed: bool) {
    let st = app.state::<Arc<AppState>>().inner().clone();
    let mode = st.settings.get().mode;
    let mut p = st.pipe.lock();
    match (pressed, p.phase) {
        (true, Phase::Idle) => {
            p.pressed_at = Instant::now();
            p.toggled = mode == Mode::Toggle;
            drop(p);
            start_recording(app, &st);
        }
        (true, Phase::Recording) if p.toggled => {
            drop(p);
            stop_recording(app, &st);
        }
        (false, Phase::Recording) if !p.toggled => {
            let quick = p.pressed_at.elapsed().as_millis() < TAP_MS;
            if mode == Mode::Hybrid && quick {
                p.toggled = true; // a tap: keep listening until the next press
            } else {
                drop(p);
                stop_recording(app, &st);
            }
        }
        _ => {}
    }
}

fn start_recording(app: &AppHandle, st: &Arc<AppState>) {
    let s = st.settings.get();
    let Some(model_path) = s.model.as_deref().and_then(|id| st.models.path_for(id)) else {
        overlay::show(app, &s);
        overlay_state(app, "error", Some("Pick a speech model first".into()));
        hide_later(app, 1800);
        open_main(app, Some("models"));
        return;
    };
    if let Err(e) = st.recorder.start(s.microphone.clone()) {
        overlay::show(app, &s);
        overlay_state(app, "error", Some(format!("Mic problem: {e}")));
        hide_later(app, 2200);
        return;
    }
    {
        let mut p = st.pipe.lock();
        p.phase = Phase::Recording;
        p.started_at = Instant::now();
    }
    if s.show_overlay {
        overlay::show(app, &s);
    }
    overlay_state(app, "listen", None);
    if let Ok(sc) = ESCAPE.parse::<Shortcut>() {
        let _ = app.global_shortcut().register(sc);
    }
    // Warm the model while the user talks so it's ready when they stop.
    let engine = st.engine.clone();
    std::thread::spawn(move || {
        if let Err(e) = engine.ensure_loaded(&model_path) {
            eprintln!("baa: model load failed: {e}");
        }
    });
}

fn release_escape(app: &AppHandle) {
    if let Ok(sc) = ESCAPE.parse::<Shortcut>() {
        let _ = app.global_shortcut().unregister(sc);
    }
}

fn stop_recording(app: &AppHandle, st: &Arc<AppState>) {
    let audio_ms = {
        let mut p = st.pipe.lock();
        if p.phase != Phase::Recording {
            return;
        }
        p.phase = Phase::Working;
        p.started_at.elapsed().as_millis() as u64
    };
    release_escape(app);
    let pcm = st.recorder.stop();
    overlay_state(app, "think", None);

    let app = app.clone();
    let st = st.clone();
    std::thread::spawn(move || {
        let s = st.settings.get();
        let focused = paste::focused_app();
        let profile = focused.as_deref().and_then(|exe| s.app_profiles.iter().find(|p| p.exe.eq_ignore_ascii_case(exe)));
        let model_id = s.model.clone().unwrap_or_default();
        let result = match st.models.path_for(&model_id) {
            Some(path) => st.engine.transcribe(&path, &pcm, s.use_vad, s.language.clone(), s.custom_words.clone()),
            None => Err(anyhow::anyhow!("The selected model file is missing")),
        };
        match result {
            Ok(Outcome::Text(raw)) => {
                let out = text::process(&raw, &s, profile);
                let send = profile.and_then(|p| p.auto_send).unwrap_or(s.auto_send);
                overlay_state(&app, "send", None);
                if let Err(e) = paste::paste(&out, send, s.restore_clipboard) {
                    overlay_state(&app, "error", Some(format!("Couldn't paste: {e}")));
                }
                let _ = st.history.add(out.trim_end(), &raw, focused.as_deref(), text::word_count(&out), audio_ms, Some(&model_id));
                let _ = app.emit("history-changed", ());
                finish(&app, &st, 900);
            }
            Ok(Outcome::NoSpeech) => {
                overlay_state(&app, "nothing", None);
                finish(&app, &st, 1100);
            }
            Err(e) => {
                overlay_state(&app, "error", Some(e.to_string()));
                finish(&app, &st, 2200);
            }
        }
    });
}

fn cancel_recording(app: &AppHandle) {
    let st = app.state::<Arc<AppState>>().inner().clone();
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

fn finish(app: &AppHandle, st: &Arc<AppState>, hide_ms: u64) {
    st.pipe.lock().phase = Phase::Idle;
    hide_later(app, hide_ms);
}

fn hide_later(app: &AppHandle, ms: u64) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        let st = app.state::<Arc<AppState>>();
        if st.pipe.lock().phase == Phase::Idle {
            overlay_state(&app, "idle", None);
            overlay::hide(&app);
        }
    });
}

fn register_shortcut(app: &AppHandle, shortcut: &str) -> anyhow::Result<()> {
    let gs = app.global_shortcut();
    gs.unregister_all()?;
    // Modifier-only chords (Ctrl+Win) go through our keyboard hook instead.
    if let Some(mask) = keyhook::parse_modifier_only(shortcut) {
        keyhook::set_chord(mask, chord_sender(app));
        return Ok(());
    }
    keyhook::set_chord(0, chord_sender(app));
    let sc: Shortcut = shortcut.parse().map_err(|e| anyhow::anyhow!("Invalid shortcut \"{shortcut}\": {e}"))?;
    gs.register(sc)?;
    Ok(())
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
                    keyhook::ChordEvent::Pressed => on_shortcut(&app, true),
                    keyhook::ChordEvent::Released => on_shortcut(&app, false),
                    keyhook::ChordEvent::Aborted => cancel_recording(&app),
                }
            }
        });
        Mutex::new(tx)
    });
    move |e| {
        let _ = tx.lock().send(e);
    }
}

// ---------- windows ----------

fn open_main(app: &AppHandle, page: Option<&str>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(p) = page {
            let _ = app.emit("navigate", p);
        }
        return;
    }
    let url = match page {
        Some(p) => format!("index.html#{p}"),
        None => "index.html".into(),
    };
    let _ = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
        .title("Baa")
        .inner_size(980.0, 680.0)
        .min_inner_size(760.0, 520.0)
        .center()
        .build();
}

// ---------- commands for the settings window ----------

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
fn get_settings(st: S) -> Settings {
    st.settings.get()
}

#[tauri::command]
fn set_settings(app: AppHandle, st: S, settings: Settings) -> Result<Settings, String> {
    let old = st.settings.get();
    if old.shortcut != settings.shortcut {
        if let Err(e) = register_shortcut(&app, &settings.shortcut) {
            let _ = register_shortcut(&app, &old.shortcut);
            return Err(e.to_string());
        }
    }
    st.settings.set(settings.clone()).map_err(err)?;
    Ok(settings)
}

#[tauri::command]
fn list_models(st: S) -> Vec<models::ModelRow> {
    st.models.list(&st.settings.get().imported_models)
}

#[tauri::command]
async fn download_model(app: AppHandle, st: S<'_>, id: String) -> Result<(), String> {
    let models = st.models.clone();
    let res = models.download(app.clone(), id.clone()).await;
    // First model ever downloaded becomes the active one.
    if res.is_ok() && st.settings.get().model.is_none() {
        let _ = st.settings.update(|s| s.model = Some(id));
        let _ = app.emit("settings-changed", ());
    }
    res.map_err(err)
}

#[tauri::command]
fn cancel_download(st: S, id: String) {
    st.models.cancel(&id);
}

#[tauri::command]
fn delete_model(app: AppHandle, st: S, id: String) -> Result<(), String> {
    if st.settings.get().model.as_deref() == Some(&id) {
        st.engine.unload();
    }
    if let Some(path) = id.strip_prefix("file:") {
        // imported models are only forgotten, never deleted from the user's disk
        let path = path.to_string();
        st.settings.update(|s| s.imported_models.retain(|p| p != &path)).map_err(err)?;
    } else {
        st.models.delete(&id).map_err(err)?;
    }
    if st.settings.get().model.as_deref() == Some(&id) {
        st.settings.update(|s| s.model = None).map_err(err)?;
    }
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
fn import_model(app: AppHandle, st: S, path: String) -> Result<String, String> {
    if !path.to_lowercase().ends_with(".gguf") && !path.to_lowercase().ends_with(".bin") {
        return Err("Pick a .gguf (or whisper.cpp .bin) model file".into());
    }
    // Make sure transcribe.cpp can actually open it before we list it.
    transcribe_cpp::Model::load(&path).map_err(|e| format!("That file isn't a model Baa can load: {e}"))?;
    let id = format!("file:{path}");
    st.settings
        .update(|s| {
            if !s.imported_models.contains(&path) {
                s.imported_models.push(path.clone());
            }
            s.model = Some(id.clone());
        })
        .map_err(err)?;
    let _ = app.emit("settings-changed", ());
    Ok(id)
}

#[tauri::command]
fn engine_status(st: S) -> engine::EngineStatus {
    st.engine.status()
}

#[tauri::command]
fn list_microphones() -> Vec<String> {
    audio::list_inputs()
}

#[tauri::command]
fn history_list(st: S, limit: i64, offset: i64, query: Option<String>) -> Result<Vec<history::Entry>, String> {
    st.history.list(limit, offset, query).map_err(err)
}

#[tauri::command]
fn history_delete(st: S, id: i64) -> Result<(), String> {
    st.history.delete(id).map_err(err)
}

#[tauri::command]
fn history_clear(st: S) -> Result<(), String> {
    st.history.clear().map_err(err)
}

#[tauri::command]
fn stats(st: S) -> Result<history::Stats, String> {
    st.history.stats().map_err(err)
}

#[tauri::command]
fn repaste_last(st: S) -> Result<(), String> {
    let text = st.history.last_text().ok_or("Nothing dictated yet")?;
    let s = st.settings.get();
    paste::paste(&format!("{text}{}", if s.trailing_space { " " } else { "" }), settings::AutoSend::Off, s.restore_clipboard)
        .map_err(err)
}

#[tauri::command]
fn preview_text(st: S, raw: String) -> String {
    text::process(&raw, &st.settings.get(), None)
}

#[tauri::command]
fn focused_app_name() -> Option<String> {
    paste::focused_app()
}

// ---------- startup ----------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_main(app, None)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    let pressed = event.state() == ShortcutState::Pressed;
                    let esc: Option<Shortcut> = ESCAPE.parse().ok();
                    if Some(*shortcut) == esc {
                        if pressed {
                            cancel_recording(app);
                        }
                    } else {
                        on_shortcut(app, pressed);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data)?;
            let vad_path = app
                .path()
                .resolve("resources/silero-vad.bin", tauri::path::BaseDirectory::Resource)?;

            let level_app = handle.clone();
            let state = Arc::new(AppState {
                settings: SettingsStore::load(&data),
                engine: Engine::new(vad_path),
                models: Models::new(data.join("models")),
                history: History::open(&data.join("history.db"))?,
                recorder: audio::Recorder::new(Arc::new(move |lvl| {
                    let _ = level_app.emit("mic-level", lvl);
                })),
                pipe: Mutex::new(Pipe { phase: Phase::Idle, pressed_at: Instant::now(), started_at: Instant::now(), toggled: false }),
            });
            app.manage(state.clone());

            let s = state.settings.get();
            if let Err(e) = register_shortcut(&handle, &s.shortcut) {
                eprintln!("baa: {e}; falling back to Ctrl+Space");
                let _ = register_shortcut(&handle, "Ctrl+Space");
            }
            overlay::create(&handle)?;

            // Tray: left click opens Baa, right click shows the menu.
            let open_i = MenuItem::with_id(app, "open", "Open Baa", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit Baa", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_i, &quit_i])?;
            TrayIconBuilder::with_id("baa")
                .icon(app.default_window_icon().cloned().expect("app icon"))
                .tooltip("Baa")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => open_main(app, None),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        open_main(tray.app_handle(), None);
                    }
                })
                .build(app)?;

            // Free the model after it sits idle, so Baa stays light in the background.
            let st = state.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(10));
                let after = Duration::from_secs(st.settings.get().unload_after_secs);
                if st.pipe.lock().phase == Phase::Idle && st.engine.unload_if_idle(after) {
                    eprintln!("baa: model unloaded after idle");
                }
            });

            let hidden = std::env::args().any(|a| a == "--hidden");
            if !hidden || !s.onboarded {
                open_main(&handle, if s.model.is_none() { Some("models") } else { None });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            list_models,
            download_model,
            cancel_download,
            delete_model,
            import_model,
            engine_status,
            list_microphones,
            history_list,
            history_delete,
            history_clear,
            stats,
            repaste_last,
            preview_text,
            focused_app_name
        ])
        .build(tauri::generate_context!())
        .expect("error while building Baa")
        .run(|_app, event| {
            // Closing the settings window frees its memory; Baa keeps running in the tray.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
