//! Extension registry — the source of truth for installed extensions.
//!
//! Responsibilities:
//! - Discover extensions on disk (`<dir>/<id>/manifest.toml`).
//! - Track enabled/disabled state, persisted to `<dir>/registry.json`.
//! - On enable, grant a capability token scoped to `plugin.<id>` covering
//!   exactly the manifest's requested permissions; on disable/uninstall,
//!   revoke it. Extension result-actions are mediated by the same gate.
//! - Install (copy a folder in) / uninstall (remove it).
//! - Fan a query out to enabled script extensions and collect results.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::capability::gate::{CapabilityGate, GateDecision};
use crate::capability::namespace::Namespace;
use crate::capability::registry::CapabilityRegistry;
use crate::capability::token::{CapabilityToken, Permission};

use super::host::{self, ExtensionAction};
use super::wasm;
use super::manifest::{ExtensionKind, ExtensionManifest};

/// A single installed extension and its runtime state.
struct ExtensionRecord {
    manifest: ExtensionManifest,
    dir: PathBuf,
    enabled: bool,
    /// Capability token, present only while enabled.
    token: Option<CapabilityToken>,
}

/// Serializable summary of an extension for the manager UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub kind: ExtensionKind,
    pub enabled: bool,
    pub permissions: Vec<PermissionInfo>,
}

/// A requested permission rendered for the consent UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionInfo {
    pub permission: String,
    pub justification: String,
}

/// A single result row from `query`, tagged with the extension that produced it
/// so the caller can route its action back to the right capability token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionQueryHit {
    pub extension_id: String,
    pub title: String,
    pub subtitle: String,
    pub action: Option<ExtensionAction>,
}

/// Errors from registry operations.
#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error("extension `{0}` not found")]
    NotFound(String),
    #[error("extension `{0}` is already installed")]
    AlreadyInstalled(String),
    #[error("invalid manifest: {0}")]
    Manifest(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("action failed: {0}")]
    Action(String),
}

impl From<std::io::Error> for ExtensionError {
    fn from(e: std::io::Error) -> Self {
        ExtensionError::Io(e.to_string())
    }
}

/// The installed-extension registry.
pub struct ExtensionRegistry {
    dir: PathBuf,
    records: RwLock<Vec<ExtensionRecord>>,
    capabilities: Arc<CapabilityRegistry>,
    gate: Arc<CapabilityGate>,
}

impl ExtensionRegistry {
    /// Create a registry rooted at `dir`. Call [`load`](Self::load) to scan.
    pub fn new(dir: PathBuf, capabilities: Arc<CapabilityRegistry>, gate: Arc<CapabilityGate>) -> Self {
        Self {
            dir,
            records: RwLock::new(Vec::new()),
            capabilities,
            gate,
        }
    }

    /// Scan the extensions directory, parse manifests, and restore enabled
    /// state. Granting tokens for the extensions that were left enabled.
    pub fn load(&self) -> Result<(), ExtensionError> {
        std::fs::create_dir_all(&self.dir)?;
        let enabled_state = self.read_state();

        let mut records = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join("manifest.toml");
            if !manifest_path.exists() {
                continue;
            }
            let text = match std::fs::read_to_string(&manifest_path) {
                Ok(t) => t,
                Err(e) => {
                    warn!(path = %manifest_path.display(), error = %e, "Skipping unreadable manifest");
                    continue;
                }
            };
            let manifest = match ExtensionManifest::from_toml(&text).map_err(|e| e.to_string())
                .and_then(|m| m.validate().map(|_| m).map_err(|e| e.to_string()))
            {
                Ok(m) => m,
                Err(e) => {
                    warn!(path = %manifest_path.display(), error = %e, "Skipping invalid manifest");
                    continue;
                }
            };

            let enabled = enabled_state.get(&manifest.id).copied().unwrap_or(false);
            let token = if enabled { Some(self.grant_for(&manifest)) } else { None };
            records.push(ExtensionRecord { manifest, dir: path, enabled, token });
        }

        info!(count = records.len(), "Extensions loaded");
        *self.records.write() = records;
        Ok(())
    }

    /// List all installed extensions for the manager UI.
    pub fn list(&self) -> Vec<ExtensionInfo> {
        self.records.read().iter().map(record_info).collect()
    }

    /// Enable or disable an extension, granting/revoking its capability token.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), ExtensionError> {
        {
            let mut records = self.records.write();
            let record = records
                .iter_mut()
                .find(|r| r.manifest.id == id)
                .ok_or_else(|| ExtensionError::NotFound(id.to_string()))?;

            if enabled && !record.enabled {
                record.token = Some(self.grant_for(&record.manifest));
            } else if !enabled && record.enabled {
                self.capabilities.revoke_all_for_grantee(id);
                record.token = None;
            }
            record.enabled = enabled;
        }
        self.persist_state();
        Ok(())
    }

    /// Install an extension by copying a source directory (containing a valid
    /// `manifest.toml`) into the registry. Installed disabled by default so the
    /// user must consent via enable.
    pub fn install(&self, src: &Path) -> Result<String, ExtensionError> {
        let text = std::fs::read_to_string(src.join("manifest.toml"))
            .map_err(|_| ExtensionError::Manifest("missing manifest.toml".into()))?;
        let manifest = ExtensionManifest::from_toml(&text)
            .map_err(|e| ExtensionError::Manifest(e.to_string()))?;
        manifest.validate().map_err(|e| ExtensionError::Manifest(e.to_string()))?;

        let dest = self.dir.join(&manifest.id);
        if dest.exists() {
            return Err(ExtensionError::AlreadyInstalled(manifest.id));
        }
        copy_dir(src, &dest)?;
        info!(id = %manifest.id, "Extension installed");

        let id = manifest.id.clone();
        self.records.write().push(ExtensionRecord {
            manifest,
            dir: dest,
            enabled: false,
            token: None,
        });
        self.persist_state();
        Ok(id)
    }

    /// Uninstall an extension: revoke its token and remove its directory.
    pub fn uninstall(&self, id: &str) -> Result<(), ExtensionError> {
        let dir = {
            let mut records = self.records.write();
            let pos = records
                .iter()
                .position(|r| r.manifest.id == id)
                .ok_or_else(|| ExtensionError::NotFound(id.to_string()))?;
            self.capabilities.revoke_all_for_grantee(id);
            records.remove(pos).dir
        };
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        info!(id, "Extension uninstalled");
        self.persist_state();
        Ok(())
    }

    /// Fan a query out to enabled script extensions and collect their results,
    /// each tagged with its source extension id.
    pub fn query(&self, input: &str) -> Vec<ExtensionQueryHit> {
        let input_lower = input.to_lowercase();
        let targets: Vec<(String, ExtensionKind, PathBuf, String)> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| r.enabled)
                .filter(|r| keyword_match(&r.manifest.keywords, &input_lower))
                .map(|r| (r.manifest.id.clone(), r.manifest.kind, r.dir.clone(), r.manifest.entrypoint.clone()))
                .collect()
        };

        let mut hits = Vec::new();
        for (id, kind, dir, entrypoint) in targets {
            let outcome = match kind {
                ExtensionKind::Script => host::run_query(&dir, &entrypoint, input),
                ExtensionKind::Wasm => {
                    wasm::run_query(&dir.join(&entrypoint), input).map_err(|e| {
                        // Normalize to the host error type for uniform logging.
                        super::host::HostError::BadOutput(e)
                    })
                }
            };
            match outcome {
                Ok(results) => {
                    for r in results {
                        hits.push(ExtensionQueryHit {
                            extension_id: id.clone(),
                            title: r.title,
                            subtitle: r.subtitle,
                            action: r.action,
                        });
                    }
                }
                Err(e) => warn!(id, dir = %dir.display(), error = %e, "Extension query failed"),
            }
        }
        hits
    }

    /// Execute an extension result-action, mediated by the extension's token.
    pub fn execute_action(&self, id: &str, action: &ExtensionAction) -> Result<(), ExtensionError> {
        let token = {
            let records = self.records.read();
            let record = records
                .iter()
                .find(|r| r.manifest.id == id)
                .ok_or_else(|| ExtensionError::NotFound(id.to_string()))?;
            record
                .token
                .clone()
                .ok_or_else(|| ExtensionError::PermissionDenied(format!("{} is disabled", id)))?
        };

        let (namespace, permission) = match action {
            ExtensionAction::OpenUrl { .. } => ("network", Permission::NetworkConnect),
            ExtensionAction::OpenPath { .. } => ("fs", Permission::FileRead),
            ExtensionAction::Copy { .. } => ("clipboard", Permission::ClipboardWrite),
        };
        let ns = Namespace::new(format!("plugin.{}.{}", id, namespace));
        if let GateDecision::Denied { reason, .. } = self.gate.check(Some(&token), &ns, permission) {
            return Err(ExtensionError::PermissionDenied(format!(
                "{:?} requires {:?}: {}",
                action, permission, reason
            )));
        }

        run_action(action).map_err(ExtensionError::Action)
    }

    // ── internals ──────────────────────────────────────────────────────

    /// Grant a token for an extension in its `plugin.<id>` namespace.
    fn grant_for(&self, manifest: &ExtensionManifest) -> CapabilityToken {
        self.capabilities.grant(
            Namespace::new(format!("plugin.{}", manifest.id)),
            manifest.requested_permissions(),
            manifest.id.clone(),
            None,
            format!("extension {}", manifest.name),
        )
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join("registry.json")
    }

    fn read_state(&self) -> HashMap<String, bool> {
        std::fs::read_to_string(self.state_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn persist_state(&self) {
        let state: HashMap<String, bool> = self
            .records
            .read()
            .iter()
            .map(|r| (r.manifest.id.clone(), r.enabled))
            .collect();
        match serde_json::to_string_pretty(&state) {
            Ok(json) => {
                if let Err(e) = std::fs::write(self.state_path(), json) {
                    warn!(error = %e, "Failed to persist extension state");
                }
            }
            Err(e) => warn!(error = %e, "Failed to serialize extension state"),
        }
    }
}

fn record_info(r: &ExtensionRecord) -> ExtensionInfo {
    ExtensionInfo {
        id: r.manifest.id.clone(),
        name: r.manifest.name.clone(),
        version: r.manifest.version.clone(),
        author: r.manifest.author.clone(),
        description: r.manifest.description.clone(),
        kind: r.manifest.kind,
        enabled: r.enabled,
        permissions: r
            .manifest
            .permissions
            .iter()
            .map(|p| PermissionInfo {
                permission: format!("{:?}", p.permission),
                justification: p.justification.clone(),
            })
            .collect(),
    }
}

fn keyword_match(keywords: &[String], input_lower: &str) -> bool {
    keywords.is_empty() || keywords.iter().any(|k| input_lower.contains(&k.to_lowercase()))
}

/// Perform the OS side of an extension action via argv spawns (no shell).
fn run_action(action: &ExtensionAction) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    match action {
        ExtensionAction::OpenUrl { url } => Command::new("open")
            .arg("--")
            .arg(url)
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        ExtensionAction::OpenPath { path } => Command::new("open")
            .arg("--")
            .arg(path)
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        ExtensionAction::Copy { text } => {
            let mut child = Command::new("pbcopy")
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            }
            child.wait().map(|_| ()).map_err(|e| e.to_string())
        }
    }
}

/// Recursively copy a directory tree.
fn copy_dir(src: &Path, dest: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
            // Preserve the executable bit so script entrypoints stay runnable.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&from) {
                    let _ = std::fs::set_permissions(&to, std::fs::Permissions::from_mode(meta.permissions().mode()));
                }
            }
        }
    }
    Ok(())
}

// These tests build `#!/bin/sh` script extensions and `chmod +x` them, so they
// are unix-only (macOS + Linux); Windows packaging is exercised elsewhere.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn make_registry() -> (ExtensionRegistry, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let caps = Arc::new(CapabilityRegistry::new());
        let gate = Arc::new(CapabilityGate::new(caps.clone()));
        let reg = ExtensionRegistry::new(dir.path().to_path_buf(), caps, gate);
        (reg, dir)
    }

    fn sample_extension(root: &Path, id: &str) -> PathBuf {
        let ext = root.join(id);
        fs::create_dir_all(&ext).unwrap();
        fs::write(
            ext.join("manifest.toml"),
            format!(
                "id = \"{id}\"\nname = \"Test\"\nversion = \"1.0.0\"\nkind = \"script\"\nentrypoint = \"run.sh\"\n\n[[permissions]]\npermission = \"NetworkConnect\"\njustification = \"test\"\n"
            ),
        )
        .unwrap();
        let script = ext.join("run.sh");
        fs::write(&script, "#!/bin/sh\nprintf '[{\"title\":\"R:%s\"}]' \"$1\"\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        ext
    }

    #[test]
    fn install_enable_query_uninstall_lifecycle() {
        let (reg, dir) = make_registry();
        reg.load().unwrap();

        // Install from a staging dir.
        let staging = tempfile::tempdir().unwrap();
        let src = sample_extension(staging.path(), "demo");
        let id = reg.install(&src).unwrap();
        assert_eq!(id, "demo");

        // Installed but disabled → not consulted.
        assert_eq!(reg.list().len(), 1);
        assert!(!reg.list()[0].enabled);
        assert!(reg.query("hello").is_empty());

        // Enable → token granted, query runs.
        reg.set_enabled("demo", true).unwrap();
        assert!(reg.list()[0].enabled);
        let results = reg.query("hi");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "R:hi");

        // State persists across reload.
        let reloaded = ExtensionRegistry::new(
            dir.path().to_path_buf(),
            Arc::new(CapabilityRegistry::new()),
            Arc::new(CapabilityGate::new(Arc::new(CapabilityRegistry::new()))),
        );
        reloaded.load().unwrap();
        assert!(reloaded.list()[0].enabled);

        // Disable → not consulted again.
        reg.set_enabled("demo", false).unwrap();
        assert!(reg.query("hi").is_empty());

        // Uninstall → gone.
        reg.uninstall("demo").unwrap();
        assert!(reg.list().is_empty());
    }

    #[test]
    fn disabled_extension_action_is_denied() {
        let (reg, _dir) = make_registry();
        reg.load().unwrap();
        let staging = tempfile::tempdir().unwrap();
        let src = sample_extension(staging.path(), "demo");
        reg.install(&src).unwrap();
        // Not enabled → no token → action denied (never touches the OS).
        let err = reg
            .execute_action("demo", &ExtensionAction::Copy { text: "x".into() })
            .unwrap_err();
        assert!(matches!(err, ExtensionError::PermissionDenied(_)));
    }

    #[test]
    fn action_without_granted_permission_is_denied() {
        let (reg, _dir) = make_registry();
        reg.load().unwrap();
        let staging = tempfile::tempdir().unwrap();
        let src = sample_extension(staging.path(), "demo"); // grants NetworkConnect only
        reg.install(&src).unwrap();
        reg.set_enabled("demo", true).unwrap();
        // Copy needs ClipboardWrite, which this extension did not request.
        let err = reg
            .execute_action("demo", &ExtensionAction::Copy { text: "x".into() })
            .unwrap_err();
        assert!(matches!(err, ExtensionError::PermissionDenied(_)));
    }
}
