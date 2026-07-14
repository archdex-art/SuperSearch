# Extensions

An extension is a directory with a `manifest.toml` and an entrypoint,
installed under
`~/Library/Application Support/com.supersearch.app/extensions/`. Two
execution models share one registry:

| Model | Status | Sandbox |
|---|---|---|
| **Script** | Available now | Native subprocess, argv only (no shell), hard 10s timeout |
| **WASM** | Manifest support in place; sandbox scaffolding exists (`plugin/`), not yet wired to the live query path | `wasmtime`, fuel + memory limits |

## Manifest (`manifest.toml`)

```toml
id = "ddg"
name = "DuckDuckGo Search"
version = "1.0.0"
kind = "script"
entrypoint = "run.sh"
keywords = ["ddg", "search"]   # empty = consulted for every query

[[permissions]]
permission = "NetworkConnect"
justification = "Open search results in your default browser"
```

`manifest::manifest.rs` parses and validates this file; `id` must be unique
across installed extensions and is the namespace root for the extension's
capability token (`plugin.<id>`).

## Script contract

Invoked as `run.sh "<query>"`. Print a JSON array of results to stdout:

```json
[{ "title": "…", "subtitle": "…", "action": { "type": "open_url", "url": "…" } }]
```

A runnable reference implementation lives in
[`examples/extensions/ddg/`](../examples/extensions/ddg/).

## WASM contract (manifest-level; execution not yet live)

A `.wasm`/`.wat` module is expected to export:

- `memory`
- `alloc(i32) -> i32`
- `query(i32, i32) -> i64` — returns a packed pointer to a JSON result array

See [`examples/extensions/wasm-hello/`](../examples/extensions/wasm-hello/).
Runtime execution goes through `wasmtime` with fuel and memory limits once
`plugin/sandbox.rs` is on the live path (tracked in
[ROADMAP.md](../ROADMAP.md)).

## Capability model for extensions

Enabling an extension grants a **revocable** capability token scoped to
`plugin.<id>`, covering exactly the permissions its manifest requests — each
shown with its stated justification at enable time. Result-actions
(`open_url`, `open_path`, `copy`) are checked against that token by the same
`CapabilityGate` the first-party agent uses (see
[security.md](security.md)). An extension requesting an action outside its
granted permission set is denied before it reaches the OS — no special-casing
for first-party vs. third-party code.

## IPC surface (host-side, backs the manager UI)

Implemented in `src-tauri/src/commands/extensions.rs`, backed by
`extension::registry` in the runtime crate:

- `list_extensions`
- `install_extension`
- `uninstall_extension`
- `set_extension_enabled`
- `query_extensions` — merges extension results into the unified search ranking
- `execute_extension_action`

## Writing your own

1. Create a directory under the extensions path with `manifest.toml` +
   entrypoint.
2. Start from `examples/extensions/ddg/` (script) or
   `examples/extensions/wasm-hello/` (WASM).
3. Request only the permissions your entrypoint actually uses — the gate
   denies anything not listed, so an under-scoped manifest fails loudly
   during development rather than silently in production.
4. `set_extension_enabled` to test; results merge into unified search
   immediately if `keywords` matches, or on every query if `keywords` is
   empty.
