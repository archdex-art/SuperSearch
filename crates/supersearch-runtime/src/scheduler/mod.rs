//! # Module 1: Unified Runtime Scheduling Architecture
//!
//! Multi-Queue Cooperative Scheduler combining:
//! - Tokio cooperative scheduling (async yield points)
//! - Chromium task priorities (5-tier priority classes)
//! - React concurrent scheduling (interruptible rendering)
//! - Erlang supervision semantics (fault-isolated restart policies)
//!
//! ## Decoupled Governance Boundary
//!
//! This module is **exclusively** concerned with CPU time-slicing and task
//! lifecycle management. Quantitative resource accounting (token caps,
//! inference ceilings, API rate limits) is handled by an external governance
//! middleware that decorates futures *before* submission to this scheduler.
//! The scheduler treats all submitted futures as opaque work units.

pub mod priority;
pub mod task;
pub mod queue;
pub mod supervisor;
pub mod yielding;
pub mod executor;

pub use priority::PriorityClass;
pub use task::{TaskHandle, TaskDescriptor, TaskId, CancellationHandle};
pub use queue::MultiQueue;
pub use supervisor::{Supervisor, RestartPolicy, SupervisorStrategy};
pub use executor::SchedulerExecutor;
pub use yielding::cooperative_yield_loop;
