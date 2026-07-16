//! Persistent user settings.
//!
//! Stored as `settings.json` under the app data dir and loaded at boot, so the
//! hotkey, dismiss behavior, and theme survive restarts (previously the app was
//! cold every launch). Thread-safe; cloned snapshots are handed to the UI.

use std::path::PathBuf;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Built-in default global summon/dismiss hotkey. Exposed so `lib.rs` can
/// fall back to it — and persist the correction — if the user's configured
/// shortcut turns out to be one macOS reserves for itself (e.g. Control+Space
/// for input-source switching), which a third-party registration can only
/// intermittently win against.
pub const DEFAULT_TOGGLE_SHORTCUT: &str = "Alt+Space";

/// User-configurable settings. `#[serde(default)]` keeps old files
/// forward-compatible as new fields are added.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Global summon/dismiss hotkey, in Tauri accelerator syntax (e.g.
    /// "Alt+Space", "CommandOrControl+Space").
    pub toggle_shortcut: String,
    /// Hide the palette automatically when it loses focus (Spotlight-style).
    pub hide_on_blur: bool,
    /// UI theme identifier (the frontend interprets this).
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            toggle_shortcut: DEFAULT_TOGGLE_SHORTCUT.into(),
            hide_on_blur: true,
            theme: "dark".into(),
        }
    }
}

/// Thread-safe, file-backed settings store.
pub struct SettingsStore {
    path: PathBuf,
    settings: RwLock<Settings>,
}

impl SettingsStore {
    /// Load settings from `dir/settings.json`, falling back to defaults.
    pub fn load(dir: PathBuf) -> Self {
        let path = dir.join("settings.json");
        let settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self { path, settings: RwLock::new(settings) }
    }

    /// Current settings snapshot.
    pub fn get(&self) -> Settings {
        self.settings.read().clone()
    }

    /// Replace settings and persist to disk.
    pub fn set(&self, new: Settings) -> Result<(), String> {
        *self.settings.write() = new;
        self.persist()
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(&*self.settings.read())
            .map_err(|e| e.to_string())?;
        std::fs::write(&self.path, json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_then_roundtrips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path().to_path_buf());
        let s = store.get();
        assert_eq!(s.toggle_shortcut, "Alt+Space");
        assert!(s.hide_on_blur);

        store
            .set(Settings { toggle_shortcut: "CommandOrControl+Space".into(), hide_on_blur: false, theme: "light".into() })
            .unwrap();

        // A fresh store reading the same dir sees the persisted values.
        let reloaded = SettingsStore::load(dir.path().to_path_buf());
        let r = reloaded.get();
        assert_eq!(r.toggle_shortcut, "CommandOrControl+Space");
        assert!(!r.hide_on_blur);
        assert_eq!(r.theme, "light");
    }
}
