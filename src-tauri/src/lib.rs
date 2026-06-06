//! SuperSearch — AI-Native Productivity Layer
//!
//! Tauri v2 application entry point. Boots the Rust runtime kernel,
//! extracts thread-safe handles for IPC, and launches the WebView.

mod state;
mod commands;

use std::time::Instant;
use tracing::info;


use supersearch_runtime::kernel::runtime::{RuntimeKernel, KernelConfig};
use state::AppState;

/// Build and configure the Tauri application.
///
/// ## Boot Sequence
/// 1. Initialize tracing subscriber for structured logging.
/// 2. Boot the RuntimeKernel (capability registry, scheduler, journal, etc.).
/// 3. Extract thread-safe handles into `AppState`.
/// 4. Spawn the kernel's async run loop on a background Tokio task.
/// 5. Register all IPC command handlers.
/// 6. Launch the Tauri WebView with the frameless Spotlight-style window.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 1. Structured logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "supersearch=info,tauri=warn".into()),
        )
        .compact()
        .init();

    info!("SuperSearch v0.1.0 starting");

    // 2. Boot the runtime kernel.
    let boot_start = Instant::now();
    let config = KernelConfig::default();
    let kernel = RuntimeKernel::boot(config);
    let boot_ms = boot_start.elapsed().as_millis() as u64;
    info!(boot_ms, "Runtime kernel booted");

    // 3. Extract thread-safe handles for Tauri state.
    let app_state = AppState::from_kernel(&kernel, boot_ms);

    // 4. Build the Tauri app.
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::actions::execute_action,
            commands::window::hide_window,
            commands::telemetry::get_telemetry,
            commands::search::search_query,
            commands::agent::agent_query,
            commands::agent::agent_check,
        ])
        .setup(|_app| {
            info!("Tauri setup complete — WebView ready");

            #[cfg(target_os = "macos")]
            {
                // Trigger macOS Accessibility permission prompt
                info!("Requesting macOS accessibility permissions if not granted...");
                let _ = std::process::Command::new("osascript")
                    .arg("-e")
                    .arg("tell application \"System Events\" to get name of every process")
                    .output();
            }

            // Spawn kernel run loop in background.
            // The kernel runs independently; IPC uses the extracted handles.
            let _kernel_handle = tauri::async_runtime::spawn(async move {
                kernel.run().await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Failed to run SuperSearch");
}
