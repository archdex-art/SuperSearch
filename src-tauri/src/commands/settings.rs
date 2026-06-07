//! Settings IPC — read and update persisted user settings.

use std::sync::Arc;

use tauri::{command, AppHandle};

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
    Ok(())
}
