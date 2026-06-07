//! Extension management IPC — backs the Plugin Manager UI.
//!
//! All handlers operate on the shared [`ExtensionRegistry`] managed by Tauri.
//! Installation/enable/uninstall mutate disk + capability grants; `query` and
//! `execute_extension_action` are the runtime path used by search.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::command;

use supersearch_runtime::extension::{
    ExtensionAction, ExtensionInfo, ExtensionQueryHit, ExtensionRegistry,
};

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
#[command]
pub fn execute_extension_action(
    id: String,
    action: ExtensionAction,
    registry: tauri::State<'_, Arc<ExtensionRegistry>>,
) -> Result<(), String> {
    registry.execute_action(&id, &action).map_err(|e| e.to_string())
}
