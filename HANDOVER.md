# SuperSearch Extension Platform: State Handover Log

**Target Audience:** Future developers, AI agents, or system maintainers.
**Last Updated:** July 2026

This document serves as the absolute source of truth for the current state of the SuperSearch Extension Platform. It details the frozen architecture, the exact implementation progress, accumulated technical debt, and the immediate next steps required to reach Beta.

---

## 1. Project Vision & Architecture (Frozen v1.0)
SuperSearch is an AI-native desktop launcher and productivity operating layer built on **Tauri (Rust)** and **React**. The Extension Platform allows third-party developers to write plugins in React/TypeScript that execute securely in isolated environments and natively integrate with the SuperSearch AI Agent.

**Core Architectural Pillars:**
*   **Execution:** Isolated V8 Sandboxes via `deno_core`. Zero ambient Web/Node APIs are provided by default.
*   **Security:** Default-deny capability gates. All OS operations (FS, Network) route through Rust. Bundles MUST be Ed25519 signed by the Marketplace.
*   **UI Rendering:** A Custom React Reconciler (`@supersearch/reconciler`) running in V8 translates JSX into an abstract UI Node Tree.
*   **IPC:** Zero-copy MessagePack binary arrays over bounded MPSC channels bridge the V8 Guest and Rust Host in `< 1ms`.
*   **AI Integration:** Extensions are compiled directly into Model Context Protocol (MCP) tool schemas. The AI Agent seamlessly invokes extensions as native tools.

*(For full details, see the 15-phase specification in `docs/architecture/`)*.

---

## 2. Current Implementation State

The implementation is structured around strict, executable "Gates".

### ✅ Gate A: Build System Stabilization (PASSED)
*   NPM workspaces are correctly configured for `@supersearch/api`, `@supersearch/reconciler`, and `@supersearch/cli`.
*   Cross-package boundary imports are strictly enforced through `dist/` outputs.
*   All packages compile cleanly using `ES2024` target libraries.

### ✅ Gate B: Single Extension Runtime Proof (PASSED)
*   Verified that a single compiled JavaScript bundle successfully loads into `SandboxAllocator`.
*   The `react-reconciler` correctly fires `postUiSync`.
*   The Rust host securely receives, decodes, and validates the `IpcEnvelope::UiSync` payload.
*   Malformed IPC payloads are safely caught and rejected without panicking the Rust host.

### ✅ Gate C: Application Integration / Discovery (PASSED)
*   `discovery.rs` accurately scans `~/.supersearch/extensions/` for valid manifests.
*   Gracefully skips malformed or duplicate manifests without crashing.
*   `launch_extension()` effectively creates an isolate and pulls the initial rendered UI tree via IPC.
*   Wired `discovery.rs` backend into the Tauri React Frontend (`App.tsx` and `bridge.ts`).
*   `Hydrator.tsx` accepts the `UiSync` payload dynamically from the launched extension and natively renders the UI.
*   Vertical slice complete: Install → Discover → Search → Launch → Render → Return.

### ✅ Gate D: Developer Experience Validation (PASSED)
*   Automated E2E tests for the Golden Path (Hello World).
*   Automated regression tests for Broken Extensions (malformed manifest safety).
*   Automated regression tests for Runtime Failure (JS bundle throwing errors safely caught).
*   Developer documentation (`extensions.md`) updated to reflect the new React/V8 architecture, making the onboarding process clear and aligned with the current implementation.
---

## 3. Directory Map (Where things live)
*   `crates/supersearch-runtime/`: The core Rust engine.
    *   `src/extension/runtime/`: The `deno_core` V8 isolate wrappers, allocator, and fast ops (`op2`).
    *   `src/extension/ipc/`: MessagePack envelope definitions and error mappings.
    *   `src/extension/scheduler/`: Fair multiplexing MPSC queue.
    *   `src/extension/discovery.rs`: Manifest scanning and launch orchestration.
    *   `src/agent/mcp.rs`: Translation of extension commands to LLM tool schemas.
*   `packages/reconciler/`: The Guest-side React Reconciler and Event Loop.
*   `packages/api/`: The public developer SDK (`List`, `ActionPanel`, `Clipboard`).
*   `packages/cli/`: The developer CLI (`supersearch dev` / `supersearch build`).
*   `docs/architecture/`: The definitive 15-phase architectural specification and ADRs.

---

## 4. Technical Debt & Known Issues
Before Gate D or a Private Beta release, the following technical debt must be addressed:

1.  **Timer Stopgap Polyfill (`isolate.rs`):** Currently, `setTimeout` and `clearTimeout` are polyfilled as immediate microtasks to allow `react-reconciler` to initialize without crashing. *Fix required:* Implement a real `tokio::time`-backed op. No Beta release may ship with this placeholder behavior.
2.  **Esbuild Bundle Size:** The `hello-world` example bundle is `~849KB` because the entire `react-reconciler` is bundled. *Optimization required:* Evaluate externalizing the reconciler or relying heavily on V8 snapshots during isolate boot.
3.  **Flaky Test:** The legacy `large_stdout_does_not_stall_until_timeout` test in `extension/host.rs` is ignored.

---

## 5. Next Immediate Steps (To Be Made)

When resuming work, the AI or developer should focus on:

### Step 1: Beta Prep & Polish
*   Implement the **Persist**, **Update**, **Disable**, and **Uninstall** handlers in the UI.
*   Verify that unloading an extension successfully cleans up the SQLite database and gracefully compacts the V8 heap.
*   Fix the Timer Stopgap Polyfill (`isolate.rs`) and externalize the React reconciler to optimize bundle sizes.

### Step 2: Operational Validation
*   Invite an external developer to install the CLI and build an extension using only the public docs to measure "Time to First Render".

### Step 3: Beta Execution & Monitoring
*   Track platform performance against the targets defined in [`docs/beta_exit_metrics.md`](docs/beta_exit_metrics.md).
*   Achieve rolling 14-day compliance with the exit metrics before declaring General Availability (GA).
