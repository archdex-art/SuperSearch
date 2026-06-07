//! Auto-update IPC.
//!
//! Checks the configured update endpoint (GitHub Releases — see RELEASING.md)
//! for a newer signed build. Auto-update is gated behind the `updater` Cargo
//! feature (off by default), because the updater plugin requires signing keys
//! to be configured. Without the feature, this command reports that updates are
//! not built in, and the app runs normally.

use tauri::{command, AppHandle};

/// Check for an available update. `Ok(Some(version))` if one is available,
/// `Ok(None)` if up to date, `Err` if updates aren't built in / configured.
#[command]
pub async fn check_for_updates(app: AppHandle) -> Result<Option<String>, String> {
    #[cfg(feature = "updater")]
    {
        use tauri_plugin_updater::UpdaterExt;
        let updater = app
            .updater()
            .map_err(|e| format!("Updater not configured: {e}"))?;
        match updater.check().await {
            Ok(Some(update)) => Ok(Some(update.version.clone())),
            Ok(None) => Ok(None),
            Err(e) => Err(format!("Update check failed: {e}")),
        }
    }

    #[cfg(not(feature = "updater"))]
    {
        let _ = app;
        Err("This build was compiled without auto-update support".into())
    }
}
