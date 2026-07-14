//! SuperSearch — AI-Native Productivity Layer
//!
//! Tauri v2 application entry point. Boots the Rust runtime kernel,
//! extracts thread-safe handles for IPC, and launches the WebView.

pub mod state;
pub mod settings;
pub mod commands;

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
/// relative path: a packaged app may have a non-writable working directory.
fn default_journal_dir() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let mut p = std::path::PathBuf::from(home);
            p.push("Library/Application Support/com.supersearch.app/journal");
            return p.to_string_lossy().into_owned();
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
            return std::path::PathBuf::from(data)
                .join("supersearch/journal")
                .to_string_lossy()
                .into_owned();
        }
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home)
                .join(".local/share/supersearch/journal")
                .to_string_lossy()
                .into_owned();
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return std::path::PathBuf::from(appdata)
                .join("supersearch\\journal")
                .to_string_lossy()
                .into_owned();
        }
    }
    std::env::temp_dir()
        .join("supersearch/journal")
        .to_string_lossy()
        .into_owned()
}

/// Make the palette behave like a system overlay: it joins every Space, floats
/// over full-screen apps, *and* sits at a high enough window level to actually
/// draw on top of them.
///
/// Two things are required and both are set here (Tauri exposes neither):
/// 1. **Collection behavior** — `CanJoinAllSpaces` lets the window follow the
///    active Space; `FullScreenAuxiliary` lets it attach to a full-screen app's
///    Space (which is otherwise isolated). `set_visible_on_all_workspaces` only
///    sets the first flag.
/// 2. **A high window level** — `set_always_on_top(true)` only raises the window
///    to `NSFloatingWindowLevel` (3), which is *not* high enough to composite
///    above a full-screen app; the window silently stays behind it. Spotlight-
///    style overlays need `NSStatusWindowLevel` (25), i.e. the menu-bar level.
///    That's why the caller must NOT also call `set_always_on_top` — its async
///    `setLevel:` would otherwise clobber this one back down to 3.
///
/// Re-asserted on every summon (see `show_palette`) because macOS can reset a
/// window's level/behavior when it is ordered out and back across Spaces.
#[cfg(target_os = "macos")]
fn enable_fullscreen_overlay(window: &tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    // NSWindowCollectionBehavior bit flags (AppKit).
    const NS_CAN_JOIN_ALL_SPACES: usize = 1 << 0;
    const NS_FULLSCREEN_AUXILIARY: usize = 1 << 8;
    // NSStatusWindowLevel — the menu-bar level (CGWindowLevelForKey of
    // kCGStatusWindowLevelKey == 25 on all current macOS). High enough to draw
    // over a full-screen app's content, low enough to stay below system alerts.
    const NS_STATUS_WINDOW_LEVEL: isize = 25;

    match window.ns_window() {
        Ok(ptr) => {
            let ns_window = ptr as *mut AnyObject;
            // SAFETY: `ns_window` is a valid NSWindow pointer owned by the
            // window for its lifetime; we only read and re-set its collection
            // behavior (a plain bitmask, adding flags without dropping any) and
            // its window level (a plain NSInteger). This runs on the main
            // thread (Tauri `setup` / the summon handler), where AppKit window
            // mutation is required.
            unsafe {
                let current: usize = msg_send![ns_window, collectionBehavior];
                let behavior = current | NS_CAN_JOIN_ALL_SPACES | NS_FULLSCREEN_AUXILIARY;
                let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
                let _: () = msg_send![ns_window, setLevel: NS_STATUS_WINDOW_LEVEL];
            }
        }
        Err(e) => error!(error = %e, "Could not access NSWindow for overlay setup"),
    }
}

/// Ask macOS to trust this app for **Accessibility** (synthetic keystrokes).
///
/// Keystroke-based actions — app commands (`/chatgpt …`), and any System Events
/// `keystroke`/`key code` — are silently dropped until the app is listed and
/// enabled under *System Settings → Privacy & Security → Accessibility*. Calling
/// `AXIsProcessTrustedWithOptions` with the prompt option posts the system grant
/// dialog on first run and registers the app in that list. (Returns the current
/// trust state; the user must still flip the toggle, after which keystrokes work.)
#[cfg(target_os = "macos")]
fn request_accessibility() {
    use std::ffi::c_void;
    use std::ptr;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
        static kAXTrustedCheckOptionPrompt: *const c_void; // CFStringRef
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFBooleanTrue: *const c_void; // CFBooleanRef
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> *const c_void;
        fn CFRelease(cf: *const c_void);
    }

    // SAFETY: standard CoreFoundation/Accessibility FFI. We build a 1-entry
    // options dict { kAXTrustedCheckOptionPrompt: true } with the type-aware
    // CF callbacks, pass it to the trust check, then release the dict. All
    // pointers come from framework symbols or `CFDictionaryCreate`.
    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const _,
            &kCFTypeDictionaryValueCallBacks as *const _,
        );
        let trusted = AXIsProcessTrustedWithOptions(options);
        if !options.is_null() {
            CFRelease(options);
        }
        info!(trusted, "macOS Accessibility trust state");
    }
}

#[cfg(not(target_os = "macos"))]
fn enable_fullscreen_overlay(_window: &tauri::WebviewWindow) {}

/// Show, center, and focus the palette, then tell the UI to reset its input.
fn show_palette(window: &tauri::WebviewWindow) {
    // Re-assert the overlay collection-behavior + high window level every time:
    // macOS can drop them when the window is ordered out and back across
    // Spaces, which would make the palette fall behind a full-screen app.
    enable_fullscreen_overlay(window);
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

    info!("SuperSearch v{} starting", env!("CARGO_PKG_VERSION"));

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
            commands::extensions::set_extension_trusted,
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

                // Request the *Accessibility* permission keystroke synthesis
                // needs (app commands, lock via keystroke, etc.). The old
                // `osascript … get name of every process` only prompted for
                // Automation — the wrong permission — so keystrokes stayed
                // silently blocked.
                info!("Checking macOS Accessibility trust (prompts on first run)...");
                request_accessibility();
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
                // Float over full-screen apps + sit at a high window level.
                // (Deliberately NOT `set_always_on_top`: it async-sets only
                // NSFloatingWindowLevel, which would clobber the higher
                // NSStatusWindowLevel this sets and leave the palette behind
                // full-screen apps.)
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
