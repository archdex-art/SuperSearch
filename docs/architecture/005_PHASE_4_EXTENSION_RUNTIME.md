# PHASE 4 — EXTENSION RUNTIME

**Prepared By:** Principal Engineering Team

### 1. Subsystem Overview
The Runtime is an asynchronous state machine in Rust (Tokio).

### 2. Core Subsystems
#### 2.1 Worker Manager & Sandbox Allocator
LRU cache of "warm" V8 snapshots. Checks if isolate exists (Resume), else instantiates `JsRuntime`, registers allowed FFI ops, and evaluates JS bundle.

#### 2.2 Lifecycle Manager
State Machine: `Unloaded` -> `Loading` -> `Active` <-> `Suspended` -> `Unloaded`. Crash recovery utilizes exponential backoff (max 3 restarts/60s).

#### 2.3 Resource Monitor (Watchdog)
Category-aware quotas:
*   Quick Action: Low CPU / 10MB RAM
*   View: Medium CPU / 30MB RAM
*   Background: Low CPU / High RAM
Exceeding quotas results in `v8::Isolate::terminate_execution`.

#### 2.4 Security
Each extension executes in its own V8 isolate with separate managed heaps, providing strong memory isolation at the JavaScript runtime level. Host-side Rust code forms the trusted computing base.
