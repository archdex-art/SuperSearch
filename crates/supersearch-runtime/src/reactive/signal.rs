//! High-level Signal, Computed, and Effect primitives.
//!
//! These are the user-facing API for the reactive system. They wrap the
//! lower-level `DependencyGraph` and `ReactiveNode` with ergonomic types.

use std::any::Any;

use super::graph::DependencyGraph;
use super::node::{NodeId, ReactiveNode};

/// A reactive signal — mutable source state.
///
/// Signals are the roots of the reactive graph. When a signal's value changes,
/// all transitive dependents are re-evaluated in topological order.
///
/// ## Usage
/// ```rust,ignore
/// let graph = Arc::new(RwLock::new(DependencyGraph::new()));
/// let count = Signal::new(&mut graph.write().unwrap(), "count", 0u32);
/// count.set(&mut graph.write().unwrap(), 42u32);
/// ```
pub struct Signal {
    pub id: NodeId,
}

impl Signal {
    /// Create a new signal and register it in the dependency graph.
    pub fn new<T: Any + Send + Sync>(
        graph: &mut DependencyGraph,
        label: &'static str,
        initial: T,
    ) -> Self {
        let node = ReactiveNode::signal(label, initial);
        let id = graph.add_node(node);
        Self { id }
    }

    /// Get the current value of this signal.
    pub fn get<T: Any + Send + Sync + Clone>(&self, graph: &DependencyGraph) -> Option<T> {
        graph
            .get_node(&self.id)
            .and_then(|node| node.get::<T>().cloned())
    }

    /// Set the signal value and trigger reactive propagation.
    /// Returns the topological evaluation order of affected nodes.
    pub fn set<T: Any + Send + Sync>(&self, graph: &mut DependencyGraph, value: T) -> Vec<NodeId> {
        if let Some(node) = graph.get_node_mut(&self.id) {
            node.set(value);
        }
        graph.notify_change(self.id)
    }
}

/// A computed value — derived reactively from dependencies.
///
/// Computed values are lazily evaluated and cached. They only re-evaluate
/// when at least one dependency has changed (version-tracked).
pub struct Computed {
    pub id: NodeId,
}

impl Computed {
    /// Create a new computed node and declare its dependencies.
    pub fn new(graph: &mut DependencyGraph, label: &'static str, dependencies: &[NodeId]) -> Self {
        let node = ReactiveNode::computed(label);
        let id = graph.add_node(node);

        for &dep in dependencies {
            graph.add_dependency(id, dep);
        }

        Self { id }
    }

    /// Get the cached value.
    pub fn get<T: Any + Send + Sync + Clone>(&self, graph: &DependencyGraph) -> Option<T> {
        graph
            .get_node(&self.id)
            .and_then(|node| node.get::<T>().cloned())
    }

    /// Update the cached value after re-evaluation.
    pub fn update<T: Any + Send + Sync>(&self, graph: &mut DependencyGraph, value: T) {
        if let Some(node) = graph.get_node_mut(&self.id) {
            node.update_cached(value);
        }
    }
}

/// An effect — a side-effect that runs when its dependencies change.
///
/// Effects are terminal nodes in the reactive graph. They don't produce
/// values; they execute actions (e.g., updating the UI, sending IPC).
pub struct Effect {
    pub id: NodeId,
}

impl Effect {
    /// Create a new effect and declare its dependencies.
    pub fn new(graph: &mut DependencyGraph, label: &'static str, dependencies: &[NodeId]) -> Self {
        let node = ReactiveNode::effect(label);
        let id = graph.add_node(node);

        for &dep in dependencies {
            graph.add_dependency(id, dep);
        }

        Self { id }
    }

    /// Mark this effect as clean after execution.
    pub fn mark_executed(&self, graph: &mut DependencyGraph) {
        if let Some(node) = graph.get_node_mut(&self.id) {
            node.mark_clean();
        }
    }
}
