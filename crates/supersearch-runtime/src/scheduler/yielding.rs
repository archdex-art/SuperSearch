//! Cooperative yielding with budget-aware backpressure.
//!
//! Unlike a bare `tokio::task::yield_now()` (which is a blind single-poll yield),
//! this module provides a yield loop that checks elapsed wall-clock time against
//! the task's deadline and poll count against its budget.

use std::time::Instant;
use super::task::TaskDescriptor;

/// Outcome of a yield check — tells the caller whether to continue, yield, or abort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YieldDecision {
    /// Budget remains. Continue executing.
    Continue,
    /// Poll budget exhausted or deadline approaching. Yield to scheduler.
    Yield,
    /// Task has been cancelled or deadline exceeded. Abort immediately.
    Abort,
}

/// Budget-aware yield context carried through a task's execution.
///
/// Created at task start, checked at yield points. The `check()` method
/// costs ~25ns (one `Instant::now()`) and should be called at natural
/// suspension points in long-running computations.
pub struct YieldContext {
    polls_remaining: u32,
    deadline: Instant,
    cancelled: bool,
}

impl YieldContext {
    /// Create a yield context from a task descriptor.
    #[inline]
    pub fn from_descriptor(desc: &TaskDescriptor) -> Self {
        Self {
            polls_remaining: desc.poll_budget,
            deadline: desc.deadline_at,
            cancelled: desc.cancellation.is_cancelled(),
        }
    }

    /// Check whether the task should yield or abort.
    ///
    /// This is the hot-path check called at every yield point.
    /// Cost breakdown:
    ///   - `is_cancelled()`: ~1ns (atomic load)
    ///   - `Instant::now()`: ~25ns (clock_gettime)
    ///   - Total: ~26ns per check
    #[inline]
    pub fn check(&mut self) -> YieldDecision {
        // 1. Cancellation check (cheapest — single atomic load).
        if self.cancelled {
            return YieldDecision::Abort;
        }

        // 2. Poll budget check (no syscall, just a decrement).
        if self.polls_remaining == 0 {
            return YieldDecision::Yield;
        }
        self.polls_remaining -= 1;

        // 3. Deadline check (~25ns for Instant::now()).
        // We check this after the poll budget to avoid the syscall cost
        // when the budget is already exhausted.
        if Instant::now() >= self.deadline {
            return YieldDecision::Abort;
        }

        YieldDecision::Continue
    }

    /// Update the cancellation flag from an external source.
    /// Called by the executor between polls.
    #[inline]
    pub fn sync_cancellation(&mut self, desc: &TaskDescriptor) {
        self.cancelled = desc.cancellation.is_cancelled();
    }
}

/// Execute a cooperative yield loop for CPU-bound work within a task.
///
/// This function wraps a chunked computation pattern:
/// 1. Process one chunk of work.
/// 2. Check the yield context.
/// 3. If `Yield`, call `tokio::task::yield_now().await` to let the scheduler
///    run higher-priority tasks.
/// 4. If `Abort`, return early.
///
/// # Arguments
/// * `desc` — The task's descriptor (for deadline and budget).
/// * `work` — An async closure called repeatedly. Returns `Some(result)` if
///   there is more work, or `None` when complete.
///
/// # Returns
/// A vector of results from each chunk, or `None` if aborted.
///
/// # Example
/// ```rust,ignore
/// let results = cooperative_yield_loop(&descriptor, |chunk_idx| async move {
///     if chunk_idx < total_chunks {
///         let result = process_chunk(chunk_idx);
///         Some(result)
///     } else {
///         None // done
///     }
/// }).await;
/// ```
pub async fn cooperative_yield_loop<T, F, Fut>(
    desc: &TaskDescriptor,
    mut work: F,
) -> Option<Vec<T>>
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let mut ctx = YieldContext::from_descriptor(desc);
    let mut results = Vec::new();
    let mut chunk_idx = 0;

    loop {
        // Check budget before each chunk.
        match ctx.check() {
            YieldDecision::Continue => {}
            YieldDecision::Yield => {
                // Cooperative yield — return control to the Tokio executor so
                // higher-priority tasks (e.g., Critical keyboard input at <4ms)
                // can be serviced.
                tokio::task::yield_now().await;
                // After resumption, refresh cancellation state.
                ctx.sync_cancellation(desc);
                // Reset poll budget for next slice.
                ctx.polls_remaining = desc.poll_budget;
                continue;
            }
            YieldDecision::Abort => {
                // Deadline exceeded or cancelled. Return partial results.
                return None;
            }
        }

        // Execute one chunk of work.
        match work(chunk_idx).await {
            Some(result) => {
                results.push(result);
                chunk_idx += 1;
            }
            None => {
                // Work is complete.
                return Some(results);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn yield_context_exhausts_poll_budget() {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut ctx = YieldContext {
            polls_remaining: 3,
            deadline,
            cancelled: false,
        };
        assert_eq!(ctx.check(), YieldDecision::Continue); // 2 remaining
        assert_eq!(ctx.check(), YieldDecision::Continue); // 1 remaining
        assert_eq!(ctx.check(), YieldDecision::Continue); // 0 remaining
        assert_eq!(ctx.check(), YieldDecision::Yield);    // exhausted
    }

    #[test]
    fn yield_context_aborts_on_cancellation() {
        let mut ctx = YieldContext {
            polls_remaining: 100,
            deadline: Instant::now() + Duration::from_secs(10),
            cancelled: true,
        };
        assert_eq!(ctx.check(), YieldDecision::Abort);
    }
}
