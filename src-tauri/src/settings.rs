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
    /// Base UI theme identifier (the frontend interprets this — "dark" is
    /// the only built-in today).
    pub theme: String,
    /// Accent hex color (e.g. "#f5a623") overriding the built-in amber
    /// identity. `None` = use the default. Validated as `#RRGGBB` client-side
    /// (the settings UI's color picker); stored as an opaque string here
    /// since the palette only ever consumes it as a CSS value.
    #[serde(default)]
    pub accent_color: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            toggle_shortcut: DEFAULT_TOGGLE_SHORTCUT.into(),
            hide_on_blur: true,
            theme: "dark".into(),
            accent_color: None,
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
            .set(Settings {
                toggle_shortcut: "CommandOrControl+Space".into(),
                hide_on_blur: false,
                theme: "light".into(),
                accent_color: Some("#22d3ee".into()),
            })
            .unwrap();

        // A fresh store reading the same dir sees the persisted values.
        let reloaded = SettingsStore::load(dir.path().to_path_buf());
        let r = reloaded.get();
        assert_eq!(r.toggle_shortcut, "CommandOrControl+Space");
        assert!(!r.hide_on_blur);
        assert_eq!(r.theme, "light");
        assert_eq!(r.accent_color.as_deref(), Some("#22d3ee"));
    }

    #[test]
    fn old_settings_file_without_accent_color_still_loads() {
        // Simulates a settings.json written before `accent_color` existed —
        // `#[serde(default)]` on the struct must fill it in as `None` rather
        // than fail to deserialize the whole file.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"toggle_shortcut":"Alt+Space","hide_on_blur":true,"theme":"dark"}"#,
        )
        .unwrap();
        let s = SettingsStore::load(dir.path().to_path_buf()).get();
        assert_eq!(s.toggle_shortcut, "Alt+Space");
        assert_eq!(s.accent_color, None);
    }
}
