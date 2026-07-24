//! Reactive graph nodes — signals, computed values, and effects.

use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Unique identifier for a reactive node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

impl NodeId {
    #[inline]
    pub fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        NodeId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "node#{}", self.0)
    }
}

/// Classification of reactive nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// Source signal — mutable state owned by the application.
    /// Changes to signals initiate reactive propagation.
    Signal,
    /// Computed value — derived from one or more signals/computed nodes.
    /// Lazily evaluated, cached, and invalidated when dependencies change.
    Computed,
    /// Effect — side-effect triggered when its dependencies change.
    /// Effects are the terminal nodes in the dependency graph.
    Effect,
}

/// Version counter for change detection. Incremented on every mutation.
/// Computed nodes compare their cached version against their dependencies'
/// versions to determine if re-evaluation is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64);

impl Version {
    pub const INITIAL: Version = Version(0);

    #[inline]
    pub fn increment(&mut self) {
        self.0 += 1;
    }
}

/// Dirty flags for incremental evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyState {
    /// Node's value is current — no re-evaluation needed.
    Clean,
    /// Node might be dirty — needs to check its dependencies' versions.
    /// This is the intermediate state during propagation.
    MaybeDirty,
    /// Node is definitely dirty — re-evaluation required.
    Dirty,
}

/// A node in the reactive dependency graph.
///
/// Nodes are type-erased (`Box<dyn Any>`) to allow heterogeneous values
/// in the same graph. The graph manages dependencies and evaluation order;
/// individual nodes manage their values and dirty states.
pub struct ReactiveNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub label: &'static str,
    /// Current version of this node's value.
    pub version: Version,
    /// Dirty state for incremental evaluation.
    pub dirty: DirtyState,
    /// The node's current value (type-erased).
    value: Box<dyn Any + Send + Sync>,
    /// Version of each dependency when this node was last evaluated.
    /// Only meaningful for Computed and Effect nodes.
    pub dependency_versions: Vec<(NodeId, Version)>,
}

impl ReactiveNode {
    /// Create a new signal node with an initial value.
    pub fn signal<T: Any + Send + Sync>(label: &'static str, value: T) -> Self {
        Self {
            id: NodeId::next(),
            kind: NodeKind::Signal,
            label,
            version: Version::INITIAL,
            dirty: DirtyState::Clean,
            value: Box::new(value),
            dependency_versions: Vec::new(),
        }
    }

    /// Create a new computed node (initially dirty, no cached value).
    pub fn computed(label: &'static str) -> Self {
        Self {
            id: NodeId::next(),
            kind: NodeKind::Computed,
            label,
            version: Version::INITIAL,
            dirty: DirtyState::Dirty,
            value: Box::new(()) as Box<dyn Any + Send + Sync>,
            dependency_versions: Vec::new(),
        }
    }

    /// Create a new effect node.
    pub fn effect(label: &'static str) -> Self {
        Self {
            id: NodeId::next(),
            kind: NodeKind::Effect,
            label,
            version: Version::INITIAL,
            dirty: DirtyState::Dirty,
            value: Box::new(()) as Box<dyn Any + Send + Sync>,
            dependency_versions: Vec::new(),
        }
    }

    /// Get the current value as a concrete type.
    #[inline]
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.value.downcast_ref::<T>()
    }

    /// Set the value (for signals). Increments version and marks dependents dirty.
    pub fn set<T: Any + Send + Sync>(&mut self, value: T) {
        self.value = Box::new(value);
        self.version.increment();
        // Note: marking dependents dirty is handled by the DependencyGraph,
        // not here — this node doesn't know its dependents.
    }

    /// Update the cached value (for computed nodes).
    pub fn update_cached<T: Any + Send + Sync>(&mut self, value: T) {
        self.value = Box::new(value);
        self.version.increment();
        self.dirty = DirtyState::Clean;
    }

    /// Mark this node as clean.
    #[inline]
    pub fn mark_clean(&mut self) {
        self.dirty = DirtyState::Clean;
    }

    /// Mark this node as dirty.
    #[inline]
    pub fn mark_dirty(&mut self) {
        self.dirty = DirtyState::Dirty;
    }

    /// Mark this node as maybe-dirty (needs dependency check).
    #[inline]
    pub fn mark_maybe_dirty(&mut self) {
        self.dirty = DirtyState::MaybeDirty;
    }
}

impl std::fmt::Debug for ReactiveNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReactiveNode")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("label", &self.label)
            .field("version", &self.version)
            .field("dirty", &self.dirty)
            .finish()
    }
}
