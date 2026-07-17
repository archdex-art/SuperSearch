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
///
/// `rev` is a per-window, strictly-increasing counter the frontend bumps on
/// every issued patch (see `SettingsApp.tsx`'s `patchSettings`). The settings
/// UI fires this command on every step of a color-picker drag, so several
/// calls can be in flight at once with no guarantee they *complete* in the
/// order they were *issued* — `SettingsStore::set` uses `rev` to discard a
/// stale, out-of-order write instead of letting it clobber a newer one back
/// onto disk. See its doc comment for the full race.
#[command]
pub fn update_settings(
    settings: Settings,
    rev: u64,
    app: AppHandle,
    store: tauri::State<'_, Arc<SettingsStore>>,
) -> Result<(), String> {
    let old = store.get();
    let applied = store.set(settings.clone(), rev)?;
    if !applied {
        // A newer patch already won; this one is stale and must not rebind
        // the hotkey or broadcast — that would momentarily flip every
        // window back to this call's older values.
        return Ok(());
    }
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

/// Temporarily unregister the current global hotkey so the settings window
/// can capture arbitrary keystrokes (including the currently-bound combo
/// itself) while the user records a new one — otherwise the OS-level
/// shortcut hook intercepts the keydown before it ever reaches the webview
/// and the capture UI looks permanently stuck on "Listening…".
#[command]
pub fn suspend_toggle_shortcut(app: AppHandle, store: tauri::State<'_, Arc<SettingsStore>>) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let _ = app.global_shortcut().unregister(store.get().toggle_shortcut.as_str());
}

/// Re-register the persisted toggle hotkey after a capture session ends
/// (committed, cancelled, or the settings window closed mid-capture).
#[command]
pub fn resume_toggle_shortcut(app: AppHandle, store: tauri::State<'_, Arc<SettingsStore>>) -> Result<(), String> {
    crate::register_toggle(&app, store.get().toggle_shortcut.as_str())
}
