# PHASE 7: APPLICATION INTEGRATION (PRE-BETA)

*Bridging the gap between the production-grade Extension Platform infrastructure and the SuperSearch end-user experience.*

### 1. Objective
Transform the isolated runtime infrastructure into an end-to-end usable product. This phase focuses entirely on the "Missing Integration Layer" required to expose the platform's capabilities to users through the SuperSearch UI.

### 2. Deliverables
*   **Extension Discovery Service:** Rust background scanner that detects `.sse` bundles in `~/.supersearch/extensions/` and loads their manifests into memory.
*   **Search Index Registration:** Injecting parsed extension commands natively into the global Command Palette fuzzy-finder.
*   **Command Execution Pipeline:** Wiring the "Enter" keypress on an extension search result to the `SandboxAllocator` and `op_ipc_post` invocation.
*   **Hydrator Integration:** Connecting `Hydrator.tsx` to the main React Router, enabling seamless transitions from the search bar to the extension view.
*   **SDK Visual Implementation:** Developing the actual Tailwind CSS designs for the `@supersearch/api` components (`<List>`, `<ActionPanel>`, `<Form>`) so they match the host's design system perfectly.
*   **Extension Lifecycle Management:** The GUI and Rust logic for Installing, Updating, and cleanly Uninstalling (wiping SQLite, clearing caches) extensions.
*   **Loading States & Error Boundaries:** Polished UX for cold-starts, network delays, and isolate crashes.

### 2a. Release Criteria Carried Forward From Gate B
*   **No beta release may ship with placeholder timer semantics.** The `TIMER_STOPGAP_POLYFILL` in `isolate.rs` (`setTimeout`/`clearTimeout` resolved as immediate microtasks) is tracked debt, not a shipped feature. It must either be replaced with a real `tokio::time`-backed op, or the SDK must explicitly document timing APIs as unsupported, before Gate D.

### 2b. Gate C Scope Boundary (Stop After Render)
Gate C is intentionally cut short of the full 10-stage lifecycle below. It covers exactly: **Install (manual, on disk) → Discover → Search → Launch → Render.** Persist, Update, Disable, and Uninstall are explicitly deferred to Gate D and must not be started until a fresh, non-implementer reviewer has used the Gate C slice. A working vertical slice surfaces UX and abstraction problems that architecture review cannot.

### 3. Acceptance Criteria (Executable Test Suite)
The Application Integration phase is only complete when the following end-to-end lifecycle behaves as a verifiable, observable test suite:

| Stage | Validation |
| :--- | :--- |
| **Install** | Package verified, signature validated, manifest parsed securely. |
| **Discover** | Extension appears in registry automatically (without restart or after expected reload). |
| **Search** | Commands are indexed and ranked correctly alongside native system results. |
| **Launch** | Runtime isolate created successfully and permissions granted. |
| **Render** | Native UI displayed with expected sub-16ms latency. |
| **Interact** | Actions, forms, and state updates function correctly with bi-directional IPC. |
| **Persist** | Storage survives restart and rigorously respects namespace isolation. |
| **Update** | New version migrates cleanly without data loss or SQLite corruption. |
| **Disable** | Extension is unloaded, V8 heap is compacted, and resources are released. |
| **Uninstall** | Files, registrations, active IPC channels, and caches are removed cleanly. |

### 4. CI-Enforceable Evidence (Happy Path)
Each lifecycle stage is backed by a specific, automatable evidence source so this table becomes something CI enforces rather than something humans interpret:

| Lifecycle Stage | Evidence |
| :--- | :--- |
| **Install** | Signature verification succeeds, manifest validation passes, package registered. |
| **Discover** | Registry contains extension, search index updated. |
| **Search** | Integration test finds command with expected ranking. |
| **Launch** | Isolate created, runtime initialized, no startup errors. |
| **Render** | UI snapshot or E2E test confirms expected output within latency budget. |
| **Interact** | User actions produce expected state transitions. |
| **Persist** | Restart test confirms state survives and remains isolated. |
| **Update** | Migration test preserves user data and upgrades metadata correctly. |
| **Disable** | Runtime shuts down, resources released, no dangling handles. |
| **Uninstall** | Registry, storage, caches, and search index cleaned up. |

### 5. Failure-Path Validation (Unhappy Paths)
A platform executing third-party code must fail predictably, safely, and recoverably. Each stage requires an explicit negative test:

| Stage | Failure Scenarios to Validate |
| :--- | :--- |
| **Install** | Invalid signature, malformed manifest, unsupported SDK version. |
| **Discover** | Duplicate IDs, corrupted registry, unreadable extension directory. |
| **Search** | Index rebuild interrupted, malformed command metadata. |
| **Launch** | Isolate creation failure, permission denial, initialization timeout. |
| **Render** | Component throws, hydration failure, missing assets. |
| **Interact** | IPC timeout, rejected capability request, invalid state transition. |
| **Persist** | Database corruption, quota exceeded, migration failure. |
| **Update** | Partial download, incompatible schema, rollback required. |
| **Disable** | Background task still running, resource leak. |
| **Uninstall** | Locked files, partial cleanup, stale search entries. |

### 6. Next Milestone: Operational Validation
Application Integration is an implementation phase; the milestone that follows it is not. Once the happy-path and failure-path suites above are green, the platform moves to **Operational Validation** — measured by users, not by its creators:
*   Can external developers onboard using only public documentation?
*   What is the median "time to first extension"?
*   Where do developers encounter friction?
*   What are the most common runtime failures in the wild?
*   Are the telemetry dashboards sufficient to diagnose production issues?
*   Do rollback mechanisms work under real-world conditions?

Only after an external developer can traverse this entire lifecycle without internal knowledge or manual intervention will the platform be declared **Beta-Ready**.
