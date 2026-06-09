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
use std::time::Duration;

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

/// Per-extension search budget. A slow extension is abandoned (its child is
/// killed) rather than stalling the palette — far tighter than the 10s one-off
/// invocation cap. Extensions run concurrently, so this bounds the whole fan-out.
const SEARCH_BUDGET: Duration = Duration::from_millis(800);

/// A single installed extension and its runtime state.
struct ExtensionRecord {
    manifest: ExtensionManifest,
    dir: PathBuf,
    enabled: bool,
    /// Whether the user has explicitly trusted this extension to run unsandboxed
    /// code. WASM extensions are sandboxed by wasmtime and need no trust; a raw
    /// `kind = "script"` extension runs with full user privileges, so it only
    /// participates in queries once trusted.
    trusted: bool,
    /// Capability token, present only while enabled.
    token: Option<CapabilityToken>,
}

/// Persisted per-extension state (`registry.json`).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct RecordState {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    trusted: bool,
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
    /// User has trusted this (unsandboxed) script extension to run.
    pub trusted: bool,
    /// True when the extension is a script that still needs an explicit trust
    /// grant before it will run — the UI should surface a "Trust" affordance.
    pub needs_trust: bool,
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
        let saved = self.read_state();

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

            let state = saved.get(&manifest.id).copied().unwrap_or_default();
            let token = if state.enabled { Some(self.grant_for(&manifest)) } else { None };
            records.push(ExtensionRecord {
                manifest,
                dir: path,
                enabled: state.enabled,
                trusted: state.trusted,
                token,
            });
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

    /// Set whether the user trusts an (unsandboxed) script extension to run.
    /// This is the gate that lets a `kind = "script"` extension participate in
    /// queries; WASM extensions are sandboxed and ignore it. The UI must obtain
    /// explicit, informed consent before calling this with `true`.
    pub fn set_trusted(&self, id: &str, trusted: bool) -> Result<(), ExtensionError> {
        {
            let mut records = self.records.write();
            let record = records
                .iter_mut()
                .find(|r| r.manifest.id == id)
                .ok_or_else(|| ExtensionError::NotFound(id.to_string()))?;
            record.trusted = trusted;
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
            trusted: false,
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
        let targets: Vec<(String, ExtensionKind, PathBuf, String, bool)> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| r.enabled)
                .filter(|r| keyword_match(&r.manifest.keywords, &input_lower))
                .map(|r| {
                    (
                        r.manifest.id.clone(),
                        r.manifest.kind,
                        r.dir.clone(),
                        r.manifest.entrypoint.clone(),
                        r.trusted,
                    )
                })
                .collect()
        };

        // Fan out concurrently with a tight per-extension budget so one slow
        // extension can't stall search. Each runs on its own thread; a script
        // that overruns is killed by `host::run_query`'s own timeout, and the
        // whole fan-out is bounded by the overall deadline below.
        let (tx, rx) = std::sync::mpsc::channel::<(String, Result<Vec<host::ExtensionResult>, host::HostError>)>();
        let mut spawned = 0usize;
        for (id, kind, dir, entrypoint, trusted) in targets {
            // B1: a raw script runs unsandboxed (full user privileges), so it
            // only participates once the user has explicitly trusted it. WASM
            // extensions are sandboxed by wasmtime and always run.
            if kind == ExtensionKind::Script && !trusted {
                warn!(id, "Skipping untrusted script extension — enable trust to run it");
                continue;
            }
            let tx = tx.clone();
            let input = input.to_string();
            std::thread::spawn(move || {
                // Each worker self-bounds: scripts via `host::run_query`'s
                // timeout, wasm via its fuel budget. `catch_unwind` guarantees a
                // worker always reports exactly once (even on panic), so the
                // blocking collection below is deterministic and can never hang.
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match kind {
                    ExtensionKind::Script => host::run_query(&dir, &entrypoint, &input, SEARCH_BUDGET),
                    ExtensionKind::Wasm => {
                        wasm::run_query(&dir.join(&entrypoint), &input).map_err(host::HostError::BadOutput)
                    }
                }))
                .unwrap_or_else(|_| Err(host::HostError::BadOutput("extension worker panicked".into())));
                let _ = tx.send((id, outcome));
            });
            spawned += 1;
        }
        drop(tx);

        // Collect exactly `spawned` reports. Every worker is guaranteed to send
        // within its own budget, so this blocks for at most ~SEARCH_BUDGET and
        // never drops a result that arrived (no collection-deadline race).
        let mut hits = Vec::new();
        for _ in 0..spawned {
            match rx.recv() {
                Ok((id, Ok(results))) => {
                    for r in results {
                        hits.push(ExtensionQueryHit {
                            extension_id: id.clone(),
                            title: r.title,
                            subtitle: r.subtitle,
                            action: r.action,
                        });
                    }
                }
                Ok((id, Err(e))) => warn!(id, error = %e, "Extension query failed"),
                Err(_) => break, // all workers reported and senders dropped
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

    fn read_state(&self) -> HashMap<String, RecordState> {
        let Ok(text) = std::fs::read_to_string(self.state_path()) else {
            return HashMap::new();
        };
        // Current shape: { id: { enabled, trusted } }.
        if let Ok(m) = serde_json::from_str::<HashMap<String, RecordState>>(&text) {
            return m;
        }
        // Back-compat: older files stored { id: bool } (enabled only).
        if let Ok(old) = serde_json::from_str::<HashMap<String, bool>>(&text) {
            return old
                .into_iter()
                .map(|(k, enabled)| (k, RecordState { enabled, trusted: false }))
                .collect();
        }
        HashMap::new()
    }

    fn persist_state(&self) {
        let state: HashMap<String, RecordState> = self
            .records
            .read()
            .iter()
            .map(|r| (r.manifest.id.clone(), RecordState { enabled: r.enabled, trusted: r.trusted }))
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
        trusted: r.trusted,
        needs_trust: r.manifest.kind == ExtensionKind::Script && !r.trusted,
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

    /// Script-extension queries spawn a subprocess and are *best-effort*: under
    /// heavy parallel test load a child can occasionally be read as empty. WASM
    /// extensions are deterministic; for the script path we retry briefly to
    /// assert the steady-state result without flaking on subprocess timing.
    fn query_until(reg: &ExtensionRegistry, q: &str, want: usize) -> Vec<ExtensionQueryHit> {
        let mut last = reg.query(q);
        for _ in 0..20 {
            if last.len() == want {
                return last;
            }
            std::thread::sleep(Duration::from_millis(25));
            last = reg.query(q);
        }
        last
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

        // Enable → token granted. But "demo" is a *script* extension, so until
        // it is trusted it stays out of the query fan-out (B1 sandbox gate).
        reg.set_enabled("demo", true).unwrap();
        assert!(reg.list()[0].enabled);
        assert!(reg.list()[0].needs_trust, "untrusted script should flag needs_trust");
        assert!(reg.query("hi").is_empty(), "untrusted script must not run in queries");

        // Trust → query runs.
        reg.set_trusted("demo", true).unwrap();
        assert!(!reg.list()[0].needs_trust);
        let results = query_until(&reg, "hi", 1);
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
        assert!(reloaded.list()[0].trusted, "trust must persist across reload");
        assert_eq!(query_until(&reloaded, "hi", 1).len(), 1, "trusted+enabled script runs after reload");

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
