//! User settings, stored as JSON in the app data folder.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Hold the shortcut to talk; a quick tap toggles instead.
    Hybrid,
    Hold,
    Toggle,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AutoSend {
    Off,
    Enter,
    CtrlEnter,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Replacement {
    pub from: String,
    pub to: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Snippet {
    pub trigger: String,
    pub text: String,
}

/// Per-app overrides, matched on the focused app's exe name (e.g. "discord.exe").
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppProfile {
    pub exe: String,
    pub auto_send: Option<AutoSend>,
    /// "as_is", "lowercase" or "no_period"
    pub style: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub shortcut: String,
    pub mode: Mode,
    /// Catalog id, or "file:<path>" for an imported model.
    pub model: Option<String>,
    pub imported_models: Vec<String>,
    /// None = let the model detect the language.
    pub language: Option<String>,
    pub microphone: Option<String>,
    pub auto_send: AutoSend,
    pub restore_clipboard: bool,
    pub trailing_space: bool,
    pub remove_fillers: bool,
    pub use_vad: bool,
    pub custom_words: Vec<String>,
    pub replacements: Vec<Replacement>,
    pub snippets: Vec<Snippet>,
    pub app_profiles: Vec<AppProfile>,
    /// Free the model from memory after this many idle seconds (0 = keep loaded).
    pub unload_after_secs: u64,
    pub overlay_position: String,
    pub show_overlay: bool,
    pub sounds: bool,
    pub character: String,
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            shortcut: "Ctrl+Space".into(),
            mode: Mode::Hybrid,
            model: None,
            imported_models: vec![],
            language: None,
            microphone: None,
            auto_send: AutoSend::Off,
            restore_clipboard: true,
            trailing_space: true,
            remove_fillers: true,
            use_vad: true,
            custom_words: vec![],
            replacements: vec![],
            snippets: vec![],
            app_profiles: vec![],
            unload_after_secs: 300,
            overlay_position: "bottom".into(),
            show_overlay: true,
            sounds: true,
            character: "sheep".into(),
            onboarded: false,
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    inner: RwLock<Settings>,
}

impl SettingsStore {
    pub fn load(dir: &std::path::Path) -> Self {
        let path = dir.join("settings.json");
        let inner = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        SettingsStore { path, inner: RwLock::new(inner) }
    }

    pub fn get(&self) -> Settings {
        self.inner.read().clone()
    }

    pub fn set(&self, s: Settings) -> anyhow::Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&s)?)?;
        std::fs::rename(&tmp, &self.path)?;
        *self.inner.write() = s;
        Ok(())
    }

    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> anyhow::Result<Settings> {
        let mut s = self.get();
        f(&mut s);
        self.set(s.clone())?;
        Ok(s)
    }
}
