//! Extension management IPC — backs the Plugin Manager UI.
//!
//! All handlers operate on the shared [`ExtensionRegistry`] managed by Tauri.
//! Installation/enable/uninstall mutate disk + capability grants; `query` and
//! `execute_extension_action` are the runtime path used by search.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{command, AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use supersearch_runtime::extension::{
    ExtensionAction, ExtensionInfo, ExtensionQueryHit, ExtensionRegistry,
};
use supersearch_runtime::extension::manifest::ExtensionKind;
use supersearch_runtime::extension::runtime::V8Isolate;
use supersearch_runtime::extension::ipc::envelope::EnvelopeType;

/// List all installed extensions (for the manager UI).
#[command]
pub fn list_extensions(registry: tauri::State<'_, Arc<ExtensionRegistry>>) -> Vec<ExtensionInfo> {
    registry.list()
}

/// Install an extension from a local directory containing `manifest.toml`.
/// Returns the new extension id. Installed disabled pending user consent.
#[command]
pub fn install_extension(
    path: String,
    registry: tauri::State<'_, Arc<ExtensionRegistry>>,
) -> Result<String, String> {
    registry.install(&PathBuf::from(path)).map_err(|e| e.to_string())
}

/// Uninstall an extension by id (revokes its token, removes its files).
#[command]
pub fn uninstall_extension(
    id: String,
    registry: tauri::State<'_, Arc<ExtensionRegistry>>,
) -> Result<(), String> {
    registry.uninstall(&id).map_err(|e| e.to_string())
}

/// Enable or disable an extension (grants/revokes its capability token).
#[command]
pub fn set_extension_enabled(
    id: String,
    enabled: bool,
    registry: tauri::State<'_, Arc<ExtensionRegistry>>,
) -> Result<(), String> {
    registry.set_enabled(&id, enabled).map_err(|e| e.to_string())
}

/// Trust (or untrust) an unsandboxed script extension so it may run in queries.
/// The UI must show an explicit, informed consent dialog before passing `true`.
#[command]
pub fn set_extension_trusted(
    id: String,
    trusted: bool,
    registry: tauri::State<'_, Arc<ExtensionRegistry>>,
) -> Result<(), String> {
    registry.set_trusted(&id, trusted).map_err(|e| e.to_string())
}

/// Run a query against all enabled script extensions and return merged results,
/// each tagged with its source extension id (for action routing).
#[command]
pub fn query_extensions(
    query: String,
    registry: tauri::State<'_, Arc<ExtensionRegistry>>,
) -> Vec<ExtensionQueryHit> {
    registry.query(&query)
}

/// Execute an extension result-action (mediated by the extension's token).
///
/// Offloaded to a blocking thread: an extension action can spawn a subprocess
/// (e.g. `open`), and a plain (non-`async`) Tauri command runs synchronously
/// on the IPC caller's thread (the WKWebView main thread on macOS), which
/// would freeze the window until the action finished.
#[command]
pub async fn execute_extension_action(
    id: String,
    action: ExtensionAction,
    registry: tauri::State<'_, Arc<ExtensionRegistry>>,
) -> Result<(), String> {
    let registry = registry.inner().clone();
    tokio::task::spawn_blocking(move || registry.execute_action(&id, &action).map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("extension action task panicked: {e}"))?
}

/// Open a native folder picker for "install an extension from this
/// directory". Returns `None` if the user cancelled. Runs on a blocking
/// thread — the native dialog blocks the calling thread until dismissed,
/// which would otherwise freeze the settings window's WebView main thread.
#[command]
pub async fn pick_extension_dir(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("folder picker task panicked: {e}"))
}

/// Launch a JS extension by id: boots a V8 isolate, executes its bundle, and
/// begins streaming `UiSync` payloads to the frontend as
/// `extension_ui_sync_<id>` Tauri events.
///
/// Returns immediately — the isolate's event loop runs on a detached background
/// task so the IPC call never blocks the WebView thread.
#[command]
pub async fn launch_extension(
    id: String,
    app: AppHandle,
    registry: tauri::State<'_, Arc<ExtensionRegistry>>,
) -> Result<(), String> {
    use tauri::Emitter as _;

    // Find the extension record: we need its dir and manifest.
    let (manifest, bundle_path) = {
        let info_list = registry.list();
        let info = info_list
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| format!("extension '{id}' not found"))?;
        if info.kind != ExtensionKind::Js {
            return Err(format!("extension '{id}' is not a JS extension"));
        }
        // Re-derive the on-disk path from the registry dir + extension id.
        // The ExtensionRegistry doesn't expose the dir directly, so we
        // reconstruct it from the known layout: <data_dir>/extensions/<id>/.
        let ext_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("extensions")
            .join(&id);
        let toml_text = std::fs::read_to_string(ext_dir.join("manifest.toml"))
            .map_err(|e| format!("cannot read manifest: {e}"))?;
        let manifest = supersearch_runtime::extension::manifest::ExtensionManifest::from_toml(
            &toml_text,
        )
        .map_err(|e| format!("invalid manifest: {e}"))?;
        let bundle_path = ext_dir.join(&manifest.entrypoint);
        (manifest, bundle_path)
    };

    let bundle_src = std::fs::read_to_string(&bundle_path)
        .map_err(|e| format!("cannot read bundle '{}': {e}", bundle_path.display()))?;

    let event_name = format!("extension_ui_sync_{}", id);

    // Spawn the isolate on a dedicated thread — `JsRuntime` is `!Send` so it
    // cannot cross await points and must live entirely on one OS thread.
    let app_handle = app.clone();
    std::thread::spawn(move || {
        let mut isolate = V8Isolate::new(manifest);
        if let Err(e) = isolate.evaluate_script("extension:bundle.js", &bundle_src) {
            let _ = app_handle.emit(
                &event_name,
                serde_json::json!({ "error": e.to_string() }),
            );
            return;
        }

        // Drain UiSync messages from the guest and forward them as Tauri events.
        // This loop runs until the isolate's sender is dropped (extension exits).
        while let Some(envelope) = isolate.rx.blocking_recv() {
            if envelope.2 == EnvelopeType::UiSync {
                // Payload is the serialized UINode tree; forward as JSON to the
                // frontend so Hydrator.tsx can deserialize and render it.
                if let Ok(json) = serde_json::to_value(&envelope.5) {
                    let _ = app_handle.emit(&event_name, json);
                }
            }
        }
    });

    Ok(())
}
