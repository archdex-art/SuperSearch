# PHASE 3 — ARCHITECTURE DECISION RECORDS (ADRs)

**Prepared By:** Principal Engineering Team

### ADR 001: Extension Guest Runtime Sandbox
*   **Recommendation:** V8 Isolates via `deno_core`.
*   **Rationale:** Preserves the speed/security of a Rust-bound sandbox while delivering flawless JS/TS developer experience. WASM models rejected due to DX overhead (compiling React to WASM) or performance penalty (JS engines inside WASM).

### ADR 002: IPC & UI Tree Serialization
*   **Recommendation:** MessagePack.
*   **Rationale:** React UI trees are polymorphic. Schema-strict protocols (Protobuf) create friction. Shared Memory diffing requires stateful patching across FFI; MessagePack full-tree serialization is faster and simpler for <16ms frames.

### ADR 003: Extension Local Storage Engine
*   **Recommendation:** SQLite.
*   **Rationale:** Excels at OLTP workloads (unlike DuckDB's OLAP focus). Enables complex local querying without loading data into V8 memory.

### ADR 004: Extension SDK Reconciler Framework
*   **Recommendation:** React 18 Custom Reconciler paired with V8 Snapshots.
*   **Rationale:** 100% ecosystem compatibility. V8 snapshots eliminate the massive parsing cost at startup.
