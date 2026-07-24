# PHASE 8 ADDENDUM — IPC REFINEMENTS

*Integrating architectural feedback to lock down protocol evolution, streaming, and resilience before implementation.*

### 1. Envelope Extensibility & Protocol Flags
To support future evolution without breaking existing SDKs, the MessagePack array envelope is expanded to reserve slots for Version and Flags.

**New Universal Schema:** `[Version, Flags, Type, ID, MethodOrEvent, Payload]`

**Protocol Flags (Bitmask):**
*   `0x01`: Compressed (LZ4)
*   `0x02`: Encrypted (Future-proofing)
*   `0x04`: Streamed (Indicates payload is a chunk)
*   `0x08`: Partial (More data follows)
*   `0x10`: High Priority

### 2. Request Priorities
The Rust host MPSC channel implements a priority queue to ensure background tasks do not block the UI.
*   **Priority 0 (System):** Cancellation, Isolate Termination.
*   **Priority 1 (UI Sync):** React Reconciler diffs.
*   **Priority 2 (Interactive):** User input, Action dispatches.
*   **Priority 3 (Standard):** Filesystem, Storage, Network fetch.
*   **Priority 4 (Background):** Telemetry, Indexing.

### 3. Streaming Protocol
Essential for AI inference, large file reads, and bulk indexing. The protocol supports bi-directional streaming by multiplexing chunks over the standard envelope.
*   `StreamStart`: Opens the channel.
*   `StreamChunk`: Appends data. Uses the `Streamed` flag.
*   `StreamEnd`: Closes the channel.
*   `StreamAbort`: Cancels the stream.

### 4. Timeout Semantics
*   **Default:** All Requests (Type 0) inherit a `10s` default timeout.
*   **Configurable:** The SDK can pass a custom timeout via an `AbortSignal.timeout(ms)`.
*   **Resolution:** If the Rust host does not respond within the threshold, the SDK automatically rejects the Promise with a `TimeoutError` and emits a `CancelRequest` to Rust to clean up the pending task.

### 5. Protocol Guarantees

| Guarantee | Supported | Rationale |
| :--- | :--- | :--- |
| **Ordered** | Yes | Messages are processed sequentially within their priority tier. |
| **Reliable** | Yes | Shared memory buffers do not suffer from network packet loss. |
| **Duplicate Delivery** | No | Memory channel guarantees exactly-once processing. |
| **Backpressure** | Yes | Bounded channels + Frame Acknowledgments prevent flooding. |
| **Cancellation** | Yes | Dedicated `CancelRequest` envelope aborts active tasks. |
| **Streaming** | Yes | First-class support via multiplexed chunk envelopes. |
| **Version Negotiation** | Yes | Verified during Sandbox Allocation. |

### 6. Edge Case & Malformed Packet Handling
*   **Oversized Payloads:** Payloads exceeding the configured max limit (e.g., 50MB) are immediately dropped by the Host. A `PayloadTooLargeError` response is returned.
*   **Malformed Packets / Unknown Types:** If the Rust Host fails to decode the MessagePack array or encounters an unknown `Type`, it logs a fatal error and **terminates the V8 isolate**.
*   **Unknown Version:** SDK panics on boot. Extension state transitions to `Disabled(Incompatible)`.
*   **Fuzz Testing:** The CI pipeline includes a dedicated fuzzer that injects randomized bytes into the Rust IPC router to guarantee panic-free deserialization boundaries.
