# Architecture

SuperSearch is a Tauri desktop app (native shell + OS integration) fronting a
standalone Rust runtime kernel that owns intent classification, scheduling,
and capability-gated execution. The kernel is a separate crate on purpose: it
has no Tauri dependency and can be embedded, tested, or replayed headless.

```
 keystroke ("/chatgpt summarize this file")
        │
        ▼
┌───────────────────┐   fuzzy match over apps / files / commands
│ react-command-     │──────────────────────────────────────────┐
│ palette/ (frontend)│                                          │
└─────────┬──────────┘                                          │
          │ Tauri IPC                                           ▼
┌─────────▼──────────┐                                 ┌─────────────────┐
│  src-tauri (host)   │                                 │ system_search.rs │
│  commands/*.rs      │                                 │ search.rs        │
└─────────┬──────────┘                                 └─────────────────┘
          │ calls into
┌─────────▼─────────────────────────────────────────────────────────────┐
│                supersearch-runtime (crates/supersearch-runtime)        │
│                                                                         │
│  agent/patterns.rs   → classify text into an AgentIntent (no LLM)      │
│  agent/planner.rs    → compile Intent → TaskGraph (DAG)                │
│  agent/executor.rs   → walk the graph, dispatch each node              │
│  capability/gate.rs  → authorize each node against the agent's token   │
│  kernel/*, platform/*→ spawn the OS action (argv, never sh -c)         │
│  journal/*           → append-only record of every decision + result   │
└─────────────────────────────────────────────────────────────────────────┘
```

## Crate / directory map

| Path | Responsibility |
|---|---|
| `react-command-palette/` | The frontend: React + TypeScript + Tailwind + Framer Motion palette (Spotlight/Raycast-grade motion). Built with Vite; `frontendDist` in `tauri.conf.json` points at its `dist/` output — this is the only frontend the app builds/ships. |
| `src-tauri/` | Tauri app host: window/hotkey management, IPC command handlers, updater |
| `src-tauri/src/commands/` | One module per IPC surface: `search`, `system_search`, `actions`, `agent`, `extensions`, `journal`, `settings`, `telemetry`, `updater`, `window` |
| `crates/supersearch-runtime/` | The AI kernel — Tauri-independent, unit-testable in isolation |
| `crates/supersearch-runtime/src/agent/` | `patterns.rs` (intent classifier), `planner.rs` (Intent → `TaskGraph`), `executor.rs`, `memory.rs`, `context.rs`, `controller.rs` |
| `crates/supersearch-runtime/src/capability/` | `token.rs`, `namespace.rs`, `registry.rs`, `gate.rs` — the object-capability mediation layer |
| `crates/supersearch-runtime/src/journal/` | `writer.rs`, `reader.rs`, `entry.rs`, `replay.rs` — append-only, replayable execution log |
| `crates/supersearch-runtime/src/kernel/` | `runtime.rs`, `process.rs` — privileged OS automation primitives |
| `crates/supersearch-runtime/src/platform/` | `macos.rs`, `linux.rs`, `windows.rs`, `unsupported.rs` behind one `PlatformBackend` trait (`exec.rs`) |
| `crates/supersearch-runtime/src/extension/` | `manifest.rs`, `host.rs` (script extensions), `wasm.rs`, `registry.rs` |
| `crates/supersearch-runtime/src/scheduler/` | Multi-queue cooperative scheduler + supervision (`priority.rs`, `queue.rs`, `supervisor.rs`, `task.rs`, `yielding.rs`) |
| `crates/supersearch-runtime/src/reactive/` | Dependency graph with topological evaluation (`graph.rs`, `node.rs`, `reconcile.rs`, `signal.rs`) |
| `crates/supersearch-runtime/src/plugin/` | Sandboxed WASM adapter scaffolding (`sandbox.rs`, `host.rs`, `ipc.rs`, `manifest.rs`) |
| `examples/extensions/` | Runnable reference extensions (`ddg/` script, `wasm-hello/` WASM) |

## Architecture invariants

These are stated in [`lib.rs`](../crates/supersearch-runtime/src/lib.rs) and
hold across every module:

1. **Deterministic execution** — every action is journaled with the literal
   token stream and tool payload that produced it, so a run can be replayed
   without live inference.
2. **Capability injection, not ambient authority** — nothing discovers a
   capability; it is granted, scoped to a namespace, and revocable at any
   time via an atomic flag flip.
3. **Decoupled governance** — the scheduler only owns time-slicing. Token
   budgets, inference ceilings, and quota monitoring are external middleware
   concerns and are never baked into the scheduling loop.

## Request lifecycle

1. **Input** — the palette UI sends the raw query over Tauri IPC
   (`commands/search.rs`, `commands/agent.rs`).
2. **Classification** — `agent::patterns` maps text to a closed
   `AgentIntent` enum (`LaunchApp`, `OpenFile`, `OpenUrl`, `WebSearch`,
   `FindFiles`, `ClipboardRead/Write`, `SystemCommand`, `MultiStep`, …) using
   keyword templates and entity extraction — no LLM, fully local, zero
   network latency.
3. **Planning** — `agent::planner` compiles the `AgentIntent` into a
   `TaskGraph`: a DAG of nodes with explicit dependencies, so multi-step
   intents (`MultiStep`) execute in the right order / parallel where safe.
4. **Authorization** — before `agent::executor` dispatches a node, it asks
   `capability::gate::CapabilityGate` whether the agent's token authorizes
   the `(Namespace, Permission)` pair the node requires. A denial short-circuits
   before any process is spawned.
5. **Execution** — authorized nodes reach the OS exclusively through argument
   vectors (`kernel::process`, `platform::{macos,linux,windows}`) — `open`,
   `mdfind`, `osascript`, `pbcopy`, etc. User-derived data is never
   interpolated into a shell string.
6. **Journaling** — every `CapabilityCheck` and `OsAutomationResult` is
   appended to the journal (`journal::writer`), giving a replayable audit
   trail independent of the UI.

## Extending the surface

New capability: add a `Permission` variant in `capability/token.rs`, wire it
through `capability::gate`, and require it from the executor node that needs
it — never bypass the gate. New intent: add an `AgentIntent` variant in
`agent/patterns.rs`, extend `agent/planner.rs` to compile it into
`TaskNode`s, and add an executor branch. See [extensions.md](extensions.md)
for the user-installable extension path, which doesn't require touching the
kernel at all.
