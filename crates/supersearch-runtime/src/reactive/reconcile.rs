//! Reconciliation engine for fast-path bypass.
//!
//! When a Critical-priority input uses the optimistic fast-path bypass,
//! it applies a speculative state update directly to the render pipeline
//! WITHOUT going through the reactive dependency graph. This creates a
//! temporal inconsistency window.
//!
//! The ReconciliationEngine resolves this by:
//! 1. Accepting the fast-path delta (what changed speculatively).
//! 2. Propagating the change through the reactive graph at UserBlocking priority.
//! 3. Comparing the graph's result with the speculative result.
//! 4. If they diverge, issuing a correction to the render pipeline.
//!
//! The reconciliation must be idempotent and convergent — essentially a
//! CRDT-like merge where the speculative and canonical states converge.


use std::time::Instant;
use tracing::{debug, info, warn};

use super::node::NodeId;
use super::graph::DependencyGraph;
use crate::scheduler::task::TaskId;

/// A fast-path delta: the speculative change applied before graph evaluation.
#[derive(Debug, Clone)]
pub struct FastPathDelta {
    /// The scheduler task that initiated the fast-path bypass.
    pub source_task: TaskId,
    /// The signal that was speculatively updated.
    pub signal_id: NodeId,
    /// Serialized speculative value (for comparison after reconciliation).
    pub speculative_value: Vec<u8>,
    /// When the fast-path was applied.
    pub applied_at: Instant,
}

/// Result of reconciliation.
#[derive(Debug, Clone)]
pub enum ReconciliationResult {
    /// Speculative value matches canonical graph evaluation. No correction needed.
    Converged {
        signal_id: NodeId,
        elapsed_us: u64,
    },
    /// Speculative value diverges from canonical. A correction patch is emitted.
    Diverged {
        signal_id: NodeId,
        /// IDs of nodes that need correction in the render pipeline.
        corrected_nodes: Vec<NodeId>,
        elapsed_us: u64,
    },
}

/// The reconciliation engine.
pub struct ReconciliationEngine {
    /// Pending fast-path deltas awaiting reconciliation.
    pending: Vec<FastPathDelta>,
    /// Statistics.
    total_reconciled: u64,
    total_converged: u64,
    total_diverged: u64,
}

impl ReconciliationEngine {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            total_reconciled: 0,
            total_converged: 0,
            total_diverged: 0,
        }
    }

    /// Register a fast-path delta for deferred reconciliation.
    pub fn register_delta(&mut self, delta: FastPathDelta) {
        debug!(
            task = %delta.source_task,
            signal = %delta.signal_id,
            "Fast-path delta registered for reconciliation"
        );
        self.pending.push(delta);
    }

    /// Reconcile all pending deltas against the reactive dependency graph.
    ///
    /// This should be called as a UserBlocking-priority task after the
    /// fast-path frame has been rendered.
    ///
    /// ## Arguments
    /// - `graph`: The reactive dependency graph (source of canonical state).
    /// - `compare_fn`: A function that compares the speculative value bytes
    ///   against the canonical value for a given signal node.
    ///   Returns `true` if they match (converged), `false` if diverged.
    pub fn reconcile<F>(
        &mut self,
        graph: &mut DependencyGraph,
        compare_fn: F,
    ) -> Vec<ReconciliationResult>
    where
        F: Fn(NodeId, &[u8]) -> bool,
    {
        let deltas: Vec<FastPathDelta> = self.pending.drain(..).collect();
        let mut results = Vec::with_capacity(deltas.len());

        for delta in deltas {
            let start = Instant::now();

            // Propagate the signal change through the canonical graph.
            let eval_order = graph.notify_change(delta.signal_id);

            // Compare speculative vs canonical.
            let converged = compare_fn(delta.signal_id, &delta.speculative_value);

            let elapsed_us = start.elapsed().as_micros() as u64;

            if converged {
                self.total_converged += 1;
                debug!(
                    signal = %delta.signal_id,
                    elapsed_us,
                    "Reconciliation converged — no correction needed"
                );
                results.push(ReconciliationResult::Converged {
                    signal_id: delta.signal_id,
                    elapsed_us,
                });
            } else {
                self.total_diverged += 1;
                warn!(
                    signal = %delta.signal_id,
                    affected_nodes = eval_order.len(),
                    elapsed_us,
                    "Reconciliation DIVERGED — emitting correction"
                );
                results.push(ReconciliationResult::Diverged {
                    signal_id: delta.signal_id,
                    corrected_nodes: eval_order,
                    elapsed_us,
                });
            }

            self.total_reconciled += 1;
        }

        if !results.is_empty() {
            info!(
                count = results.len(),
                converged = self.total_converged,
                diverged = self.total_diverged,
                "Reconciliation batch complete"
            );
        }

        results
    }

    /// Number of pending deltas awaiting reconciliation.
    pub fn pending_count(&self) -> usize { self.pending.len() }

    pub fn total_reconciled(&self) -> u64 { self.total_reconciled }
    pub fn convergence_rate(&self) -> f64 {
        if self.total_reconciled == 0 { return 1.0; }
        self.total_converged as f64 / self.total_reconciled as f64
    }
}
