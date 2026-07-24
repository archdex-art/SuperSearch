# PHASE 10 — PERFORMANCE ARCHITECTURE

**Prepared By:** Principal Engineering Team
**Objective:** Define the benchmarking methodologies, memory optimizations, bundle constraints, and caching strategies required to guarantee a 60FPS UI and sub-10ms search latency across the Extension Platform.

---

### 1. Latency Targets & SLAs
The platform architecture is strictly bound by the following Service Level Agreements (SLAs):
*   **Search Latency:** `< 10ms` (From keystroke to local result generation).
*   **IPC Overhead:** `< 1ms` (Serialization + FFI crossing + Deserialization).
*   **UI Render Frame:** `< 16ms` (Maintaining smooth 60FPS).
*   **Extension Cold Start:** `< 50ms` (Time to interactive).

---

### 2. Startup Lifecycles (Cold vs. Warm)
Startup speed dictates the perceived quality of the launcher. We employ two distinct initialization profiles.

#### 2.1 Cold Start (Target: < 50ms)
Occurs when the app first launches, or an extension is invoked after being completely unloaded.
1.  **V8 Snapshot Load (~2ms):** Rust loads the pre-compiled `deno_core` snapshot (which includes the React Reconciler engine).
2.  **Isolate Initialization (~5ms):** V8 context is created and capability bindings are injected.
3.  **Script Evaluation (~20-40ms):** The extension's bundled JavaScript is parsed and executed by V8. (This is the primary bottleneck and is heavily monitored).

#### 2.2 Warm Start (Target: < 2ms)
Occurs when an extension transitions from `Suspended` to `Active`.
*   The V8 Isolate is already resident in RAM.
*   The Host emits a `Resume` event.
*   The Isolate instantly processes the event and fires any queued React effects. No parsing or initialization overhead occurs.

---

### 3. Bundle Optimization & Constraints
Large JS payloads destroy cold start times due to V8 parsing overhead.
*   **Build Pipeline:** Extensions must be compiled using the SuperSearch CLI (`@supersearch/cli`), which enforces `esbuild` or `swc` for aggressive minification and tree-shaking.
*   **Size Quotas:** The maximum permissible bundle size for a Quick Action is `1MB` (uncompressed). View extensions are capped at `5MB`. 
*   **External Dependencies:** Large libraries (e.g., `lodash`, `moment`) are strongly discouraged via CLI linting rules in favor of native JS APIs (e.g., `Intl`).

---

### 4. Memory Profiling & Garbage Collection
Running dozens of V8 Isolates can quickly consume gigabytes of RAM if unmanaged.
*   **Metrics:** Rust uses `v8::Isolate::get_heap_statistics()` to continuously profile active extensions.
*   **Suspension GC:** Before an extension is transitioned to the `Suspended` state, the Rust Host forcefully triggers a V8 Garbage Collection cycle (`isolate.request_garbage_collection_for_testing(v8::GarbageCollectionType::Full)`). This compacts the heap and returns unused memory to the OS *before* the isolate goes idle.
*   **Isolate Reuse Pool:** The Host maintains a strict ceiling (e.g., Max 10 Warm Isolates). When the 11th extension is loaded, the Least Recently Used (LRU) suspended isolate is completely `Unloaded` to reclaim memory.

---

### 5. Incremental Indexing Strategy
When a user types in the global search bar, we cannot afford to wake V8 Isolates or parse hundreds of `package.json` manifests.
*   **FST / Radix Tree:** During app boot (or when an extension is installed), the Rust Host parses all manifests and builds an in-memory Finite State Transducer (FST) or Radix Tree of all registered commands, titles, and aliases.
*   **Sub-millisecond Search:** The keystroke queries this in-memory Rust structure, resolving the search intent in `< 1ms` without ever touching V8 or the SQLite disk.

---

### 6. Caching Strategy
*   **SQLite Memory-Mapping:** The local SQLite storage engine uses `PRAGMA mmap_size` to memory-map frequently accessed extension data, bypassing disk I/O for hot queries.
*   **Network Caching:** The `@supersearch/api/network` module intercepts HTTP headers. If standard `Cache-Control` or `ETag` headers are present, the Rust Host transparently caches the binary response on disk. Subsequent identical fetches by the extension return instantly from the Rust cache without initiating a network socket.

---

### 7. Regression Benchmarking Methodology
Performance is treated as a security requirement. Code that degrades performance is rejected by CI.
*   **Rust (Backend):** We utilize `criterion.rs` to benchmark IPC serialization, channel throughput, and FST search resolution.
*   **JavaScript (Guest):** We utilize `mitata` (or similar V8 micro-benchmarkers) to measure the React Reconciler's UI Tree generation speed.
*   **CI Gates:** Every Pull Request to the core runtime runs these benchmarks against the `main` branch baseline. Any performance degradation exceeding **5%** automatically fails the build, requiring manual architectural review.
