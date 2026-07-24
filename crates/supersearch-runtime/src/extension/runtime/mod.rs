//! # V8 Runtime Subsystem (Phase 4)
//!
//! Owns the `deno_core` execution environment. Isolates extensions into strictly
//! separate heaps with bounded resource constraints.

pub mod allocator;
pub mod hmr;
pub mod isolate;
pub mod ops;
pub use allocator::SandboxAllocator;
pub use isolate::V8Isolate;
