# Roadmap

This tracks where SuperSearch is headed, grounded in the current
implementation status noted in the [README](README.md) — it separates what's
shipped from what's scaffolded, and doesn't promise dates for open-source,
spare-time work. Priorities shift; open an issue if you want to argue for
reordering something.

## Shipped (v0.1.x)

- Cross-platform OS automation behind a single Platform Abstraction Layer
  (macOS / Linux / Windows backends).
- Deterministic intent classification (`agent/patterns.rs`) → `TaskGraph`
  planning → capability-gated execution, fully local, no LLM in the loop.
- Capability-mediated execution enforced on the live agent path, with an
  append-only, replayable journal of every gate decision and OS result.
- Argv-only OS execution — no shell string interpolation of user input.
- Script extensions: manifest, capability-scoped install/enable, merged into
  unified search ranking (`query_extensions`).
- Unified React palette merging system search + extension results into one
  ranked list.
- Unsigned installers for macOS (universal), Linux (`.deb`), and Windows
  (NSIS + `.msi`) published via the tag-triggered release workflow.

## Now (V1.0 Architecture Implementation)

*Note: The platform has recently undergone a massive architectural shift (V1.0) moving away from Wasmtime/WASM towards V8 Isolates to better support the React/TypeScript ecosystem, and evolving the offline-only Agent to an MCP-native LLM orchestrator.*

- **V8 Extension Runtime** — Replaced `wasmtime` with `deno_core`. Extensions are now written in React/TypeScript, sandboxed with strict 50MB memory constraints, and communicate via a Zero-Copy MessagePack IPC bridge. (WASM exploration is archived).
- **Custom React Reconciler** — `@supersearch/reconciler` allows developers to write declarative UI components that natively hydrate inside the Tauri window.
- **AI Integration (MCP)** — Shifted from purely offline, fixed intent classification to an LLM-orchestrated `AgentController`. Extension manifests dynamically compile into Model Context Protocol (MCP) schemas, treating the AI as a first-class consumer.
- **Code signing & notarization** — Ed25519 signature enforcement is now integrated into the `SandboxAllocator`. macOS Gatekeeper / Windows SmartScreen warnings are pending final developer certificates.
- **Scheduler on the live path** — `scheduler/` now utilizes `tokio_stream::StreamMap` to fairly multiplex V8 IPC messages without priority inversion or starvation.

## Next

- **Application Integration (Frontend Wiring)** — The V8 isolates and Rust discovery services are functional, but `react-command-palette/App.tsx` must be wired to invoke them seamlessly from the global search bar.
- **Extension manager UI** — A first-class UI for browsing, installing, and toggling extensions (rather than hand-editing `~/.supersearch/extensions/`).
- **Auto-update, enabled by default** — the `updater` Cargo feature and `check_for_updates` IPC command exist behind a feature flag; turning this on by default needs a release-channel decision.

## Later / exploratory

- Third-party plugin marketplace via Edge CDN distribution — each plugin gets its own narrowly-scoped capability token through the existing `CapabilityGate`, enforced by Ed25519 signatures.
- Cross-device sync of extensions/settings (explicitly opt-in; the security
  model's "local-first" invariant for query data doesn't change).
- Team/shared-config profiles for organizations standardizing on
  SuperSearch.

## Explicitly not planned

- Purely unconstrained Cloud execution without local verification — while we now support LLM inference via MCP, all OS capability boundaries and user confirmations (idempotency checks) remain strictly enforced locally by the Rust Host.

---

Have an idea that isn't here? Open a
[discussion](https://github.com/archdex-art/SuperSearch/discussions) or an
[issue](https://github.com/archdex-art/SuperSearch/issues) — see
[CONTRIBUTING.md](CONTRIBUTING.md).
