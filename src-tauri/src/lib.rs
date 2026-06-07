//! SuperSearch — AI-Native Productivity Layer
//!
//! Tauri v2 application entry point. Boots the Rust runtime kernel,
//! extracts thread-safe handles for IPC, and launches the WebView.

mod state;
mod settings;
mod commands;

use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};

use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use supersearch_runtime::extension::ExtensionRegistry;
use supersearch_runtime::kernel::runtime::{RuntimeKernel, KernelConfig};
use settings::SettingsStore;
use state::AppState;

/// Absolute path for the runtime journal, under the user's data dir. Never a
/// relative path: a packaged `.app` has a non-writable working directory.
fn default_journal_dir() -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let mut p = std::path::PathBuf::from(home);
        p.push("Library/Application Support/com.supersearch.app/journal");
        return p.to_string_lossy().into_owned();
    }
    std::env::temp_dir()
        .join("supersearch/journal")
        .to_string_lossy()
        .into_owned()
}

/// Make the palette behave like a system overlay: it joins every Space *and*
/// floats over full-screen apps. `CanJoinAllSpaces` (set elsewhere via
/// `set_visible_on_all_workspaces`) handles normal Spaces, but a full-screen
/// app is its own Space, so the window also needs `FullScreenAuxiliary` — a
/// collection-behavior flag Tauri doesn't expose, so we set it on the NSWindow.
#[cfg(target_os = "macos")]
fn enable_fullscreen_overlay(window: &tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    // NSWindowCollectionBehavior bit flags (AppKit).
    const NS_CAN_JOIN_ALL_SPACES: usize = 1 << 0;
    const NS_FULLSCREEN_AUXILIARY: usize = 1 << 8;

    match window.ns_window() {
        Ok(ptr) => {
            let ns_window = ptr as *mut AnyObject;
            // SAFETY: `ns_window` is a valid NSWindow pointer owned by the
            // window for its lifetime; we only read and re-set its collection
            // behavior (a plain bitmask), adding flags without dropping any.
            unsafe {
                let current: usize = msg_send![ns_window, collectionBehavior];
                let behavior = current | NS_CAN_JOIN_ALL_SPACES | NS_FULLSCREEN_AUXILIARY;
                let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
            }
        }
        Err(e) => error!(error = %e, "Could not access NSWindow for overlay setup"),
    }
}

#[cfg(not(target_os = "macos"))]
fn enable_fullscreen_overlay(_window: &tauri::WebviewWindow) {}

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

/// Register `shortcut` as the global summon/dismiss hotkey (fires on key-down).
pub(crate) fn register_toggle(app: &tauri::AppHandle, shortcut: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_palette(app);
            }
        })
        .map_err(|e| e.to_string())
}

/// Atomically swap the global hotkey when the user rebinds it in settings.
pub(crate) fn rebind_toggle(app: &tauri::AppHandle, old: &str, new: &str) -> Result<(), String> {
    let _ = app.global_shortcut().unregister(old);
    register_toggle(app, new)
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
    // Anchor the journal to an absolute path under the user's data dir. A
    // packaged .app runs with a non-writable CWD (often `/`), so the default
    // relative "./data/journal" would silently fail to write the audit log.
    let boot_start = Instant::now();
    let config = KernelConfig {
        journal_dir: default_journal_dir(),
        ..KernelConfig::default()
    };
    let kernel = RuntimeKernel::boot(config);
    let boot_ms = boot_start.elapsed().as_millis() as u64;
    info!(boot_ms, "Runtime kernel booted");

    // 3. Extract thread-safe handles for Tauri state.
    let app_state = AppState::from_kernel(&kernel, boot_ms);

    // 4. Build the Tauri app.
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
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
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::journal::get_journal_summary,
            commands::updater::check_for_updates,
        ]);

    // Auto-update is opt-in (off by default). The updater plugin eagerly
    // requires `plugins.updater.pubkey`, so registering it without signing keys
    // would crash boot. Release builds enable it with `--features updater`
    // after generating keys (see RELEASING.md).
    #[cfg(feature = "updater")]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        // Spotlight-style dismiss: hide the palette when it loses focus
        // (e.g. the user clicks another app), if enabled in settings.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(false) = event {
                if window.label() == "main" {
                    let hide_on_blur = window
                        .app_handle()
                        .try_state::<Arc<SettingsStore>>()
                        .map(|s| s.get().hide_on_blur)
                        .unwrap_or(false);
                    if hide_on_blur {
                        let _ = window.hide();
                    }
                }
            }
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

            let handle = app.handle().clone();

            // Resolve the app data dir once (shared by settings + extensions).
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));

            // Load persisted settings (hotkey, hide-on-blur, theme).
            let settings_store = Arc::new(SettingsStore::load(data_dir.clone()));
            let settings = settings_store.get();
            app.manage(settings_store);

            // Register the global summon/dismiss hotkey from settings.
            match register_toggle(&handle, &settings.toggle_shortcut) {
                Ok(()) => info!(shortcut = %settings.toggle_shortcut, "Global toggle shortcut registered"),
                Err(e) => error!(error = %e, shortcut = %settings.toggle_shortcut, "Failed to register global shortcut"),
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
                // Also float over full-screen apps (adds FullScreenAuxiliary).
                enable_fullscreen_overlay(&window);

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
                let ext_dir = data_dir.join("extensions");
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
