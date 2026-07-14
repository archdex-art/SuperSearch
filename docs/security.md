# Security Model

SuperSearch can autonomously drive your OS — launch apps, type into other
windows, run shell commands. This document states what is **actually
enforced today** on the live execution path, and calls out what is scaffolded
but not yet load-bearing. Treat anything not in the first list as
non-authoritative.

## Enforced today

- **Capability-mediated execution.** Before any OS action runs, the executor
  maps it to a required `(Namespace, Permission)` pair and asks the
  `CapabilityGate` (`capability/gate.rs`) whether the agent's token
  authorizes it. A denied action never reaches the OS — no process is
  spawned. The agent holds a single, revocable token granted at boot in the
  `agent` namespace (`kernel/runtime.rs`); revoking it, or narrowing its
  permission set, immediately disables the corresponding actions. Covered by
  `action_without_capability_is_blocked_before_touching_the_os`.
- **Auditable.** Every gate decision (`CapabilityCheck`) and OS result
  (`OsAutomationResult`) is appended to the append-only journal
  (`journal/writer.rs`), giving a replayable audit trail of what the agent
  was asked to do and what it was allowed to do.
- **No shell string interpolation of user input.** Every action carrying
  user-derived data (app names, file paths, URLs, search queries, clipboard
  content, terminal commands) is executed by spawning the target binary
  directly with an argument vector — `open`, `mdfind`, `pbcopy`, `osascript`
  — never by building a string for `sh -c`. Shell metacharacters (`;`, `|`,
  `$()`, backticks, quotes) are inert. Dynamic values passed to AppleScript
  are bound as `on run argv` items, not interpolated into script source. See
  `agent/executor.rs` and `src-tauri/src/commands/actions.rs`. Covered by
  `clipboard_write_roundtrips_untrusted_content`.
- **Bounded execution.** Every OS action is killed if it exceeds a hard
  timeout (`ACTION_TIMEOUT`, 15s), so a hung helper process can't wedge the
  app. IPC entry points reject empty and oversized input (queries are capped
  at 2048 bytes at the `agent_query` boundary).
- **Fixed intent taxonomy.** The agent maps natural language to a closed set
  of `TaskNodeKind` variants; it never synthesizes arbitrary scripts from
  user text. The only remaining `sh -c` calls are *constant* scripts authored
  in `planner.rs` (e.g. `pmset sleepnow`) that never contain user input.
- **Local-first classification.** Intent classification is fully local
  (rule-based, no LLM); app launches and file lookups never leave your
  machine.
- **Extension capability scoping.** Enabling an extension grants a revocable
  token scoped to `plugin.<id>` covering exactly the permissions its
  manifest requests, each with a stated justification. Extension
  result-actions go through the same `CapabilityGate` as first-party agent
  actions — see [extensions.md](extensions.md).

## Scaffolded, not yet load-bearing

The capability system and journal are on the live agent execution path as
described above. These are **not**:

- **`scheduler/`** — cooperative multi-queue scheduler + supervision. Boots,
  not yet driving first-party agent actions.
- **`reactive/`** — dependency graph with topological evaluation. Boots, not
  yet wired into the query pipeline.
- **`plugin/`** (WASM sandbox) — `wasmtime`-based sandboxing scaffolding
  exists; WASM extensions are declared in manifests but not yet executed on
  the live path (script extensions are).

These exist to host future third-party plugins, which will receive their own
narrowly-scoped capability tokens through the same gate described above —
tracked in [ROADMAP.md](../ROADMAP.md). **Grant Accessibility permission
only if you trust the build you are running** — that permission is what
makes keystroke injection possible in the first place, independent of the
capability system's internal checks.

## Distribution integrity

- Builds are **not code-signed or notarized yet** — macOS Gatekeeper and
  Windows SmartScreen will warn on first launch until credentials are
  configured (see [RELEASING.md](../RELEASING.md)). Verify you downloaded
  from the official [Releases page](https://github.com/archdex-art/SuperSearch/releases)
  before overriding either warning.
- Auto-update is implemented behind the `updater` Cargo feature but off by
  default (it refuses to start without a configured `plugins.updater.pubkey`).

## Reporting a vulnerability

Open a [private security advisory](https://github.com/archdex-art/SuperSearch/security/advisories/new)
rather than a public issue for anything that bypasses the capability gate,
achieves shell injection, or escalates beyond an extension's granted
permissions.
