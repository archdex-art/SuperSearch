//! Settings IPC — read and update persisted user settings.

use std::sync::Arc;

use serde::Serialize;
use tauri::{command, AppHandle, Emitter};

use crate::settings::{Settings, SettingsStore};

/// Return the current persisted settings.
#[command]
pub fn get_settings(store: tauri::State<'_, Arc<SettingsStore>>) -> Settings {
    store.get()
}

/// Persist new settings. If the toggle shortcut changed, re-register the global
/// hotkey atomically so the change takes effect immediately.
#[command]
pub fn update_settings(
    settings: Settings,
    app: AppHandle,
    store: tauri::State<'_, Arc<SettingsStore>>,
) -> Result<(), String> {
    let old = store.get();
    store.set(settings.clone())?;
    if settings.toggle_shortcut != old.toggle_shortcut {
        crate::rebind_toggle(&app, &old.toggle_shortcut, &settings.toggle_shortcut)?;
    }
    // Broadcast to every window (palette + settings) so an accent/theme
    // change repaints live instead of needing a reopen. See
    // `applyAccent`/the `settings-changed` listener in App.tsx.
    let _ = app.emit("supersearch://settings-changed", &settings);
    Ok(())
}

/// Result of validating a candidate global-shortcut accelerator.
#[derive(Serialize)]
pub struct ShortcutCheck {
    pub ok: bool,
    pub reason: Option<String>,
}

/// Validate a candidate accelerator *before* the settings UI commits it —
/// reuses the exact same reserved-shortcut check `register_toggle` enforces
/// on save, so the hotkey-capture control can warn the instant the user
/// picks a combo macOS reserves for itself, instead of only finding out
/// after `update_settings` rejects it.
#[command]
pub fn validate_shortcut(shortcut: String) -> ShortcutCheck {
    if crate::is_reserved_macos_shortcut(&shortcut) {
        ShortcutCheck {
            ok: false,
            reason: Some(format!(
                "\"{shortcut}\" is reserved by macOS for switching input sources \
                 (System Settings → Keyboard → Keyboard Shortcuts → Input Sources)."
            )),
        }
    } else {
        ShortcutCheck { ok: true, reason: None }
    }
}
