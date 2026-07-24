# PHASE 5 ADDENDUM — RENDERER REFINEMENTS

*Integrating architectural feedback from the Phase 5 review to harden the custom React reconciler design.*

### 1. Stable Node Identity
NodeIDs must be deterministic to ensure efficient diffing and patching. 
*   **Generation:** `NodeID` is derived from React's internal Fiber tree (`fiber.key` if provided by the developer, otherwise a composite of `fiber.index` and parent hierarchy). 
*   **Reordering:** If developers provide standard React `key` props, the NodeID survives reordering, preventing unnecessary unmount/remount cycles during sorting or filtering.

### 2. Callback Lifecycle & Garbage Collection
To prevent memory leaks in the `CallbackRegistry` over long sessions:
*   **Invalidation:** When `removeChild` or `commitUpdate` removes or replaces an event prop, the old `CallbackID` is immediately deleted from the registry.
*   **Crash Cleanup:** If an isolate crashes, the Host unilaterally drops all active event listeners associated with that isolate ID.
*   **Suspension:** When an extension transitions to `Suspended`, callbacks are preserved but marked *inactive*. Events fired by the host during suspension (e.g., stale async network responses) are logged and dropped to prevent waking the isolate unnecessarily.

### 3. Concurrent React Support
*   **Enabled:** Yes. The reconciler fully supports React 18 Concurrent Mode (`startTransition`, `useDeferredValue`).
*   **Interruptible Renders:** The serialization pipeline *only* triggers when `resetAfterCommit` is called. If React yields or aborts a render pass due to higher-priority interactions, no incomplete UI trees are serialized over IPC.

### 4. Renderer Memory Model (Ownership)
Strict boundaries prevent mutation leaks:
`React Fiber (Owner)` → mutates → `UINodes (Owned)` → registers → `Callback Registry (Owned)`.
*   When `resetAfterCommit` fires, the `UINodes` tree is fed to the MessagePack encoder. The resulting byte buffer is ephemeral. The Rust Host receives a strictly **immutable snapshot**, ensuring the Guest retains sole ownership of the mutable UI state.

### 5. Serialization Evolution Heuristic
Instead of hardcoded benchmarks, the platform implements runtime telemetry to govern serialization strategy:
*   **Threshold 1:** Full tree serialization is used by default.
*   **Threshold 2 (Dynamic):** If `NodeCount > 1000` OR the previous frame's serialization phase exceeded `4.0ms`, the host config switches to computing and transmitting JSON-Patches (diffs) for subsequent renders.
