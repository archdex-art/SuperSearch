//! # Extension Discovery & Launch (Phase 7 / Gate C)
//!
//! The minimal, verifiable slice of Application Integration: find `kind: "js"`
//! extensions on disk, and launch one to completion (bundle executes, UI tree
//! arrives on the Guest -> Host channel). Deliberately does not touch search
//! ranking, persistence, or lifecycle management — those are Gate D concerns.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::capability::signature::SignatureError;
use crate::extension::ipc::EnvelopeType;
use crate::extension::manifest::{ExtensionKind, ExtensionManifest, ManifestError};
use crate::extension::runtime::allocator::SandboxAllocator;

/// A `kind: "js"` extension found on disk, ready to launch.
#[derive(Debug, Clone)]
pub struct DiscoveredExtension {
    pub manifest: ExtensionManifest,
    /// Directory containing `manifest.json` and the entrypoint bundle.
    pub dir: PathBuf,
}

/// Scans the immediate subdirectories of `root` for `manifest.json` files
/// declaring `"kind": "js"`. Never panics on a single bad entry — a corrupted
/// manifest, an unreadable directory, or a duplicate id is logged and skipped
/// so one broken extension cannot take down discovery for every other one.
pub fn discover_js_extensions(root: &Path) -> Vec<DiscoveredExtension> {
    let mut found = Vec::new();
    let mut seen_ids = HashSet::new();

    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) => {
            warn!(root = %root.display(), error = %e, "extension discovery: root unreadable");
            return found;
        }
    };

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }

        let manifest_path = dir.join("manifest.json");
        let raw = match std::fs::read_to_string(&manifest_path) {
            Ok(raw) => raw,
            Err(_) => continue, // No manifest.json here — not an extension directory.
        };

        let manifest = match ExtensionManifest::from_json(&raw) {
            Ok(m) => m,
            Err(e) => {
                warn!(path = %manifest_path.display(), error = %e, "extension discovery: malformed manifest, skipping");
                continue;
            }
        };

        if let Err(e) = manifest.validate() {
            warn!(path = %manifest_path.display(), error = %e, "extension discovery: manifest failed validation, skipping");
            continue;
        }

        if manifest.kind != ExtensionKind::Js {
            continue;
        }

        if !seen_ids.insert(manifest.id.clone()) {
            warn!(id = %manifest.id, "extension discovery: duplicate id, keeping first, skipping duplicate");
            continue;
        }

        found.push(DiscoveredExtension { manifest, dir });
    }

    found
}

/// Errors from launching a discovered extension.
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("entrypoint not found or unreadable: {0}")]
    BundleUnreadable(#[from] std::io::Error),
    #[error("bundle signature rejected: {0}")]
    Signature(#[from] SignatureError),
    #[error("bundle threw during execution: {0}")]
    Execution(String),
    #[error("extension did not emit a UiSync envelope before returning")]
    NoUiSync,
    #[error("expected a UiSync envelope, got a different envelope type")]
    UnexpectedEnvelopeType,
    #[error("manifest validation failed: {0}")]
    Manifest(#[from] ManifestError),
}

/// Launches a discovered extension end-to-end and returns its initial UI tree as
/// plain `serde_json::Value`, ready to hand to the frontend Hydrator.
///
/// `dev_mode` bypasses Ed25519 signature verification — the same flag
/// `SandboxAllocator::allocate` already requires (see `signature.rs`); production
/// callers must pass `false` with a real detached signature.
pub fn launch_extension(
    ext: &DiscoveredExtension,
    dev_mode: bool,
) -> Result<serde_json::Value, LaunchError> {
    ext.manifest.validate()?;

    let bundle_path = ext.dir.join(&ext.manifest.entrypoint);
    let bundle_source = std::fs::read_to_string(&bundle_path)?;

    let mut isolate = SandboxAllocator::allocate(
        ext.manifest.clone(),
        bundle_source.as_bytes(),
        None,
        dev_mode,
    )?;

    isolate
        .evaluate_script("bundle.js", &bundle_source)
        .map_err(|e| LaunchError::Execution(e.to_string()))?;

    let envelope = isolate.rx.try_recv().map_err(|_| LaunchError::NoUiSync)?;

    if envelope.2 != EnvelopeType::UiSync {
        return Err(LaunchError::UnexpectedEnvelopeType);
    }

    Ok(rmpv_to_json(&envelope.5))
}

/// Converts an `rmpv::Value` (the wire representation the Guest sends) into plain
/// `serde_json::Value`. A manual walk rather than `serde_json::to_value` because
/// `rmpv::Value`'s own `Serialize` impl represents maps as key/value pair arrays,
/// not JSON objects — that shape is correct for MessagePack, wrong for JSON.
fn rmpv_to_json(v: &rmpv::Value) -> serde_json::Value {
    use rmpv::Value as V;
    match v {
        V::Nil => serde_json::Value::Null,
        V::Boolean(b) => serde_json::Value::Bool(*b),
        V::Integer(i) => i
            .as_i64()
            .map(serde_json::Value::from)
            .or_else(|| i.as_u64().map(serde_json::Value::from))
            .unwrap_or(serde_json::Value::Null),
        V::F32(f) => serde_json::Number::from_f64(*f as f64)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        V::F64(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        V::String(s) => serde_json::Value::String(s.as_str().unwrap_or_default().to_string()),
        V::Binary(b) => serde_json::Value::Array(
            b.iter()
                .map(|byte| serde_json::Value::from(*byte))
                .collect(),
        ),
        V::Array(items) => serde_json::Value::Array(items.iter().map(rmpv_to_json).collect()),
        V::Map(entries) => {
            let mut obj = serde_json::Map::with_capacity(entries.len());
            for (k, val) in entries {
                if let Some(key) = k.as_str() {
                    obj.insert(key.to_string(), rmpv_to_json(val));
                }
            }
            serde_json::Value::Object(obj)
        }
        V::Ext(_, bytes) => serde_json::Value::Array(
            bytes
                .iter()
                .map(|byte| serde_json::Value::from(*byte))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_no_extensions_in_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(discover_js_extensions(dir.path()).is_empty());
    }

    #[test]
    fn skips_malformed_manifest_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("broken-ext");
        std::fs::create_dir(&ext_dir).unwrap();
        std::fs::write(ext_dir.join("manifest.json"), "{ not valid json").unwrap();

        let found = discover_js_extensions(dir.path());
        assert!(found.is_empty(), "a malformed manifest must be skipped, not crash discovery");
    }

    #[test]
    fn skips_duplicate_ids_keeping_first() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["ext-a", "ext-b"] {
            let ext_dir = dir.path().join(name);
            std::fs::create_dir(&ext_dir).unwrap();
            std::fs::write(
                ext_dir.join("manifest.json"),
                r#"{"id":"dup","name":"Dup","version":"1.0.0","kind":"js","entrypoint":"dist/bundle.js"}"#,
            )
            .unwrap();
        }

        let found = discover_js_extensions(dir.path());
        assert_eq!(found.len(), 1, "duplicate ids must not both be registered");
    }

    #[tokio::test]
    async fn gate_c_discovers_and_launches_the_real_hello_world_fixture() {
        let examples_root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");

        let found = discover_js_extensions(&examples_root);
        let hello = found
            .iter()
            .find(|e| e.manifest.id == "hello-world")
            .unwrap_or_else(|| {
                panic!(
                    "hello-world fixture not discovered under {}; \
                     ensure examples/hello-world/manifest.json exists and dist/bundle.js is built",
                    examples_root.display()
                )
            });

        let ui_tree = launch_extension(hello, true).expect("hello-world must launch and render");

        assert_eq!(ui_tree["type"], "ROOT");
        assert_eq!(ui_tree["children"][0]["type"], "List.Item");
        assert_eq!(
            ui_tree["children"][0]["props"]["title"],
            "Hello from the sandbox"
        );
    }
}
