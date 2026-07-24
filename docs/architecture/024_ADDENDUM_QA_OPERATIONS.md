# PHASE 14 ADDENDUM — QUALITY ASSURANCE OPERATIONS

*Operationalizing the testing strategy into enforceable quality gates, cross-platform matrices, and release certification procedures.*

### 1. Coverage Targets & Quality Gates
While 100% coverage is often a vanity metric, critical subsystems demand strict enforcement.
| Subsystem Layer | Coverage Target |
| :--- | :--- |
| **Rust Unit Tests** | `≥ 90%` of Critical Paths |
| **TypeScript SDK** | `≥ 90%` of Public APIs |
| **IPC Bridge Integration** | `100%` of Message Types |
| **Security / Fuzz Suite** | All Threat-Model Scenarios |
| **End-to-End (E2E)** | Critical User Journeys (Search -> Execute -> View) |

### 2. Property-Based Testing
To complement example-based unit tests, we utilize `proptest` (Rust) and `fast-check` (TS) to generate thousands of randomized inputs for deterministic components:
*   Manifest schema parsing.
*   Capability constraint resolution (`fs.read:/a/b` vs `/a/*`).
*   MessagePack UI-Tree serialization.

### 3. Deterministic Replay & Endurance
Stability is proven over time, not just in sub-second CI runs.
*   **Endurance Suite:** Ephemeral runners maintain a host instance for 24 hours. They simulate repeated isolate creation/destruction, cache growth, and memory leak detection across 10,000 synthetic invocations.
*   **Deterministic Replay:** The Rust IPC logger can record a binary stream of a crash. Developers can replay this exact byte stream against a local isolate to deterministically reproduce the error.

### 4. Cross-Platform Validation Matrix
Tauri targets multiple OS backends. CI runners explicitly validate platform-specific FFI boundaries:
*   **macOS:** AppKit window management, Apple Keychain integration, `osascript` shell limits.
*   **Windows:** WebView2 rendering, Windows Credential Guard, PowerShell escaping.
*   **Linux:** WebKitGTK rendering, Secret Service API, X11/Wayland shortcut bindings.

### 5. AI Validation Suite
The integration between extensions and the `AgentController` (Phase 13) requires specialized testing:
*   **Tool Compilation:** Asserts that an extension manifest accurately compiles into valid OpenAI and Anthropic MCP Tool Schemas.
*   **Context Ordering:** Asserts that foreground Context Providers correctly override background providers.
*   **Idempotency Enforcement:** Asserts that mutative commands properly halt and invoke the User Approval flow in a headless environment.

### 6. Flaky Test Policy
*   If a test flakes (fails intermittently) in CI, it is automatically quarantined and marked `#[ignore]`.
*   The pipeline opens an automated GitHub Issue assigned to the subsystem owner.
*   Quarantined tests do not block releases, but accumulating >5 quarantined tests halts all feature merges until the technical debt is addressed.

### 7. Release Certification Checklist
Before a new version of the Host is tagged as a Release Candidate (RC):
1.  [ ] All automated tests (Unit, Integration, Fuzz, E2E) pass on macOS, Windows, and Linux.
2.  [ ] Performance regressions (Startup, IPC, Render) are < 0%.
3.  [ ] Compatibility Matrix confirms the new Host successfully runs V8 extensions built with the last 3 major SDK versions.
4.  [ ] Security audit passes (no quarantined sandbox tests).
5.  [ ] Release is manually smoke-tested by the QA team on bare-metal hardware.
