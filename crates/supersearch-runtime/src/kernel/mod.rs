//! # Module 6: Runtime Kernel — Privileged Execution
//!
//! The kernel owns privileged execution: process management and the bridge
//! between sandboxed plugins and the host operating system. Raw OS automation
//! (launch/open/clipboard/filesystem) lives behind the Platform Abstraction
//! Layer ([`crate::platform`]) and is driven by the agent executor — the kernel
//! does not duplicate it. All operations here are capability-gated — even the
//! kernel itself uses capability tokens for internal auditing.
//!
//! ## Separation of Concerns
//! - **Kernel**: Owns process management, mediates plugin access.
//! - **Platform**: Owns OS automation backends (Module `platform`).
//! - **Scheduler**: Owns CPU time-slicing (Module 1).
//! - **Journal**: Owns deterministic replay (Module 2).
//! - **Capabilities**: Own security policy (Module 3).
//!
//! The kernel orchestrates these modules but does not subsume their logic.

pub mod process;
pub mod runtime;

pub use process::{ManagedProcess, ProcessConfig, ProcessManager};
pub use runtime::RuntimeKernel;
