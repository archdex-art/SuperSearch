//! SuperSearch — AI-Native Productivity Layer
//!
//! Tauri v2 application entry point. Boots the Rust runtime kernel,
//! extracts thread-safe handles for IPC, and launches the WebView.

mod state;
mod commands;

use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};

use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use supersearch_runtime::extension::ExtensionRegistry;
use supersearch_runtime::kernel::runtime::{RuntimeKernel, KernelConfig};
use state::AppState;

/// Global hotkey that summons / dismisses the palette. `Alt` is the macOS
/// Option key, so this is Option+Space — the Spotlight-style chord.
const TOGGLE_SHORTCUT: &str = "Alt+Space";

/// Show, center, and focus the palette, then tell the UI to reset its input.
fn show_palette(window: &tauri::WebviewWindow) {
    let _ = window.center();
    let _ = window.show();
    let _ = window.set_focus();
    // Clear any stale query and refocus the search box on every summon.
    let _ = window.emit("supersearch://reset", ());
}

/// Toggle palette visibility (the global-shortcut action).
fn toggle_palette(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            show_palette(&window);
        }
    }
}

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
            commands::extensions::list_extensions,
            commands::extensions::install_extension,
            commands::extensions::uninstall_extension,
            commands::extensions::set_extension_enabled,
            commands::extensions::query_extensions,
            commands::extensions::execute_extension_action,
        ])
        // Spotlight-style dismiss: hide the palette when it loses focus
        // (e.g. the user clicks another app). Release-only so DevTools focus
        // changes don't fight us during development.
        .on_window_event(|window, event| {
            #[cfg(not(debug_assertions))]
            if let tauri::WindowEvent::Focused(false) = event {
                if window.label() == "main" {
                    let _ = window.hide();
                }
            }
            // Silence unused-variable warnings in debug builds.
            let _ = (window, event);
        })
        .setup(|app| {
            info!("Tauri setup complete — WebView ready");

            // Run as an accessory app: no Dock icon, floats as an overlay over
            // other apps (including full-screen), like Spotlight.
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);

                // Trigger macOS Accessibility permission prompt early.
                info!("Requesting macOS accessibility permissions if not granted...");
                let _ = std::process::Command::new("osascript")
                    .arg("-e")
                    .arg("tell application \"System Events\" to get name of every process")
                    .output();
            }

            // Register the global summon/dismiss hotkey (Option+Space).
            let handle = app.handle().clone();
            if let Err(e) = app.global_shortcut().on_shortcut(
                TOGGLE_SHORTCUT,
                move |app, _shortcut, event| {
                    // Fire on key-down only; ignore the key-up event.
                    if event.state == ShortcutState::Pressed {
                        toggle_palette(app);
                    }
                },
            ) {
                error!(error = %e, shortcut = TOGGLE_SHORTCUT, "Failed to register global shortcut");
            } else {
                info!(shortcut = TOGGLE_SHORTCUT, "Global toggle shortcut registered");
            }

            // Make the palette overlay the *active* Space instead of living on
            // its own desktop. Without CanJoinAllSpaces (set via this call),
            // summoning from another app switches Spaces to wherever the window
            // was created — the "opens in a separate desktop" bug.
            if let Some(window) = handle.get_webview_window("main") {
                if let Err(e) = window.set_visible_on_all_workspaces(true) {
                    error!(error = %e, "Failed to set visible-on-all-workspaces");
                }
                let _ = window.set_always_on_top(true);

                // Show the palette once on first launch so the app isn't
                // invisible before the user discovers the hotkey.
                show_palette(&window);
            }

            // Initialize the extension registry. It shares the kernel's
            // capability registry + gate so extension tokens live in the same
            // capability system the agent uses.
            {
                let (caps, gate) = {
                    let app_state = app.state::<AppState>();
                    (app_state.registry.clone(), app_state.gate.clone())
                };
                let ext_dir = app
                    .path()
                    .app_data_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    .join("extensions");
                let ext_registry = Arc::new(ExtensionRegistry::new(ext_dir, caps, gate));
                if let Err(e) = ext_registry.load() {
                    error!(error = %e, "Failed to load extensions");
                }
                app.manage(ext_registry);
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
