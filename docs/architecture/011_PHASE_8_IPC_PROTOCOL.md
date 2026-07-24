# PHASE 8 — IPC PROTOCOL

**Prepared By:** Principal Engineering Team
**Objective:** Define the binary communication protocol bridging the V8 Guest Isolate (`deno_core`) and the Rust Host. This protocol governs data serialization, versioning, backpressure, and asynchronous cancellation.

---

### 1. Transport Architecture

Because the V8 Isolate is embedded directly within the Rust host process, IPC does NOT utilize network sockets or OS pipes. 

*   **Boundary:** Communication occurs over **V8 Fast APIs** (Foreign Function Interfaces).
*   **Memory Transfer:** JavaScript `Uint8Array` buffers are passed directly to Rust as raw memory pointers, avoiding deep copy overhead whenever possible.
*   **Serialization Format:** **MessagePack**. It provides the dynamic flexibility of JSON required for React props, but with a highly compressed binary footprint.

---

### 2. Binary Message Envelopes (The Protocol)

To minimize parsing overhead, the protocol uses **MessagePack Arrays** instead of objects for its root envelope. This positional schema saves significant byte weight.

#### 2.1 The Request (Guest → Host)
Used for calling host APIs (e.g., `fs.read`, `fetch`).
**Schema:** `[0, RequestID, MethodString, PayloadObject]`
**Example:** `[0, 1024, "clipboard.write", { "text": "hello" }]`

#### 2.2 The Response (Host → Guest)
Used to resolve/reject a previous Request.
**Schema:** `[1, RequestID, StatusCode, PayloadOrError]`
**Example (Success):** `[1, 1024, 0, null]`
**Example (Error):** `[1, 1024, 1, { "code": "CapabilityError", "msg": "Denied" }]`

#### 2.3 The Event (Host → Guest)
One-way pub/sub events pushed to the extension (e.g., Theme changes, Focus changes).
**Schema:** `[2, EventName, PayloadObject]`
**Example:** `[2, "theme.changed", { "theme": "dark" }]`

#### 2.4 The UI Sync (Guest → Host)
A specialized, highly optimized envelope strictly for React Reconciler updates.
**Schema:** `[3, UpdateType, UINodeTree]`
*(UpdateType: 0 = Full Tree, 1 = JSON-Patch Diff)*

---

### 3. Protocol Versioning & Negotiation

Compatibility is established during the Sandbox Allocation phase before any JS code executes.

1.  **Host Injection:** Rust injects a global constant into V8: `globalThis.__SUPERSEARCH_IPC_VERSION = 1`.
2.  **SDK Handshake:** The SDK (`@supersearch/api`) boots, reads the version, and verifies compatibility.
3.  **Mismatch Handling:** If the SDK requires IPC Version 2, it throws an immediate synchronous `VersionMismatchError`. Rust catches the panic and transitions the extension state to `Disabled(Incompatible)`.

---

### 4. Backpressure & Batching Strategy

If an extension rapidly updates state (e.g., a tight loop), the Guest could flood the Rust Host, overwhelming the Tauri UI thread.

*   **Bounded Channels:** The Rust host receives UI Sync messages into a bounded `tokio::sync::mpsc` channel (e.g., capacity = 16).
*   **Guest-Side Throttling:** The SDK Reconciler implements a `requestAnimationFrame` equivalent. After sending a `UI Sync` envelope, the Reconciler will **block further serialization** until the Host replies with a `FrameAcknowledged` event.
*   **Frame Dropping:** If React commits state updates while the SDK is waiting for an acknowledgment, those intermediate commits are mutated in the V8 memory tree, but only the final state is serialized when the Host is ready.

---

### 5. Cancellation Semantics

Network requests, AI prompts, and heavy FS operations must be cancellable.

*   **SDK Boundary:** Developers use standard `AbortController.signal`.
*   **IPC Bridge:** When `.abort()` is called, the SDK emits a special `CancelRequest` envelope: `[4, OriginalRequestID]`.
*   **Host Resolution:** The Rust Host receives the cancellation, aborts the underlying Tokio task (`JoinHandle::abort()`), and sends a standard Response envelope back: `[1, OriginalRequestID, 1, { "code": "AbortError" }]`.

---

### 6. Compression Thresholds

While MessagePack is dense, serializing a `<List>` with 10,000 items creates a massive byte array.
*   **Heuristic:** Before passing the `Uint8Array` to Rust, the Guest checks the byte length.
*   **LZ4 Compression:** If `length > 64KB`, the Guest runs an ultra-fast WebAssembly LZ4 compressor.
*   **Flagging:** The UI Sync envelope `UpdateType` is bitwise shifted to indicate compression, instructing Rust to LZ4-decompress the payload before passing it to the Tauri Frontend.

---

### 7. Error Propagation Mapping

All errors encountered in Rust (e.g., `std::io::Error` from filesystem access) are intercepted at the IPC boundary.
*   Rust maps the error to a standard string identifier (`NetworkError`, `StorageError`).
*   The Response envelope transmits the identifier.
*   The Guest SDK reconstructs the native JavaScript Error class (e.g., `throw new NetworkError(msg)`) to ensure `instanceof` checks work flawlessly in the developer's try/catch blocks.
