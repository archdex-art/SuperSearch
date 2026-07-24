//! # V8 Isolate Wrapper
//!
//! Provides a safe Rust wrapper around `deno_core::JsRuntime`.
//! This struct manages the lifecycle of a single extension's V8 execution context,
//! enforcing strict isolation and memory quotas.

use crate::extension::manifest::ExtensionManifest;
use deno_core::{v8, JsRuntime, RuntimeOptions};
use std::rc::Rc;
use tokio::sync::{mpsc, Mutex};

/// Minimal, pure-JS `TextEncoder`/`TextDecoder`. Standard serialization libraries
/// (including `@msgpack/msgpack`) assume these Web Platform globals exist; `deno_core`
/// deliberately does not provide them (Phase 9: zero ambient APIs beyond registered
/// ops). This is scoped narrowly to UTF-8 codec behavior — it grants no I/O, no
/// capability, and no access outside the guest's own memory.
const TEXT_CODEC_POLYFILL: &str = r#"
class TextEncoder {
    encode(str) {
        const bytes = [];
        for (let i = 0; i < str.length; i++) {
            let code = str.charCodeAt(i);
            if (code < 0x80) {
                bytes.push(code);
            } else if (code < 0x800) {
                bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
            } else if (code >= 0xd800 && code < 0xe000) {
                code = 0x10000 + (((code & 0x3ff) << 10) | (str.charCodeAt(++i) & 0x3ff));
                bytes.push(
                    0xf0 | (code >> 18),
                    0x80 | ((code >> 12) & 0x3f),
                    0x80 | ((code >> 6) & 0x3f),
                    0x80 | (code & 0x3f),
                );
            } else {
                bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
            }
        }
        return new Uint8Array(bytes);
    }
}

class TextDecoder {
    constructor(encoding) { this.encoding = encoding || "utf-8"; }
    decode(bytes) {
        let out = "";
        let i = 0;
        while (i < bytes.length) {
            const b0 = bytes[i++];
            if (b0 < 0x80) {
                out += String.fromCharCode(b0);
            } else if (b0 < 0xe0) {
                out += String.fromCharCode(((b0 & 0x1f) << 6) | (bytes[i++] & 0x3f));
            } else if (b0 < 0xf0) {
                const b1 = bytes[i++], b2 = bytes[i++];
                out += String.fromCharCode(((b0 & 0x0f) << 12) | ((b1 & 0x3f) << 6) | (b2 & 0x3f));
            } else {
                const b1 = bytes[i++], b2 = bytes[i++], b3 = bytes[i++];
                let cp = ((b0 & 0x07) << 18) | ((b1 & 0x3f) << 12) | ((b2 & 0x3f) << 6) | (b3 & 0x3f);
                cp -= 0x10000;
                out += String.fromCharCode(0xd800 + (cp >> 10), 0xdc00 + (cp & 0x3ff));
            }
        }
        return out;
    }
}

globalThis.TextEncoder = TextEncoder;
globalThis.TextDecoder = TextDecoder;
"#;

/// STOPGAP — tracked as follow-up debt, not a final implementation.
///
/// `react-reconciler`'s host config binds `scheduleTimeout: setTimeout` at module
/// load time, so any bundle importing it needs `setTimeout`/`clearTimeout` to exist
/// merely to finish loading — independent of whether the extension ever uses timing
/// itself. `deno_core` has no timer globals without a dedicated op backed by
/// `tokio::time`, which is real scheduler work, not something to improvise inline.
///
/// This polyfill unblocks module loading by running the callback on the microtask
/// queue immediately, ignoring `delay`. It is intentionally NOT a correct timer:
/// nothing in the reconciler's synchronous commit path currently depends on actual
/// elapsed time, so this is safe for now but must be replaced with a real
/// `op_set_timeout`/`op_clear_timeout` pair backed by `tokio::time::sleep` before
/// any extension relies on timing behavior (debounce, polling, animation, etc.).
const TIMER_STOPGAP_POLYFILL: &str = r#"
globalThis.setTimeout = (fn) => { queueMicrotask(fn); return 0; };
globalThis.clearTimeout = () => {};
"#;

/// An isolated V8 execution context for a single extension.
pub struct V8Isolate {
    /// The underlying Deno runtime embedding V8.
    pub runtime: JsRuntime,
    /// The parsed manifest identifying this sandbox owner.
    pub manifest: ExtensionManifest,
    /// Receiver channel for messages bound for the Rust host scheduler (Guest -> Host)
    pub rx: mpsc::Receiver<crate::extension::ipc::IpcEnvelope>,
    /// Sender channel for pushing messages to the V8 guest (Host -> Guest)
    pub tx: mpsc::Sender<crate::extension::ipc::IpcEnvelope>,
}

impl V8Isolate {
    /// Create a new V8 sandbox for `manifest`, applying the 50MB memory limit.
    pub fn new(manifest: ExtensionManifest) -> Self {
        // Two-way channels: Guest -> Host (Backpressured) and Host -> Guest (Unbounded or bounded)
        let (guest_to_host_tx, guest_to_host_rx) = mpsc::channel(32);
        let (host_to_guest_tx, host_to_guest_rx) = mpsc::channel(32);
        // As defined in Phase 10: "Active Isolate (Guest) Target 5-20MB, Hard Limit 50MB"
        // We configure V8 to respect these boundaries.
        let create_params = v8::CreateParams::default().heap_limits(0, 50 * 1024 * 1024);

        let options = RuntimeOptions {
            create_params: Some(create_params),
            // Ops will be registered here (e.g. storage, capability-gated net)
            extensions: vec![super::ops::supersearch_ipc::init()],
            ..Default::default()
        };
        let mut runtime = JsRuntime::new(options);

        // Inject channels into the OpState
        let op_state = runtime.op_state();
        op_state.borrow_mut().put(guest_to_host_tx);
        op_state
            .borrow_mut()
            .put(Rc::new(Mutex::new(host_to_guest_rx)));
        drop(op_state);
        let handle = runtime.v8_isolate().thread_safe_handle();
        runtime.add_near_heap_limit_callback(move |current, _initial| {
            handle.terminate_execution();
            current + 5 * 1024 * 1024 // Give V8 breathing room to process the termination
        });

        runtime
            .execute_script("supersearch:bootstrap.js", TEXT_CODEC_POLYFILL)
            .expect("bootstrap polyfill is host-authored and must never fail");
        runtime
            .execute_script("supersearch:bootstrap-timers.js", TIMER_STOPGAP_POLYFILL)
            .expect("bootstrap polyfill is host-authored and must never fail");

        Self {
            runtime,
            manifest,
            rx: guest_to_host_rx,
            tx: host_to_guest_tx,
        }
    }

    /// Evaluates a raw string of JavaScript synchronously.
    /// In the final architecture, this runs the compiled, minified bundle
    /// and listens to the async event loop.
    ///
    /// # Errors
    /// Returns an error if the JavaScript throws an unhandled exception or fails to parse.
    pub fn evaluate_script(
        &mut self,
        source_name: &'static str,
        source: &str,
    ) -> Result<(), deno_core::error::AnyError> {
        // Execute the script
        self.runtime
            .execute_script(source_name, source.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::manifest::ExtensionKind;

    fn dummy_manifest() -> ExtensionManifest {
        ExtensionManifest {
            id: "oom-ext".into(),
            name: "OOM Ext".into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            kind: ExtensionKind::Js,
            entrypoint: "bundle.js".into(),
            keywords: vec![],
            permissions: vec![],
            commands: vec![],
        }
    }

    #[tokio::test]
    async fn test_memory_quota_enforcement() {
        let mut isolate = V8Isolate::new(dummy_manifest());
        // A script designed to allocate large arrays indefinitely until the 50MB limit is hit.
        let hostile_script = r#"
            let arr = [];
            while (true) {
                arr.push(new Array(1024 * 1024).join('a')); // Allocate large strings in a loop
            }
        "#;

        // Evaluating this script should quickly result in an Out Of Memory (OOM) termination
        // by the V8 engine, rather than crashing the Rust host.
        let result = isolate.evaluate_script("hostile.js", hostile_script);

        // We expect an error reflecting resource exhaustion/termination.
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();

        // The error usually indicates an execution termination or memory allocation failure
        // depending on how deno_core/v8 bubbles up the OOM.
        let is_oom = err_str.contains("Isolate")
            || err_str.contains("JavaScript execution")
            || err_str.contains("memory")
            || err_str.contains("Allocation")
            || err_str.contains("terminated")
            || err_str.contains("fatal");
        assert!(is_oom, "Unexpected error format: {}", err_str);
    }

    #[tokio::test]
    async fn test_text_codec_polyfill_round_trips_multibyte_utf8() {
        let mut isolate = V8Isolate::new(dummy_manifest());
        // "café 🎉" exercises 1-byte, 2-byte, and 4-byte (surrogate pair) UTF-8 paths.
        let script = r#"
            const encoded = new TextEncoder().encode("café 🎉");
            const decoded = new TextDecoder().decode(encoded);
            if (decoded !== "café 🎉") {
                throw new Error(`round-trip mismatch: got ${JSON.stringify(decoded)}`);
            }
        "#;
        isolate
            .evaluate_script("codec_test.js", script)
            .expect("TextEncoder/TextDecoder must round-trip multi-byte UTF-8 correctly");
    }
}
