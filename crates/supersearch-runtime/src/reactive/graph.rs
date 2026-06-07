//! Dependency graph with topological evaluation.
//!
//! Uses `petgraph::DiGraph` for the underlying graph structure. Edges represent
//! "depends on" relationships: an edge from A → B means "A depends on B"
//! (B is a dependency of A).
//!
//! ## Evaluation Strategy
//! When a signal changes:
//! 1. Mark all transitive dependents as `MaybeDirty`.
//! 2. Topologically sort the dirty subgraph.
//! 3. Evaluate nodes in topological order, skipping nodes whose dependencies
//!    haven't actually changed (version comparison).

use std::collections::{HashMap, HashSet, VecDeque};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use tracing::{debug, trace};

use super::node::{NodeId, ReactiveNode};

/// The dependency graph managing reactive nodes and their relationships.
pub struct DependencyGraph {
    /// The underlying directed graph. Edges: dependent → dependency.
    graph: DiGraph<NodeId, ()>,
    /// NodeId → petgraph NodeIndex mapping.
    index_map: HashMap<NodeId, NodeIndex>,
    /// NodeId → ReactiveNode storage.
    nodes: HashMap<NodeId, ReactiveNode>,
    /// Evaluation batch counter for stats.
    eval_count: u64,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            index_map: HashMap::new(),
            nodes: HashMap::new(),
            eval_count: 0,
        }
    }

    /// Add a reactive node to the graph.
    pub fn add_node(&mut self, node: ReactiveNode) -> NodeId {
        let id = node.id;
        let idx = self.graph.add_node(id);
        self.index_map.insert(id, idx);
        self.nodes.insert(id, node);
        debug!(id = %id, "Node added to dependency graph");
        id
    }

    /// Declare that `dependent` depends on `dependency`.
    /// Edge direction: dependent → dependency.
    pub fn add_dependency(&mut self, dependent: NodeId, dependency: NodeId) {
        let dep_idx = self.index_map[&dependent];
        let src_idx = self.index_map[&dependency];
        self.graph.add_edge(dep_idx, src_idx, ());
        trace!(dependent = %dependent, dependency = %dependency, "Dependency edge added");
    }

    /// Get a reference to a node.
    #[inline]
    pub fn get_node(&self, id: &NodeId) -> Option<&ReactiveNode> {
        self.nodes.get(id)
    }

    /// Get a mutable reference to a node.
    #[inline]
    pub fn get_node_mut(&mut self, id: &NodeId) -> Option<&mut ReactiveNode> {
        self.nodes.get_mut(id)
    }

    /// Notify the graph that a signal has changed. This marks all transitive
    /// dependents as MaybeDirty and returns the topological evaluation order.
    ///
    /// ## Algorithm
    /// 1. BFS from the changed signal along reverse edges (dependents).
    /// 2. Collect all reachable nodes and mark them MaybeDirty.
    /// 3. Topological sort the dirty subgraph using Kahn's algorithm.
    /// 4. Return the sorted node IDs for evaluation.
    pub fn notify_change(&mut self, signal_id: NodeId) -> Vec<NodeId> {
        let signal_idx = match self.index_map.get(&signal_id) {
            Some(idx) => *idx,
            None => return Vec::new(),
        };

        // Phase 1: BFS to find all transitive dependents.
        let mut dirty_set: HashSet<NodeIndex> = HashSet::new();
        let mut queue: VecDeque<NodeIndex> = VecDeque::new();

        // Find all nodes that depend ON the signal (reverse edge direction).
        // In our graph, edges go dependent → dependency, so we follow
        // incoming edges to find dependents.
        for neighbor in self.graph.neighbors_directed(signal_idx, Direction::Incoming) {
            queue.push_back(neighbor);
            dirty_set.insert(neighbor);
        }

        while let Some(idx) = queue.pop_front() {
            // Mark this node.
            let node_id = self.graph[idx];
            if let Some(node) = self.nodes.get_mut(&node_id) {
                node.mark_maybe_dirty();
            }
            // Propagate to nodes that depend on this one.
            for neighbor in self.graph.neighbors_directed(idx, Direction::Incoming) {
                if dirty_set.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }

        // Phase 2: Topological sort of the dirty subgraph (Kahn's algorithm).
        // We need to evaluate dependencies before dependents.
        let mut in_degree: HashMap<NodeIndex, usize> = HashMap::new();
        for &idx in &dirty_set {
            let mut deg = 0;
            for dep in self.graph.neighbors_directed(idx, Direction::Outgoing) {
                if dirty_set.contains(&dep) || dep == signal_idx {
                    deg += 1;
                }
            }
            in_degree.insert(idx, deg);
        }

        let mut eval_order: Vec<NodeId> = Vec::with_capacity(dirty_set.len());
        let mut ready: VecDeque<NodeIndex> = in_degree.iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&idx, _)| idx)
            .collect();

        while let Some(idx) = ready.pop_front() {
            eval_order.push(self.graph[idx]);
            for neighbor in self.graph.neighbors_directed(idx, Direction::Incoming) {
                if let Some(deg) = in_degree.get_mut(&neighbor) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        ready.push_back(neighbor);
                    }
                }
            }
        }

        self.eval_count += 1;
        debug!(
            signal = %signal_id,
            dirty_nodes = dirty_set.len(),
            eval_order_len = eval_order.len(),
            "Change propagation computed"
        );

        eval_order
    }

    /// Remove a node and all its edges from the graph.
    pub fn remove_node(&mut self, id: &NodeId) {
        if let Some(idx) = self.index_map.remove(id) {
            self.graph.remove_node(idx);
            self.nodes.remove(id);
            // Note: petgraph renumbers indices on removal. In production,
            // use StableGraph to avoid this. For now, rebuild the index map.
            self.rebuild_index_map();
        }
    }

    /// Rebuild the NodeId → NodeIndex map after graph mutations.
    fn rebuild_index_map(&mut self) {
        self.index_map.clear();
        for idx in self.graph.node_indices() {
            let id = self.graph[idx];
            self.index_map.insert(id, idx);
        }
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize { self.graph.node_count() }

    /// Number of dependency edges.
    pub fn edge_count(&self) -> usize { self.graph.edge_count() }

    /// Total evaluation batches performed.
    pub fn eval_count(&self) -> u64 { self.eval_count }
}
