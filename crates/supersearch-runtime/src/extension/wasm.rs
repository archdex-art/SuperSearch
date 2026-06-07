//! WebAssembly extension host (v2 runtime).
//!
//! Runs a sandboxed `.wasm` extension to answer a query, with the same result
//! contract as script extensions. Sandboxing is enforced by wasmtime:
//! - **Fuel** caps total executed instructions (kills infinite loops).
//! - **Memory limit** caps linear-memory growth.
//! - No host imports are provided here, so a query module is pure compute;
//!   side-effecting actions it returns are still mediated by the capability
//!   gate when executed (same as scripts).
//!
//! ## Guest ABI
//! The module must export:
//! - `memory`
//! - `alloc(len: i32) -> i32` — return a writable buffer offset of `len` bytes
//! - `query(ptr: i32, len: i32) -> i64` — read the UTF-8 query at `ptr..ptr+len`
//!   and return a packed `(out_ptr << 32) | out_len` pointing at a UTF-8 JSON
//!   array of result rows (`[{title, subtitle?, action?}]`) in `memory`.

use std::path::Path;

use wasmtime::{Config, Engine, Instance, Module, Store, StoreLimits, StoreLimitsBuilder};

use super::host::ExtensionResult;

/// Instruction budget per query (wasmtime fuel). Generous but finite.
const FUEL: u64 = 50_000_000;
/// Linear-memory ceiling per instance.
const MAX_MEMORY: usize = 16 * 1024 * 1024;

struct HostState {
    limits: StoreLimits,
}

/// Run a `.wasm` extension at `path` for `query`, returning parsed results.
pub fn run_query(path: &Path, query: &str) -> Result<Vec<ExtensionResult>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read wasm: {e}"))?;
    run_query_bytes(&bytes, query)
}

/// As [`run_query`] but from in-memory module bytes or WAT text (used by tests).
pub fn run_query_bytes(module_src: &[u8], query: &str) -> Result<Vec<ExtensionResult>, String> {
    let mut config = Config::new();
    config.consume_fuel(true);
    let engine = Engine::new(&config).map_err(|e| format!("engine: {e}"))?;
    let module = Module::new(&engine, module_src).map_err(|e| format!("compile: {e}"))?;

    let state = HostState {
        limits: StoreLimitsBuilder::new().memory_size(MAX_MEMORY).build(),
    };
    let mut store = Store::new(&engine, state);
    store.limiter(|s| &mut s.limits);
    store.set_fuel(FUEL).map_err(|e| format!("fuel: {e}"))?;

    let instance = Instance::new(&mut store, &module, &[]).map_err(|e| format!("instantiate: {e}"))?;

    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or("module missing `memory` export")?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut store, "alloc")
        .map_err(|e| format!("missing alloc: {e}"))?;
    let query_fn = instance
        .get_typed_func::<(i32, i32), i64>(&mut store, "query")
        .map_err(|e| format!("missing query: {e}"))?;

    // Pass the query into guest memory.
    let q = query.as_bytes();
    let in_ptr = alloc.call(&mut store, q.len() as i32).map_err(|e| format!("alloc trap: {e}"))?;
    memory
        .write(&mut store, in_ptr as usize, q)
        .map_err(|e| format!("write input: {e}"))?;

    // Call query → packed (out_ptr << 32) | out_len.
    let packed = query_fn
        .call(&mut store, (in_ptr, q.len() as i32))
        .map_err(|e| format!("query trap: {e}"))?;
    let out_ptr = ((packed >> 32) & 0xFFFF_FFFF) as usize;
    let out_len = (packed & 0xFFFF_FFFF) as usize;

    let data = memory.data(&store);
    let end = out_ptr.checked_add(out_len).ok_or("result length overflow")?;
    if end > data.len() {
        return Err("result pointer out of bounds".into());
    }
    let json = &data[out_ptr..end];
    serde_json::from_slice::<Vec<ExtensionResult>>(json).map_err(|e| format!("bad result json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal conforming guest: a bump allocator + a `query` that returns a
    // static JSON result. Exercises the full ABI (alloc + memory write + call +
    // result read) without needing a wasm toolchain — wasmtime compiles WAT.
    const WAT: &str = r#"
        (module
          (memory (export "memory") 1)
          (global $heap (mut i32) (i32.const 1024))
          (data (i32.const 16) "[{\"title\":\"hello from wasm\"}]")
          (func (export "alloc") (param $n i32) (result i32)
            (local $p i32)
            (local.set $p (global.get $heap))
            (global.set $heap (i32.add (global.get $heap) (local.get $n)))
            (local.get $p))
          (func (export "query") (param i32 i32) (result i64)
            (i64.or
              (i64.shl (i64.const 16) (i64.const 32))
              (i64.const 29))))
    "#;

    #[test]
    fn runs_wat_guest_and_parses_results() {
        let results = run_query_bytes(WAT.as_bytes(), "anything").expect("wasm query");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "hello from wasm");
    }

    #[test]
    fn rejects_module_without_required_exports() {
        let bad = r#"(module (memory (export "memory") 1))"#;
        assert!(run_query_bytes(bad.as_bytes(), "x").is_err());
    }
}
