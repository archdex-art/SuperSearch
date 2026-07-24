# PHASE 7 ADDENDUM — SDK REFINEMENTS

*Integrating architectural feedback from the Phase 7 review to harden the developer experience and public API contract.*

### 1. API Stability Levels
To allow the platform to evolve without breaking extensions unexpectedly, all exported SDK APIs are annotated with strict stability tags:
*   `@stable`: Backward compatible across major versions.
*   `@experimental`: May change without notice (triggers a compiler warning).
*   `@preview`: Requires explicit opt-in via `package.json` feature flags.
*   `@internal`: Stripped from public type definitions; throws if called directly.

### 2. Error Taxonomy
A standardized error hierarchy ensures consistent error boundary handling:
```typescript
class SuperSearchError extends Error {}
class CapabilityError extends SuperSearchError {} // e.g., "Missing 'clipboard-read' capability"
class NetworkError extends SuperSearchError {}
class StorageError extends SuperSearchError {}
class ExtensionSuspendedError extends SuperSearchError {} // Fired if async tasks resolve while suspended
class VersionMismatchError extends SuperSearchError {}
class IPCError extends SuperSearchError {} // Serialization or host-boundary failures
```

### 3. Canonical Hooks
To promote consistent state management, the SDK provides first-party hooks:
*   `useExtensionContext()`: Access lifecycle states (`isActive`, `isSuspended`).
*   `usePreference<T>(key)`: Reactive hook to user settings.
*   `useClipboard()`: Subscribe to OS clipboard changes (capability required).
*   `useEnvironment()`: Reactive access to OS Theme (Dark/Light).
*   `useAIContext()`: Access to the current Agent session and requested tool schemas.

### 4. SDK Testing Story
A dedicated `@supersearch/api/testing` package is provided. It runs in a standard Node.js/Jest/Vitest environment (bypassing the Rust host entirely).
*   **Mock Runtime:** Simulates lifecycle events (`suspend()`, `resume()`).
*   **Fake Storage:** In-memory SQLite replacements for testing cache logic.
*   **Mock Capabilities:** Assert that an extension gracefully degrades when a capability (e.g., `Network`) is denied by the test runner.
*   **Reconciler Snapshot Tester:** Asserts that the React UI evaluates to the expected MessagePack abstract tree.

### 5. Enhanced AI Tool Metadata
Manifests supporting MCP (Model Context Protocol) must provide rich metadata to assist the `AgentController` in tool selection and safety enforcement:
```json
{
  "name": "create-issue",
  "title": "Create Linear Issue",
  "mode": "no-view",
  "ai": {
    "description": "Creates a new ticket in the Linear backlog. Use when the user requests tracking a bug.",
    "idempotent": false, 
    "cost": "medium"
  }
}
```
*   `idempotent: false` informs the Agent that it MUST ask the user for confirmation before autonomously executing the tool.
