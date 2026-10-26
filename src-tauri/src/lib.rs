//! Baa: hold a shortcut, talk, and the words land in whatever app you're using.

mod ai;
mod audio;
mod buddy;
mod characters;
mod echo;
mod engine;
mod history;
mod keyhook;
mod learn;
mod meeting;
mod models;
mod overlay;
mod paste;
mod pipeline;
mod quiet;
mod settings;
pub mod studio;
mod text;

use engine::Engine;
use history::History;
use models::Models;
use parking_lot::Mutex;
use pipeline::{Phase, Pipe};
use serde::Serialize;
use settings::{Settings, SettingsStore};
use std::sync::Arc;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::ShortcutState;

pub struct AppState {
    settings: SettingsStore,
    engine: Arc<Engine>,
    models: Arc<Models>,
    history: History,
    recorder: audio::Recorder,
    pipe: Mutex<Pipe>,
    /// the setup screen's mic test is running
    mic_test: Mutex<bool>,
    /// e.g. a shortcut another app owns; shown in the settings window
    startup_warning: Mutex<Option<String>>,
    meeting: meeting::Meeting,
}

type S<'a> = State<'a, Arc<AppState>>;

// ---------- windows ----------

pub(crate) fn open_main(app: &AppHandle, page: Option<&str>) {
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
    let look = app.state::<Arc<AppState>>().settings.get().appearance;
    let built = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
        .title("Baa")
        // the design system is light-only
        .theme(Some(tauri::Theme::Light))
        // Apple-style window: our own title bar with the window dots, rounded corners
        .decorations(false)
        .shadow(true)
        .transparent(true)
        .effects(window_effects(look.wallpaper_through))
        .inner_size(980.0, 680.0)
        .min_inner_size(760.0, 520.0)
        .center()
        .build();
    #[cfg(windows)]
    if let Ok(w) = built {
        round_corners(&w);
    }
}

/// Windows 11 Mica in its light tint: the wallpaper glows softly through the sidebar.
fn window_effects(on: bool) -> tauri::utils::config::WindowEffectsConfig {
    tauri::utils::config::WindowEffectsConfig {
        effects: if on { vec![tauri::window::Effect::MicaLight] } else { vec![] },
        ..Default::default()
    }
}

/// Frameless windows are square by default; ask Windows 11 for its rounded corners.
#[cfg(windows)]
fn round_corners(w: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
    if let Ok(h) = w.hwnd() {
        let pref = DWMWCP_ROUND;
        unsafe {
            let _ = DwmSetWindowAttribute(HWND(h.0 as _), DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const _ as _, std::mem::size_of_val(&pref) as u32);
        }
    }
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
fn set_settings(app: AppHandle, st: S, mut settings: Settings) -> Result<Settings, String> {
    let old = st.settings.get();
    // where the buddy sits is set by dragging it, not by the settings window's copy
    settings.buddy_x = old.buddy_x;
    settings.buddy_y = old.buddy_y;
    let keys_changed = old.shortcut != settings.shortcut
        || old.command_shortcut != settings.command_shortcut
        || old.repaste_shortcut != settings.repaste_shortcut
        || old.mouse_button != settings.mouse_button;
    if keys_changed {
        if let Err(e) = pipeline::register(&app, &settings) {
            let _ = pipeline::register(&app, &old);
            return Err(e.to_string());
        }
        *st.startup_warning.lock() = None;
    }
    if old.appearance.wallpaper_through != settings.appearance.wallpaper_through {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_effects(window_effects(settings.appearance.wallpaper_through));
        }
    }
    st.settings.set(settings.clone()).map_err(err)?;
    // the pop-up restyles itself live when the look changes
    let _ = app.emit("appearance-changed", &settings.appearance);
    let buddy_changed = old.overlay_style != settings.overlay_style
        || old.buddy_place != settings.buddy_place
        || old.buddy_size != settings.buddy_size
        || old.overlay_position != settings.overlay_position;
    if buddy_changed {
        buddy::sync(&app, &settings);
    }
    if buddy_changed || old.character != settings.character || old.buddy_sleeps != settings.buddy_sleeps {
        let _ = app.emit("buddy-changed", &settings);
    }
    Ok(settings)
}

#[tauri::command]
fn startup_warning(st: S) -> Option<String> {
    st.startup_warning.lock().clone()
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
    let lower = path.to_lowercase();
    if !lower.ends_with(".gguf") && !lower.ends_with(".bin") {
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

/// Setup screen: open the mic so the level meter moves (mic-level events).
#[tauri::command]
fn mic_test_start(st: S) -> Result<(), String> {
    if st.pipe.lock().phase != Phase::Idle {
        return Err("Busy dictating".into());
    }
    st.recorder.start(st.settings.get().microphone).map_err(err)?;
    *st.mic_test.lock() = true;
    Ok(())
}

#[tauri::command]
fn mic_test_stop(st: S) {
    if std::mem::take(&mut *st.mic_test.lock()) {
        st.recorder.cancel();
    }
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
    pipeline::repaste(&st).map_err(err)
}

#[tauri::command]
fn preview_text(st: S, raw: String) -> String {
    text::process(&raw, &st.settings.get(), None)
}

#[derive(Serialize)]
struct AiStatus {
    ready: bool,
    keys: Vec<String>, // providers that have a saved key
}

#[tauri::command]
fn ai_status(st: S) -> AiStatus {
    let s = st.settings.get();
    AiStatus {
        ready: ai::ready(&s.ai),
        keys: ai::KEYED.iter().filter(|p| ai::has_key(p)).map(|p| p.to_string()).collect(),
    }
}

#[tauri::command]
fn set_api_key(provider: String, key: String) -> Result<(), String> {
    ai::set_key(&provider, key.trim()).map_err(err)
}

#[tauri::command]
fn default_cleanup_prompt() -> &'static str {
    settings::DEFAULT_CLEANUP_PROMPT
}

#[tauri::command]
async fn list_ai_models(st: S<'_>) -> Result<Vec<String>, String> {
    let ai_settings = st.settings.get().ai;
    tauri::async_runtime::spawn_blocking(move || ai::list_models(&ai_settings)).await.map_err(err)?.map_err(err)
}

/// Runs the clean-up prompt on a sample so the user can see it work.
#[tauri::command]
async fn test_ai(st: S<'_>, sample: String, context: Option<String>) -> Result<String, String> {
    let s = st.settings.get();
    tauri::async_runtime::spawn_blocking(move || {
        let app = context.as_deref();
        let style = match text::category(app, None) {
            "email" => s.styles.email,
            "work_chat" => s.styles.work_chat,
            "personal_chat" => s.styles.personal_chat,
            _ => s.styles.other,
        };
        ai::cleanup(&s.ai, &sample, &ai::Where { app, style: Some(&style), ..Default::default() }, &[])
    })
        .await
        .map_err(err)?
}

/// Writes every dictation to a .csv or .txt file the user picked.
#[tauri::command]
fn history_export(st: S, path: String) -> Result<usize, String> {
    st.history.export(std::path::Path::new(&path)).map_err(err)
}

#[tauri::command]
fn notes_list(st: S, query: Option<String>) -> Result<Vec<history::Note>, String> {
    st.history.notes(query).map_err(err)
}

#[tauri::command]
fn note_save(st: S, id: Option<i64>, title: String, body: String) -> Result<i64, String> {
    st.history.save_note(id, &title, &body, "note").map_err(err)
}

#[tauri::command]
fn note_delete(st: S, id: i64) -> Result<(), String> {
    st.history.delete_note(id).map_err(err)
}

#[tauri::command]
fn meeting_start(app: AppHandle) -> Result<(), String> {
    meeting::start(&app).map_err(err)
}

#[tauri::command]
fn meeting_stop(st: S) {
    st.meeting.stop();
}

#[tauri::command]
fn meeting_status(st: S) -> meeting::MeetingStatus {
    st.meeting.status()
}

#[tauri::command]
fn meeting_update(st: S, attendees: String, my_notes: String, template: String) {
    st.meeting.update(attendees, my_notes, template);
}

/// "Draft a follow-up email", "what did I promise?"... about one note.
#[tauri::command]
async fn note_ask(st: S<'_>, title: String, body: String, question: String) -> Result<String, String> {
    let ai_settings = st.settings.get().ai;
    if !ai::ready(&ai_settings) {
        return Err("Set up AI in Settings first".into());
    }
    tauri::async_runtime::spawn_blocking(move || ai::ask_note(&ai_settings, &title, &body, &question))
        .await
        .map_err(err)?
        .map_err(err)
}

// ---------- characters + buddy ----------

fn characters_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let d = app.path().app_data_dir().map_err(err)?.join("characters");
    std::fs::create_dir_all(&d).map_err(err)?;
    Ok(d)
}

#[tauri::command]
fn characters_list(app: AppHandle) -> Result<Vec<characters::CharacterInfo>, String> {
    Ok(characters::list(&characters_dir(&app)?))
}

/// An installed character's files: character.json and its sheets as data: URLs.
#[tauri::command]
fn character_files(app: AppHandle, id: String) -> Result<(String, std::collections::HashMap<String, String>), String> {
    use base64::Engine;
    let dir = characters_dir(&app)?.join(&id);
    if id.contains(['/', '\\', '.']) || !dir.is_dir() {
        return Err("No such character".into());
    }
    let json = std::fs::read_to_string(dir.join("character.json")).map_err(err)?;
    let mut files = std::collections::HashMap::new();
    for e in std::fs::read_dir(&dir).map_err(err)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let mime = if name.ends_with(".webp") { "image/webp" } else if name.ends_with(".png") { "image/png" } else { continue };
        let bytes = std::fs::read(e.path()).map_err(err)?;
        files.insert(name, format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
    }
    Ok((json, files))
}

#[tauri::command]
fn character_import(app: AppHandle, path: String) -> Result<String, String> {
    let id = characters::import(&characters_dir(&app)?, std::path::Path::new(&path)).map_err(err)?;
    let _ = app.emit("characters-changed", ());
    Ok(id)
}

#[tauri::command]
fn character_delete(app: AppHandle, st: S, id: String) -> Result<(), String> {
    characters::delete(&characters_dir(&app)?, &id).map_err(err)?;
    if st.settings.get().character == id {
        let s = st.settings.update(|s| s.character = "sheep".into()).map_err(err)?;
        let _ = app.emit("appearance-changed", &s.appearance);
        let _ = app.emit("settings-changed", ());
    }
    let _ = app.emit("characters-changed", ());
    Ok(())
}

/// The character's box in the buddy window, so it (and only it) can be grabbed.
#[tauri::command]
fn buddy_hit(x: f64, y: f64, w: f64, h: f64) {
    buddy::set_hit((w > 0.0).then_some((x, y, w, h)));
}

#[tauri::command]
fn buddy_moved(app: AppHandle, st: S) -> Result<(), String> {
    let mut s = st.settings.get();
    buddy::moved(&app, &mut s);
    st.settings.set(s).map_err(err)
}

#[tauri::command]
fn open_baa(app: AppHandle, page: Option<String>) {
    open_main(&app, page.as_deref());
}

#[tauri::command]
fn open_apps() -> Vec<String> {
    paste::open_apps()
}

// ---------- Character Studio ----------

fn studio_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let d = app.path().app_data_dir().map_err(err)?.join("studio");
    std::fs::create_dir_all(&d).map_err(err)?;
    Ok(d)
}

#[derive(Serialize)]
struct StudioStatus {
    codex: bool,
}

#[tauri::command]
fn studio_status() -> StudioStatus {
    StudioStatus { codex: studio::codex::available() }
}

/// A studio project plus how many of its move sheets are already drawn, so a job that was
/// cut short (app closed mid-way) can say "6 of 7 done" and finish only what's missing.
#[derive(serde::Serialize)]
struct StudioItem {
    #[serde(flatten)]
    project: studio::Project,
    sheets_done: usize,
    sheets_total: usize,
}

#[tauri::command]
fn studio_list(app: AppHandle) -> Result<Vec<StudioItem>, String> {
    let root = studio_dir(&app)?;
    Ok(studio::list(&root).into_iter().map(|p| {
        let sheets = studio::project_dir(&root, &p.id).join("sheets");
        let done = studio::prompts::MOODS.iter().filter(|m| sheets.join(format!("{}.png", m.id)).exists()).count();
        StudioItem { project: p, sheets_done: done, sheets_total: studio::prompts::MOODS.len() }
    }).collect())
}

#[tauri::command]
fn studio_create(app: AppHandle, name: String, description: String, refs: Vec<String>, pixel: Option<bool>) -> Result<studio::Project, String> {
    let refs: Vec<std::path::PathBuf> = refs.into_iter().map(Into::into).collect();
    studio::create(&studio_dir(&app)?, &name, &description, &refs, pixel.unwrap_or(false)).map_err(err)
}

/// Draws the master pose (a few minutes). Progress arrives as "studio-progress" events.
#[tauri::command]
async fn studio_master(app: AppHandle, id: String) -> Result<String, String> {
    let root = studio_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let p = studio::load(&root, &id).map_err(err)?;
        let tell = |pr: studio::Progress| {
            let _ = app.emit("studio-progress", pr);
        };
        // the page gets the picture itself: a file link to app data didn't show
        studio::draw_master(&root, &p, &tell).and_then(|m| studio::data_url(&m, 640)).map_err(|e| {
            tell(studio::Progress { id: id.clone(), step: "error".into(), done: 0, total: 1, message: e.to_string() });
            err(e)
        })
    })
    .await
    .map_err(err)?
}

/// The master picture of a character in progress, to pick up where you left off.
#[tauri::command]
fn studio_preview(app: AppHandle, id: String) -> Result<String, String> {
    let m = studio::project_dir(&studio_dir(&app)?, &id).join("master.png");
    studio::data_url(&m, 640).map_err(err)
}

/// Draws every mood, packs and installs the character, and picks it.
#[tauri::command]
async fn studio_moves(app: AppHandle, id: String) -> Result<String, String> {
    let root = studio_dir(&app)?;
    let chars = characters_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut p = studio::load(&root, &id).map_err(err)?;
        p.master_ok = true;
        studio::save(&root, &p).map_err(err)?;
        let tell = |pr: studio::Progress| {
            let _ = app.emit("studio-progress", pr);
        };
        match studio::draw_moves(&root, &chars, &p, &tell) {
            Ok(cid) => {
                let st = app.state::<Arc<AppState>>();
                if let Ok(s) = st.settings.update(|s| s.character = cid.clone()) {
                    let _ = app.emit("buddy-changed", &s);
                    let _ = app.emit("settings-changed", ());
                }
                let _ = app.emit("characters-changed", ());
                Ok(cid)
            }
            Err(e) => {
                tell(studio::Progress { id: id.clone(), step: "error".into(), done: 0, total: 1, message: e.to_string() });
                Err(err(e))
            }
        }
    })
    .await
    .map_err(err)?
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
                    pipeline::on_global_shortcut(app, shortcut, event.state() == ShortcutState::Pressed);
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data)?;
            let vad_path = app.path().resolve("resources/silero-vad.bin", tauri::path::BaseDirectory::Resource)?;

            let level_app = handle.clone();
            let state = Arc::new(AppState {
                settings: SettingsStore::load(&data),
                engine: Engine::new(vad_path),
                models: Models::new(data.join("models")),
                history: History::open(&data.join("history.db"))?,
                recorder: audio::Recorder::new(Arc::new(move |lvl| {
                    let _ = level_app.emit("mic-level", lvl);
                })),
                pipe: Mutex::new(Pipe::default()),
                mic_test: Mutex::new(false),
                startup_warning: Mutex::new(None),
                meeting: Default::default(),
            });
            app.manage(state.clone());

            let s = state.settings.get();
            // Whatever couldn't bind is reported; everything else still works.
            if let Err(e) = pipeline::register(&handle, &s) {
                eprintln!("baa: {e}");
                *state.startup_warning.lock() = Some(e.to_string());
            }
            overlay::create(&handle)?;
            buddy::create(&handle, &s)?;
            buddy::sync(&handle, &s);

            // Tray: left click opens Baa, right click shows the menu.
            let open_i = MenuItem::with_id(app, "open", "Open Baa", true, None::<&str>)?;
            let meet_i = MenuItem::with_id(app, "meeting", "Start or stop meeting notes", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit Baa", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_i, &meet_i, &quit_i])?;
            TrayIconBuilder::with_id("baa")
                .icon(app.default_window_icon().cloned().expect("app icon"))
                .tooltip("Baa")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => open_main(app, None),
                    "meeting" => {
                        let st = app.state::<Arc<AppState>>().inner().clone();
                        if st.meeting.status().running {
                            st.meeting.stop();
                        } else if let Err(e) = meeting::start(app) {
                            eprintln!("baa: meeting: {e}");
                        }
                        open_main(app, Some("notes"));
                    }
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
                open_main(&handle, None);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            startup_warning,
            list_models,
            download_model,
            cancel_download,
            delete_model,
            import_model,
            engine_status,
            list_microphones,
            mic_test_start,
            mic_test_stop,
            history_list,
            history_delete,
            history_clear,
            stats,
            repaste_last,
            preview_text,
            ai_status,
            set_api_key,
            list_ai_models,
            default_cleanup_prompt,
            test_ai,
            history_export,
            notes_list,
            note_save,
            note_delete,
            meeting_start,
            meeting_stop,
            meeting_status,
            meeting_update,
            note_ask,
            characters_list,
            character_import,
            character_delete,
            buddy_hit,
            buddy_moved,
            open_baa,
            studio_status,
            studio_list,
            studio_create,
            studio_master,
            studio_moves,
            open_apps,
            character_files,
            studio_preview
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
