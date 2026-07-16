//! Window control command — keeps privileged window actions behind Tauri IPC.

use tauri::command;

/// Hide the main application window. Called from the frontend *after* it has
/// played the panel's exit animation (see `supersearch://request-close` /
/// `supersearch://reset` in `App.tsx`) — never invoked directly by Rust, so
/// every dismissal path (Escape, selection, hotkey toggle, blur) animates
/// closed before the native window actually disappears.
#[command]
pub fn hide_window(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|error| error.to_string())
}