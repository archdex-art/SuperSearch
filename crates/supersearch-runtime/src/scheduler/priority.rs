//! Priority classification for scheduled tasks.
//!
//! Each priority class carries a hard latency budget derived from human
//! perception thresholds and UI responsiveness requirements. These budgets
//! are enforced by the scheduler's deadline-aware dispatching — tasks that
//! exceed their budget are candidates for preemption via cooperative yield
//! or cancellation.

use std::time::Duration;

use serde::{Serialize, Deserialize};

/// The five-tier priority classification mirroring Chromium's task scheduling
/// model, extended with latency budgets from the runtime specification.
///
/// Priority classes are ordered from highest to lowest urgency. The scheduler
/// drains higher-priority queues before lower ones, with starvation prevention
/// via aging (see [`MultiQueue`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PriorityClass {
    /// **Critical**: < 4ms latency budget.
    ///
    /// Reserved for input-path-critical work: keyboard events, command palette
    /// activation, accessibility focus changes. These tasks bypass the reactive
    /// dependency graph on the initial frame pass (optimistic fast-path) and
    /// back-propagate state asynchronously.
    ///
    /// Implementation note: Critical tasks are enqueued into a dedicated
    /// lock-free SPSC channel to avoid contention with lower-priority producers.
    Critical = 0,

    /// **Interactive**: < 16ms latency budget (one frame at 60fps).
    ///
    /// Command execution, menu interactions, drag operations. Must complete
    /// within a single frame to avoid perceptible jank.
    Interactive = 1,

    /// **UserBlocking**: < 50ms latency budget.
    ///
    /// Workflow execution, file operations, plugin lifecycle transitions.
    /// Perceptible but tolerable latency — the user initiated the action and
    /// expects a brief wait.
    UserBlocking = 2,

    /// **Background**: < 250ms latency budget.
    ///
    /// Indexing, incremental compilation, CRDT synchronization, event journal
    /// compaction. Not directly user-initiated; should yield aggressively to
    /// higher-priority work.
    Background = 3,

    /// **Idle**: Opportunistic, no hard deadline.
    ///
    /// Embedding generation, speculative prefetching, telemetry aggregation.
    /// Only scheduled when all higher-priority queues are drained.
    Idle = 4,
}

impl PriorityClass {
    /// Returns the hard latency budget for this priority class.
    ///
    /// The scheduler uses this to set per-task deadlines. Tasks exceeding
    /// their budget trigger cooperative yield checks and, if unresponsive,
    /// cancellation via the task's `CancellationToken`.
    #[inline]
    pub const fn latency_budget(self) -> Duration {
        match self {
            // < 4ms: keyboard input must be reflected within 4ms to feel instant.
            // Actual target is 2ms to leave headroom for compositor submission.
            PriorityClass::Critical => Duration::from_micros(3_800),

            // < 16ms: one frame at 60fps. We budget 14ms to leave 2ms for
            // frame composition and vsync alignment.
            PriorityClass::Interactive => Duration::from_micros(14_000),

            // < 50ms: perceptible threshold. Budget 45ms with 5ms headroom.
            PriorityClass::UserBlocking => Duration::from_micros(45_000),

            // < 250ms: background threshold. Budget 230ms.
            PriorityClass::Background => Duration::from_micros(230_000),

            // Idle: 1 second soft budget. These tasks should yield frequently
            // and will be interrupted by any higher-priority enqueue.
            PriorityClass::Idle => Duration::from_secs(1),
        }
    }

    /// Returns the maximum number of consecutive polls allowed before a
    /// mandatory yield point for tasks of this priority class.
    ///
    /// This prevents a single long-running future from monopolizing the
    /// executor thread even if its wall-clock time hasn't exceeded the budget.
    /// Values are calibrated against Tokio's default budget of 128 polls.
    #[inline]
    pub const fn poll_budget(self) -> u32 {
        match self {
            PriorityClass::Critical => 4,     // Finish fast or yield immediately
            PriorityClass::Interactive => 16,  // One frame's worth of polls
            PriorityClass::UserBlocking => 64,
            PriorityClass::Background => 128,  // Tokio default
            PriorityClass::Idle => 256,
        }
    }

    /// Returns the starvation aging threshold: how many scheduler ticks a
    /// task at this priority can wait before being promoted one level.
    ///
    /// Prevents indefinite starvation of lower-priority work during sustained
    /// high-priority load (e.g., continuous typing generating Critical tasks).
    #[inline]
    pub const fn aging_threshold_ticks(self) -> u64 {
        match self {
            PriorityClass::Critical => u64::MAX, // Never demoted; already highest
            PriorityClass::Interactive => 120,    // ~2 seconds at 60 ticks/sec
            PriorityClass::UserBlocking => 300,   // ~5 seconds
            PriorityClass::Background => 600,     // ~10 seconds
            PriorityClass::Idle => 1800,          // ~30 seconds
        }
    }

    /// Attempt to promote this priority class by one level for starvation
    /// prevention. Returns `None` if already at `Critical`.
    #[inline]
    pub const fn promote(self) -> Option<PriorityClass> {
        match self {
            PriorityClass::Critical => None,
            PriorityClass::Interactive => Some(PriorityClass::Critical),
            PriorityClass::UserBlocking => Some(PriorityClass::Interactive),
            PriorityClass::Background => Some(PriorityClass::UserBlocking),
            PriorityClass::Idle => Some(PriorityClass::Background),
        }
    }

    /// Returns all priority classes in descending urgency order.
    #[inline]
    pub const fn all_descending() -> [PriorityClass; 5] {
        [
            PriorityClass::Critical,
            PriorityClass::Interactive,
            PriorityClass::UserBlocking,
            PriorityClass::Background,
            PriorityClass::Idle,
        ]
    }
}

impl std::fmt::Display for PriorityClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PriorityClass::Critical => write!(f, "CRITICAL(<4ms)"),
            PriorityClass::Interactive => write!(f, "INTERACTIVE(<16ms)"),
            PriorityClass::UserBlocking => write!(f, "USER_BLOCKING(<50ms)"),
            PriorityClass::Background => write!(f, "BACKGROUND(<250ms)"),
            PriorityClass::Idle => write!(f, "IDLE(opportunistic)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_ordering_is_correct() {
        assert!(PriorityClass::Critical < PriorityClass::Interactive);
        assert!(PriorityClass::Interactive < PriorityClass::UserBlocking);
        assert!(PriorityClass::UserBlocking < PriorityClass::Background);
        assert!(PriorityClass::Background < PriorityClass::Idle);
    }

    #[test]
    fn latency_budgets_are_monotonically_increasing() {
        let classes = PriorityClass::all_descending();
        for window in classes.windows(2) {
            assert!(
                window[0].latency_budget() < window[1].latency_budget(),
                "{} budget should be less than {} budget",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn promotion_chain_reaches_critical() {
        let mut current = PriorityClass::Idle;
        let mut promotions = 0;
        while let Some(promoted) = current.promote() {
            current = promoted;
            promotions += 1;
            assert!(promotions <= 4, "infinite promotion loop detected");
        }
        assert_eq!(current, PriorityClass::Critical);
        assert_eq!(promotions, 4);
    }
}
