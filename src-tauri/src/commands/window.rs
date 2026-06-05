//! Window control command — keeps privileged window actions behind Tauri IPC.

use tauri::command;

/// Hide the main application window.
#[command]
pub fn hide_window(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|error| error.to_string())
}