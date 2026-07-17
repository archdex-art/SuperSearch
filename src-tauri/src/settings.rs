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
///
/// `settings`/`applied_rev` share one lock so a check-then-write decision is
/// atomic — see `set()`.
pub struct SettingsStore {
    path: PathBuf,
    state: RwLock<(Settings, u64)>,
}

impl SettingsStore {
    /// Load settings from `dir/settings.json`, falling back to defaults.
    pub fn load(dir: PathBuf) -> Self {
        let path = dir.join("settings.json");
        let settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self { path, state: RwLock::new((settings, 0)) }
    }

    /// Current settings snapshot.
    pub fn get(&self) -> Settings {
        self.state.read().0.clone()
    }

    /// Replace settings and persist to disk — but only if `rev` is newer than
    /// the last-applied write. Returns `Ok(true)` if this call's data won and
    /// was persisted, `Ok(false)` if it lost to a write the caller already
    /// knows is more recent and was silently discarded.
    ///
    /// The settings window fires `update_settings` on every keystroke/drag
    /// step of the accent color picker (`HexColorPicker.onChange`), so many
    /// overlapping IPC calls can be in flight at once. Tauri dispatches each
    /// command invocation to its own task, so completion order is **not**
    /// guaranteed to match call order — without this guard, a slower older
    /// write (e.g. an early drag frame) can complete *after* the final color
    /// the user actually released on, silently overwriting it back to a
    /// stale value on disk. `rev` is a counter the frontend increments once
    /// per issued patch (see `SettingsApp.tsx`'s `patchSettings`), so it's
    /// strictly increasing in call order even when responses race; comparing
    /// it while holding the same lock we write under (not a separate atomic
    /// check followed by a separate write) closes the check-then-act window
    /// that a plain `AtomicU64::fetch_max` guard alone would leave open.
    pub fn set(&self, new: Settings, rev: u64) -> Result<bool, String> {
        let mut state = self.state.write();
        if rev <= state.1 {
            return Ok(false);
        }
        state.0 = new;
        state.1 = rev;
        let json = serde_json::to_string_pretty(&state.0).map_err(|e| e.to_string())?;
        drop(state);
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&self.path, json).map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// Unconditionally replace settings and persist — bypassing the `rev`
    /// guard entirely, including resetting the applied-rev counter to 0.
    /// For internal, one-time corrections that happen *before* any frontend
    /// writes exist yet (currently just the boot-time self-heal that
    /// overwrites a macOS-reserved shortcut), never for anything racing
    /// against `update_settings` — using this after the frontend has already
    /// issued patches would let this call clobber a newer one, and resetting
    /// the rev counter to 0 would make a *subsequent* stale write look valid
    /// again.
    pub fn force_set(&self, new: Settings) -> Result<(), String> {
        let mut state = self.state.write();
        state.0 = new;
        state.1 = 0;
        let json = serde_json::to_string_pretty(&state.0).map_err(|e| e.to_string())?;
        drop(state);
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
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
            .set(
                Settings {
                    toggle_shortcut: "CommandOrControl+Space".into(),
                    hide_on_blur: false,
                    theme: "light".into(),
                    accent_color: Some("#22d3ee".into()),
                },
                1,
            )
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

    #[test]
    fn stale_out_of_order_write_is_discarded() {
        // Simulates two overlapping `update_settings` calls completing in
        // the *opposite* order they were issued in (exactly what an
        // unthrottled color-picker drag can trigger — see `SettingsStore::set`'s
        // doc comment). The higher-rev (later-issued) write must win even
        // though its `set()` call lands first.
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path().to_path_buf());

        // rev 2 ("the final color the user released on") completes first.
        let applied = store
            .set(
                Settings { accent_color: Some("#3b82f6".into()), ..Settings::default() },
                2,
            )
            .unwrap();
        assert!(applied);

        // rev 1 (an earlier drag frame) completes second — must be discarded.
        let applied = store
            .set(
                Settings { accent_color: Some("#a78bfa".into()), ..Settings::default() },
                1,
            )
            .unwrap();
        assert!(!applied);

        assert_eq!(store.get().accent_color.as_deref(), Some("#3b82f6"));
        let reloaded = SettingsStore::load(dir.path().to_path_buf());
        assert_eq!(reloaded.get().accent_color.as_deref(), Some("#3b82f6"));
    }
}
