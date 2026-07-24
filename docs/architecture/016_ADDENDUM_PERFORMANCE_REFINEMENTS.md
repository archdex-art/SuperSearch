# PHASE 10 ADDENDUM — PERFORMANCE REFINEMENTS

*Solidifying performance governance by establishing tail latency SLAs, strict memory budgets, and the release validation matrix.*

### 1. Tail Latency SLAs (Instead of Averages)
Production performance is judged by the 99th percentile. "Search Latency" refers strictly to the Command Lookup + FST Ranking pipeline (excluding UI render time).

| Metric | P50 | P95 | P99 |
| :--- | :--- | :--- | :--- |
| **Search Lookup** | `< 2 ms` | `< 5 ms` | `< 10 ms` |
| **IPC Overhead** | `< 0.2 ms` | `< 0.5 ms` | `< 1.0 ms` |
| **Frame Render** | `< 8 ms` | `< 12 ms` | `< 16 ms` |
| **Cold Startup** | `< 20 ms` | `< 35 ms` | `< 50 ms` |

### 2. Consolidated Memory Budget
To run effectively on baseline hardware (e.g., 8GB RAM laptops), we enforce the following memory ceilings per session:

| Component | Target Budget | Hard Limit |
| :--- | :--- | :--- |
| **Rust Host (Core)** | 30 MB | 50 MB |
| **React Renderer (UI)** | 40 MB | 80 MB |
| **SQLite Cache (Mmap)** | 10 MB | 50 MB |
| **Active Isolate (Guest)** | 5 MB - 20 MB | 50 MB (Terminated if exceeded) |
| **Idle Isolate (Suspended)**| 2 MB | 5 MB |

### 3. Benchmark Reproducibility Standard
To prevent environmental noise from polluting CI regression tests, benchmarks must specify:
*   **Hardware:** GitHub Actions Apple Silicon (M1) runners, 7GB RAM.
*   **Environment:** macOS 14, Release build, LTO enabled, `panic=abort`.
*   **Warm-up:** 1,000 iterations discarded before measurement.
*   **Iteration Count:** 10,000 samples per metric.
*   **Statistical Confidence:** 95% confidence interval using `criterion.rs` bootstrapping.

### 4. Performance Budget Ownership
If a regression occurs in CI, responsibility is assigned strictly by subsystem:

| Budget | Owner Subsystem |
| :--- | :--- |
| **Startup (Isolate + Snapshot)** | Runtime Engine (`supersearch-runtime`) |
| **IPC (Serialization + FFI)** | IPC Layer |
| **Rendering (DOM + Virtualization)**| React Renderer |
| **Command Search (Lookup + Ranking)**| Rust FST Index |
| **Network & Caching** | Host / System integrations |
| **Storage (ACID + Mmap)** | SQLite Wrapper |

### 5. Telemetry & Cache Invalidation
*   **Telemetry Integration:** Metrics (Isolate boot time, serialization duration, GC pause time, warm reuse rate) are emitted via tracing spans and aggregated into the Host's observability SQLite database.
*   **Cache Invalidation:** HTTP caches follow standard `Cache-Control` TTL. If corrupted (detected via deserialization failure), the specific cache file is instantly unlinked, and a fresh request is forced.

### 6. Performance Validation Matrix (Release Checklist)

| Scenario | Target | Benchmark Method |
| :--- | :--- | :--- |
| **Cold Boot (App to Searchable)** | `< 50 ms` | Automated (`criterion.rs`) |
| **Warm Boot (Suspended to Active)**| `< 2 ms` | Automated (`criterion.rs`) |
| **Registry Index (1,000 Extensions)**| `< 10 ms lookup` | Automated (`criterion.rs`) |
| **Virtualization (5,000 List Items)**| `< 16 ms frame` | Automated (`mitata` / React Profiler) |
| **Large Payload (5MB JSON tree)** | `< 1 ms serialization` | Automated (`mitata` FFI harness) |
| **Cache Hit (SQLite Mmap)** | `< 1 ms` | Automated (`criterion.rs`) |
