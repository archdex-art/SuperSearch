//! # Module 6: Runtime Kernel — OS Automation Primitives
//!
//! The kernel owns privileged execution: OS automation, process management,
//! and the bridge between sandboxed plugins and the host operating system.
//! All operations here are capability-gated — even the kernel itself uses
//! capability tokens for internal auditing.
//!
//! ## Separation of Concerns
//! - **Kernel**: Owns OS primitives, mediates plugin access.
//! - **Scheduler**: Owns CPU time-slicing (Module 1).
//! - **Journal**: Owns deterministic replay (Module 2).
//! - **Capabilities**: Own security policy (Module 3).
//!
//! The kernel orchestrates these modules but does not subsume their logic.

pub mod automation;
pub mod process;
pub mod runtime;

pub use automation::{OsAutomation, AutomationAction, AutomationResult};
pub use process::{ProcessManager, ManagedProcess, ProcessConfig};
pub use runtime::RuntimeKernel;
