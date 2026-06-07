//! Auto-update IPC.
//!
//! Checks the configured update endpoint (GitHub Releases — see RELEASING.md)
//! for a newer signed build. Returns the available version, or `None` if the
//! app is up to date. Errors gracefully if the updater is not yet configured
//! (no `plugins.updater` block / signing keys), so the app works without it.

use tauri::{command, AppHandle};
use tauri_plugin_updater::UpdaterExt;

/// Check for an available update. `Ok(Some(version))` if one is available,
/// `Ok(None)` if up to date.
#[command]
pub async fn check_for_updates(app: AppHandle) -> Result<Option<String>, String> {
    let updater = app
        .updater()
        .map_err(|e| format!("Updater not configured: {e}"))?;
    match updater.check().await {
        Ok(Some(update)) => Ok(Some(update.version.clone())),
        Ok(None) => Ok(None),
        Err(e) => Err(format!("Update check failed: {e}")),
    }
}
