//! # Module 5: Sandboxed Plugin Runtime
//!
//! Adapter plugins (ChatGPT, VSCode, Terminal, etc.) execute inside sandboxed
//! WASM runtimes with capability-scoped environments. Plugins NEVER directly
//! access unrestricted OS APIs — all privileged operations go through the
//! capability gate.
//!
//! ## Plugin Lifecycle
//! 1. **Load**: Parse manifest, compile WASM module, allocate sandbox.
//! 2. **Initialize**: Inject granted capabilities, start supervisor child.
//! 3. **Run**: Plugin executes, sending IPC messages through Cap'n Proto channels.
//! 4. **Suspend**: Plugin yields its time slice, state preserved in sandbox.
//! 5. **Unload**: Revoke all capabilities, teardown sandbox, notify supervisor.
//!
//! ## Isolation Guarantees
//! - Memory: Each plugin has its own WASM linear memory (default 16 MiB).
//! - CPU: Time-sliced by the scheduler with per-plugin poll budgets.
//! - Capabilities: Injected at load, revocable at any time.
//! - IPC: Mediated through Cap'n Proto channels with the kernel.

pub mod host;
pub mod ipc;
pub mod manifest;
pub mod sandbox;

pub use host::{PluginHost, PluginState};
pub use ipc::{IpcChannel, IpcMessage};
pub use manifest::{PluginManifest, PluginPermissionRequest};
pub use sandbox::WasmSandbox;
