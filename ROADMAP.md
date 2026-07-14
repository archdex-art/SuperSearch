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

## Now

- **Code signing & notarization** — macOS Gatekeeper / Windows SmartScreen
  currently warn on every fresh install. Activates purely from repo secrets
  once an Apple Developer account + Windows signing cert are available; see
  [RELEASING.md](RELEASING.md).
- **Scheduler on the live path** — `scheduler/` (multi-queue cooperative
  scheduling + supervision) currently boots but doesn't drive first-party
  agent actions yet. Wiring it in unlocks true parallel multi-step execution
  with priority and preemption instead of sequential dispatch.
- **Linux / Windows parity pass** — the PAL backends exist and build, but
  get less real-world usage than macOS; tracking rough edges as they surface
  (see `requirements/linux.md`, `requirements/windows.md`).

## Next

- **WASM extensions on the live path** — `plugin/` (wasmtime sandbox, fuel +
  memory limits) and the WASM manifest contract are scaffolded
  (`examples/extensions/wasm-hello/`), but `query`/`alloc` execution isn't
  wired into the query pipeline yet. Script extensions stay the supported
  path until this lands.
- **Extension manager UI** — the IPC surface (`list_extensions`,
  `install_extension`, `set_extension_enabled`, …) exists; a first-class UI
  for browsing/installing/toggling extensions (rather than hand-editing the
  extensions directory) does not yet.
- **Reactive context graph on the live path** — `reactive/` (topological
  dependency graph) boots but isn't yet the backing store for
  `agent/context.rs`'s short-term memory; today's context tracking is
  simpler than the graph it will eventually run on.
- **Auto-update, enabled by default** — the `updater` Cargo feature and
  `check_for_updates` IPC command exist behind a feature flag requiring
  `plugins.updater.pubkey`; turning this on by default needs a signing key
  and a release-channel decision.

## Later / exploratory

- Third-party plugin marketplace, once the WASM sandbox is load-bearing and
  has a track record — each plugin gets its own narrowly-scoped capability
  token through the existing `CapabilityGate`, no new trust model.
- Cross-device sync of extensions/settings (explicitly opt-in; the security
  model's "local-first" invariant for query data doesn't change).
- Team/shared-config profiles for organizations standardizing on
  SuperSearch.

## Explicitly not planned

- Cloud-side query processing or telemetry beyond the existing opt-in
  `telemetry` command — intent classification stays local and offline by
  design (see [docs/security.md](docs/security.md)).
- Arbitrary script synthesis from user text — the fixed `AgentIntent`
  taxonomy is a security invariant, not a v1 limitation to relax later.

---

Have an idea that isn't here? Open a
[discussion](https://github.com/archdex-art/SuperSearch/discussions) or an
[issue](https://github.com/archdex-art/SuperSearch/issues) — see
[CONTRIBUTING.md](CONTRIBUTING.md).
