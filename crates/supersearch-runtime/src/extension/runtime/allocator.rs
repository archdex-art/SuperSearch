//! # Sandbox Allocator (Phase 4 M1)
//!
//! Owns the V8 initialization and acts as the factory for new isolated instances.
//! It ensures that `deno_core` is correctly bootstrapped once per process.

use std::sync::Once;

use super::isolate::V8Isolate;
use crate::extension::manifest::ExtensionManifest;

static INIT_V8: Once = Once::new();

/// The factory for creating strict V8 isolations.
pub struct SandboxAllocator;

impl SandboxAllocator {
    /// Initialize the process-wide V8 platform. Must be called before allocating.
    pub fn init_platform() {
        INIT_V8.call_once(|| {
            // Set any necessary v8 flags before init.
            // e.g., turning off unneeded WebAssembly or profiling unless in dev mode.
            let flags = vec![
                "supersearch".to_string(),      // argv[0] dummy
                "--no-expose-wasm".to_string(), // Explicitly disable WASM inside V8 to reduce attack surface
            ];
            deno_core::v8_set_flags(flags);
        });
    }

    /// Allocates a new, cold V8 sandbox for the given extension manifest.
    ///
    /// Enforces Ed25519 signature verification unless `dev_mode` is explicitly enabled.
    /// The capability resolver acts as a gatekeeper here, ensuring only authorized
    /// operations are injected into the runtime options of the isolate.
    pub fn allocate(
        manifest: ExtensionManifest,
        bundle: &[u8],
        signature: Option<&[u8]>,
        dev_mode: bool,
    ) -> Result<V8Isolate, crate::capability::signature::SignatureError> {
        if !dev_mode {
            let sig =
                signature.ok_or(crate::capability::signature::SignatureError::UnsignedBundle)?;
            crate::capability::signature::verify_bundle(
                bundle,
                sig,
                crate::capability::signature::MARKETPLACE_PUBLIC_KEY,
            )?;
        }

        Self::init_platform();
        // In M1, we allocate cold instances.
        // In later milestones, this will pull from a warm snapshot cache.
        Ok(V8Isolate::new(manifest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::token::Permission;
    use crate::extension::manifest::{ExtensionKind, PermissionRequest};

    fn dummy_manifest() -> ExtensionManifest {
        ExtensionManifest {
            id: "test-ext".into(),
            name: "Test Ext".into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            kind: ExtensionKind::Js,
            entrypoint: "bundle.js".into(),
            keywords: vec![],
            permissions: vec![PermissionRequest {
                permission: Permission::NetworkConnect,
                justification: "test".into(),
            }],
            commands: vec![],
        }
    }

    #[tokio::test]
    async fn test_allocator_and_evaluation() {
        let mut isolate =
            SandboxAllocator::allocate(dummy_manifest(), b"10 + 32", None, true).unwrap();

        // Execute a simple script to verify functionality
        let result = isolate.evaluate_script("test.js", "10 + 32");
        assert!(result.is_ok());

        // Execute a script creating an object
        let result2 = isolate.evaluate_script("test2.js", "JSON.stringify({ hello: 'world' })");
        assert!(result2.is_ok());
    }
}
