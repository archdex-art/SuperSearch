# Extensions

An extension is a directory with a `manifest.toml` and an entrypoint,
installed under `~/.local/share/com.supersearch.app/extensions/` (or equivalent per OS).

Three execution models share one registry:

| Model | Status | Sandbox |
|---|---|---|
| **JavaScript (React UI)** | Available now | V8 Isolate (`deno_core`) executing React Native-style Reconciler |
| **Script** | Available now | Native subprocess, argv only (no shell), hard 10s timeout |
| **WASM** | Sandbox exists, not live | `wasmtime`, fuel + memory limits |

## JavaScript Manifest (`manifest.toml`)

```toml
id = "hello-world"
name = "Hello World"
version = "1.0.0"
kind = "js"
entrypoint = "dist/bundle.js"
keywords = ["hello"]

[[permissions]]
permission = "NetworkConnect"
justification = "Fetch remote user data"
```

`manifest::manifest.rs` parses and validates this file; `id` must be unique
across installed extensions and is the namespace root for the extension's
capability token (`plugin.<id>`).

## JavaScript (React UI) Contract

JS extensions run inside a highly secure **V8 Isolate**. They use a custom React Reconciler, so you write standard React components with the `@supersearch/api` SDK.
Instead of rendering HTML to a DOM, the Reconciler translates your React tree into native UI instructions (`UiSync` envelopes) sent to the host via IPC over MessagePack.

To build a JS extension:
1. Scaffold your project and install `@supersearch/api`.
2. Write your entrypoint (`src/index.tsx`), exporting your root component.
3. Bundle the app (using `esbuild`, `vite`, etc.) to the `entrypoint` specified in the manifest.

A runnable reference implementation lives in
[`examples/hello-world/`](../examples/hello-world/).

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
2. Start from `examples/hello-world/` (JavaScript), `examples/extensions/ddg/` (script), or
   `examples/extensions/wasm-hello/` (WASM).
3. Request only the permissions your entrypoint actually uses — the gate
   denies anything not listed, so an under-scoped manifest fails loudly
   during development rather than silently in production.
4. `set_extension_enabled` to test; results merge into unified search
   immediately if `keywords` matches, or on every query if `keywords` is
   empty.
