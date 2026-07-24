//! # Gate B — Single Extension Runtime Proof
//!
//! Proves the runtime contract end-to-end for exactly one extension, with nothing
//! else in the loop: no search, no registry, no marketplace, no Hydrator.
//!
//! ```text
//! TypeScript (examples/hello-world)
//!       -> esbuild bundle (examples/hello-world/dist/bundle.js, built ahead of test run)
//!       -> SandboxAllocator::allocate
//!       -> V8 executes the bundle
//!       -> postUiSync() -> MessagePack encode (JS side)
//!       -> Deno.core.ops.op_ipc_post -> MessagePack decode (Rust side)
//!       -> IpcEnvelope::UiSync received on the Guest->Host channel
//! ```
//!
//! Deliberately does NOT assert anything about the reconciler's Fiber lifecycle,
//! the Hydrator, or application-level routing — those are separate, already-tested
//! surfaces. This test answers exactly one question: does the wire protocol between
//! a compiled extension bundle and the Rust host work, unchanged, end-to-end?

use std::path::PathBuf;

use supersearch_runtime::extension::ipc::EnvelopeType;
use supersearch_runtime::extension::manifest::{ExtensionKind, ExtensionManifest};
use supersearch_runtime::extension::runtime::allocator::SandboxAllocator;

fn hello_world_manifest() -> ExtensionManifest {
    ExtensionManifest {
        id: "hello-world".into(),
        name: "Hello World".into(),
        version: "1.0.0".into(),
        author: None,
        description: None,
        kind: ExtensionKind::Js,
        entrypoint: "dist/bundle.js".into(),
        keywords: vec![],
        permissions: vec![],
        commands: vec![],
    }
}

fn hello_world_bundle_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-world/dist/bundle.js")
}

/// Navigate an `rmpv::Value` map by string key. Returns `None` if the value isn't
/// a map or the key is absent — never panics on unexpected shape.
fn map_get<'a>(value: &'a rmpv::Value, key: &str) -> Option<&'a rmpv::Value> {
    value
        .as_map()?
        .iter()
        .find(|(k, _)| k.as_str() == Some(key))
        .map(|(_, v)| v)
}

#[tokio::test]
async fn gate_b_extension_ui_sync_round_trips_through_the_real_bundle() {
    let bundle_path = hello_world_bundle_path();
    let bundle_source = std::fs::read_to_string(&bundle_path).unwrap_or_else(|e| {
        panic!(
            "Gate B fixture missing at {}: {e}. Build it first: \
             ./packages/cli/node_modules/.bin/esbuild examples/hello-world/src/index.ts \
             --bundle --format=iife --platform=browser --target=es2022 \
             --outfile=examples/hello-world/dist/bundle.js",
            bundle_path.display()
        )
    });

    // dev_mode = true: Gate B validates the runtime contract, not the signing
    // pipeline (that boundary is already covered by `signature.rs`'s own tests).
    let mut isolate =
        SandboxAllocator::allocate(hello_world_manifest(), bundle_source.as_bytes(), None, true)
            .expect("dev-mode allocation must not require a signature");

    isolate
        .evaluate_script("bundle.js", &bundle_source)
        .expect("bundle must execute without throwing");

    let envelope = isolate
        .rx
        .try_recv()
        .expect("op_ipc_post must have queued exactly one envelope synchronously");

    assert_eq!(envelope.0, 1, "protocol version must be 1");
    assert_eq!(envelope.2, EnvelopeType::UiSync);
    assert_eq!(envelope.3, 0, "UiSync envelopes carry no request id");

    let root = &envelope.5;
    assert_eq!(map_get(root, "type").and_then(|v| v.as_str()), Some("ROOT"));

    let children = map_get(root, "children")
        .and_then(|v| v.as_array())
        .expect("root.children must be an array");
    assert_eq!(children.len(), 1);

    let first_child = &children[0];
    assert_eq!(
        map_get(first_child, "type").and_then(|v| v.as_str()),
        Some("List.Item")
    );

    let title = map_get(first_child, "props")
        .and_then(|props| map_get(props, "title"))
        .and_then(|v| v.as_str());
    assert_eq!(title, Some("Hello from the sandbox"));
}

/// Failure-path companion to the success test above: a guest that posts corrupt
/// MessagePack must fail predictably (a JS exception surfaced through
/// `evaluate_script`), never panic the host or silently swallow the error.
#[tokio::test]
async fn gate_b_malformed_ipc_payload_is_rejected_not_panicked() {
    let mut isolate = SandboxAllocator::allocate(hello_world_manifest(), b"", None, true)
        .expect("dev-mode allocation must not require a signature");

    // No bundler involved here on purpose: this is a hostile guest, not the
    // well-formed fixture, so it is authored as a raw script.
    let hostile_script = r#"
        Deno.core.ops.op_ipc_post(new Uint8Array([0xff, 0xff, 0xff, 0xff]));
    "#;

    let result = isolate.evaluate_script("hostile.js", hostile_script);

    assert!(
        result.is_err(),
        "malformed MessagePack must surface as a script error, not succeed silently"
    );

    // The host process is still alive to make this assertion at all, which is
    // itself part of the proof: op_ipc_post's IpcMalformed path did not abort V8.
    assert!(
        isolate.rx.try_recv().is_err(),
        "a rejected envelope must never reach the Guest->Host channel"
    );
}
