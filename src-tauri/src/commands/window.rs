//! Window control command — keeps privileged window actions behind Tauri IPC.

use tauri::{command, Manager};

/// Hide the main application window. Called from the frontend *after* it has
/// played the panel's exit animation (see `supersearch://request-close` /
/// `supersearch://reset` in `App.tsx`) — never invoked directly by Rust, so
/// every dismissal path (Escape, selection, hotkey toggle, blur) animates
/// closed before the native window actually disappears.
#[command]
pub fn hide_window(window: tauri::Window) -> Result<(), String> {
    // The frontend completed the animated-close handoff — disarm the
    // watchdog that would otherwise force-hide (see `CloseHandoff`).
    crate::resolve_close_handoff(window.app_handle());
    window.hide().map_err(|error| error.to_string())
}

/// Open (or focus) the settings window — the "separate app" preferences
/// surface (hotkey, appearance, extensions). See `crate::open_settings_window`
/// for the lazy-build + Dock-activation-policy details.
#[command]
pub fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::open_settings_window(&app).map_err(|e| e.to_string())
}