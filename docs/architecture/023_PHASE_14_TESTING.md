# PHASE 14 — TESTING ARCHITECTURE

**Prepared By:** Principal Engineering Team
**Objective:** Define the comprehensive automated validation strategy. This strategy enforces the architectural guarantees (Security, Performance, Compatibility) established in prior phases and ensures the platform remains highly stable as the codebase scales.

---

### 1. The Testing Pyramid

The platform enforces a strict testing pyramid, isolating concerns between the Rust Host, the TypeScript SDK, and the V8 Interop layer.

#### 1.1 Unit Testing (Fast, Isolated)
*   **Rust Backend:** Uses `cargo test`. Focuses on pure logic: manifest parsing (`serde`), Capability Resolver logic, LRU cache eviction math, and internal Agent DAG routing.
*   **TypeScript SDK:** Uses `vitest`. Focuses on React hook logic, input validation, and data formatting before it hits the IPC bridge.

#### 1.2 Integration Testing (FFI & Interop)
Tests the critical boundaries where systems meet.
*   **IPC Bridge Tests:** Spawns a real `deno_core` isolate in a test harness, registers mock ops, and executes a JS bundle. Asserts that passing a JSON-Patch UI diff correctly parses back into Rust structs without memory leaks.
*   **Database Tests:** Verifies the `rusqlite` layer correctly memory-maps and queries concurrent reads from multiple simulated extension threads.

#### 1.3 End-to-End (E2E) Testing (High Fidelity)
*   **Tauri E2E:** Uses `webdriverio` or Playwright combined with Tauri's automated test driver.
*   **Workflow:** Compiles a suite of mock extensions, installs them into a headless SuperSearch app, simulates global keystrokes (`Cmd+Space`), and asserts that the React Native UI correctly hydrates and displays the mocked results.

---

### 2. Security & Fuzz Testing (Red Teaming)

Given the untrusted nature of third-party extensions, standard testing is insufficient. We must simulate hostile environments.

*   **IPC Fuzzing (`cargo-fuzz`):** The MessagePack deserializer is a prime target for buffer overflow attacks. Continuous fuzzing injects randomized, malformed binary arrays into the Rust Host's `op_ui_update` endpoint to guarantee panic-free handling.
*   **Sandbox Escape Suite:** A specialized test suite that loads a hostile JavaScript bundle. The bundle explicitly attempts to:
    *   Read `/etc/passwd` via prototype pollution.
    *   Infinite loop (`while(true)`) to test the Watchdog CPU limit.
    *   Allocate infinite arrays to test the OOM memory quota.
    *   *Assertion:* The host must successfully trap and terminate the isolate in 100% of these scenarios without crashing the main thread.

---

### 3. Performance Regression Testing (CI Gates)

Performance SLAs are contractual. Regressions break the build.
*   **Methodology:** Uses `criterion.rs` for Rust and `mitata` for V8.
*   **CI Enforcement:** When a PR is opened, the CI runner executes the benchmark suite. It compares the results against the `main` branch baseline.
*   **Failure Thresholds:** If Cold Start exceeds 50ms, or IPC serialization slows by > 5%, the GitHub PR is automatically marked red, requiring a manual override or code optimization.

---

### 4. Version Compatibility Matrix

The `@supersearch/api` SDK and the SuperSearch Host update independently.
*   **The Matrix:** The CI pipeline runs the Integration Test suite across a matrix of versions (e.g., Host v1.2 testing extensions built with SDK v1.0, v1.1, and v1.2).
*   **Assertion:** Guarantees that internal IPC envelope extensions (like adding new Protocol Flags) do not break legacy extensions currently running in production.

---

### 5. Marketplace CI Validation

Testing does not end at the platform layer; it extends to the extensions published by the community.
*   **The Sandbox Test:** When a developer submits a PR to the Marketplace Monorepo, the CI pipeline boots an ephemeral SuperSearch host.
*   **Dry Run:** It installs the submitted extension, runs `npm run test`, checks for prohibited dependencies, and verifies that the UI component tree successfully evaluates to MessagePack without throwing `React-Reconciler` fatal errors.

---

### 6. Observability & Chaos Testing

*   **Chaos Engineering:** Periodically injecting simulated failures (e.g., dropping SQLite database locks, simulating 10,000ms latency on the LLM API, returning corrupted HTTP headers) to ensure the platform degrades gracefully rather than hard-crashing.
*   **Test Telemetry:** Test suites automatically assert that the correct telemetry events (e.g., `CapabilityDenied`, `IsolateTerminated`) are accurately written to the local audit log when failures occur.
