//! # Gate D — Developer Experience & E2E Validation
//!
//! Validates the end-to-end integration and resilience of the extension platform
//! exactly as a third-party developer would experience it.
//!
//! Includes three primary automated regression tests:
//! 1. **Hello World:** Golden path (Install → Discover → Search → Launch).
//! 2. **Broken Extension:** A malformed manifest is safely ignored.
//! 3. **Runtime Failure:** An extension bundle throwing a JS error is caught safely.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use supersearch_runtime::capability::gate::CapabilityGate;
use supersearch_runtime::capability::registry::CapabilityRegistry;
use supersearch_runtime::extension::manifest::ExtensionKind;
use supersearch_runtime::extension::registry::ExtensionRegistry;
use supersearch_runtime::extension::runtime::V8Isolate;
use supersearch_runtime::extension::ipc::EnvelopeType;

/// Helper to create an ephemeral registry
fn make_registry() -> (ExtensionRegistry, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let caps = Arc::new(CapabilityRegistry::new());
    let gate = Arc::new(CapabilityGate::new(caps.clone()));
    let reg = ExtensionRegistry::new(dir.path().to_path_buf(), caps, gate);
    (reg, dir)
}

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
}

#[tokio::test(flavor = "current_thread")]
async fn e2e_validation_suite() {
    println!("==> Running hello world");
    test_e2e_hello_world_golden_path();
    println!("==> Running broken manifest");
    test_e2e_broken_extension_manifest();
    println!("==> Running runtime failure");
    test_e2e_runtime_failure_is_caught();
    println!("==> All done");
}

fn test_e2e_hello_world_golden_path() {
    let (reg, _dir) = make_registry();
    
    // 1. Install
    let src = examples_dir().join("hello-world");
    let ext_id = reg.install(&src).expect("installation of hello-world fixture failed");
    assert_eq!(ext_id, "hello-world");
    
    // 2. Discover (Load)
    reg.load().expect("load failed");
    
    // Extensions install disabled by default. Enable it.
    reg.set_enabled("hello-world", true).unwrap();
    
    // 3. Search
    // Simulate user typing "hello" into the global search bar
    let hits = reg.query("hello");
    assert_eq!(hits.len(), 1, "expected the hello-world extension to match the query");
    assert_eq!(hits[0].extension_id, "hello-world");
    assert_eq!(hits[0].kind, ExtensionKind::Js, "JS extensions must carry the correct kind");
    assert!(hits[0].action.is_none(), "JS extensions produce synthetic launcher rows without script actions");
    
    // 4. Launch & Render Proxy
    // The Tauri app handles this by invoking `launch_extension` which boots a V8Isolate.
    // We simulate the isolate boot and verify it produces the UiSync message.
    let records = reg.list();
    let _info = records.iter().find(|i| i.id == "hello-world").unwrap();
    
    // For the test, we bypass reading from disk again and just fetch the manifest/bundle
    let manifest_text = fs::read_to_string(src.join("manifest.json")).or_else(|_| fs::read_to_string(src.join("manifest.toml"))).unwrap();
    let manifest = supersearch_runtime::extension::manifest::ExtensionManifest::from_toml(&manifest_text).unwrap();
    
    let bundle_path = src.join("dist/bundle.js");
    let bundle_src = fs::read_to_string(&bundle_path).expect("bundle.js not found. did you run `npm run build` in examples/hello-world?");
    
    let mut isolate = V8Isolate::new(manifest);
    isolate.evaluate_script("extension:bundle.js", &bundle_src).expect("evaluation failed");
    
    // Drain events looking for UiSync (which means React rendered)
    let mut received_ui_sync = false;
    while let Ok(envelope) = isolate.rx.try_recv() {
        if envelope.2 == EnvelopeType::UiSync {
            received_ui_sync = true;
            break;
        }
    }
    
    assert!(received_ui_sync, "The JS bundle must execute and post a UiSync envelope back to the host");
}

fn test_e2e_broken_extension_manifest() {
    let (reg, dir) = make_registry();
    
    // Setup a broken extension directly in the registry directory
    let broken_dir = dir.path().join("broken-ext");
    fs::create_dir_all(&broken_dir).unwrap();
    
    // Manifest is invalid TOML
    fs::write(broken_dir.join("manifest.toml"), "id = \"broken\" \n kind = ").unwrap();
    
    // Setup a working extension as a control
    let working_dir = dir.path().join("working-ext");
    fs::create_dir_all(&working_dir).unwrap();
    fs::write(working_dir.join("manifest.toml"), "id = \"working\"\nname = \"Working\"\nversion = \"1.0.0\"\nkind = \"js\"\nentrypoint = \"dist/bundle.js\"\nkeywords = []\n").unwrap();
    
    // 1. Discover
    reg.load().expect("Registry should never panic or error out completely on a single bad extension");
    
    // 2. Verify
    let list = reg.list();
    assert_eq!(list.len(), 1, "The broken extension should be skipped, but the working one loaded");
    assert_eq!(list[0].id, "working");
}

fn test_e2e_runtime_failure_is_caught() {
    // 1. Create a minimal valid manifest but a throwing JS bundle
    let staging = tempfile::tempdir().unwrap();
    let src = staging.path().join("throwing-ext");
    fs::create_dir_all(&src).unwrap();
    
    let manifest_text = "id = \"thrower\"\nname = \"Thrower\"\nversion = \"1.0.0\"\nkind = \"js\"\nentrypoint = \"dist/bundle.js\"\nkeywords = []\n";
    fs::write(src.join("manifest.toml"), manifest_text).unwrap();
    
    let manifest = supersearch_runtime::extension::manifest::ExtensionManifest::from_toml(manifest_text).unwrap();
    
    // A bundle that throws immediately upon execution
    let bundle_src = "throw new Error('This is a simulated runtime crash');";
    
    // 2. Launch
    let mut isolate = V8Isolate::new(manifest);
    let result = isolate.evaluate_script("extension:bundle.js", bundle_src);
    
    // 3. Verify Error is caught and surfaced, not panicking the host
    assert!(result.is_err(), "The script execution must return an Error");
    
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("This is a simulated runtime crash"), "The exact JS error should be surfaced to the host");
}
