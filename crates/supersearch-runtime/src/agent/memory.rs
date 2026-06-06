//! Short-term agent memory — recent queries, context, and interaction history.
//!
//! Bounded ring buffer that keeps the last N interactions for context-aware
//! intent classification and suggestion ranking.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Instant;

/// Maximum interactions to remember.
const MAX_HISTORY: usize = 50;

/// Short-term memory for the agent.
pub struct AgentMemory {
    /// Recent query-response pairs.
    history: VecDeque<MemoryEntry>,
    /// When the memory was created (for relative timestamps).
    created_at: Instant,
}

/// A single interaction stored in memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// The user's query.
    pub query: String,
    /// The classified intent (serialized).
    pub intent_type: String,
    /// Whether the action succeeded.
    pub success: bool,
    /// Relative timestamp (seconds since memory init).
    pub timestamp_secs: f64,
    /// Brief result summary.
    pub result_summary: String,
}

impl Default for AgentMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentMemory {
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(MAX_HISTORY),
            created_at: Instant::now(),
        }
    }

    /// Record a completed interaction.
    pub fn record(
        &mut self,
        query: &str,
        intent_type: &str,
        success: bool,
        result_summary: &str,
    ) {
        if self.history.len() >= MAX_HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(MemoryEntry {
            query: query.to_string(),
            intent_type: intent_type.to_string(),
            success,
            timestamp_secs: self.created_at.elapsed().as_secs_f64(),
            result_summary: result_summary.to_string(),
        });
    }

    /// Get the N most recent entries.
    pub fn recent(&self, n: usize) -> Vec<&MemoryEntry> {
        self.history.iter().rev().take(n).collect()
    }

    /// Check if a query was recently executed (within last `n` entries).
    pub fn was_recent(&self, query: &str, n: usize) -> bool {
        self.history
            .iter()
            .rev()
            .take(n)
            .any(|e| e.query.to_lowercase() == query.to_lowercase())
    }

    /// Get the count of entries.
    pub fn len(&self) -> usize {
        self.history.len()
    }

    /// Check if memory is empty.
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }

    /// Clear all memory.
    pub fn clear(&mut self) {
        self.history.clear();
    }
}
