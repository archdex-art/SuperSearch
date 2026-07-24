//! # Module 4: Reactive Dependency Graph
//!
//! A fine-grained, push-based reactive system that tracks dependencies between
//! signals (state), computed values (derived state), and effects (side effects).
//! Evaluation follows topological order to guarantee consistency.
//!
//! ## Fast-Path Bypass Integration
//! Critical-priority inputs bypass this graph on the initial frame pass via the
//! scheduler's fast-path mechanism. The bypassed update is then reconciled by
//! a deferred task that propagates through this graph at UserBlocking priority.
//!
//! ## CRDT Compatibility
//! Signal values are designed to be CRDT-compatible: merge operations must be
//! commutative, associative, and idempotent for local-first sync.

pub mod graph;
pub mod node;
pub mod reconcile;
pub mod signal;

pub use graph::DependencyGraph;
pub use node::{NodeId, NodeKind, ReactiveNode};
pub use reconcile::ReconciliationEngine;
pub use signal::{Computed, Effect, Signal};
