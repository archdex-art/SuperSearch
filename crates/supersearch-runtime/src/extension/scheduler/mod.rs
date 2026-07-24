//! # Extension Scheduler Subsystem (Phase 4 / Milestone 2)
//!
//! Owns the lifecycle, fair multiplexing, and routing of IPC messages between
//! concurrent V8 isolates and the core host application.

pub mod queue;

pub use queue::ExtensionScheduler;
