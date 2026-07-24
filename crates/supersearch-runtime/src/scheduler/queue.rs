//! Multi-Queue priority dispatcher.
//!
//! Five lock-free MPSC queues (one per [`PriorityClass`]), drained in strict
//! priority order with starvation-prevention aging. The Critical queue uses
//! `crossbeam_queue::ArrayQueue` (bounded, allocation-free after init) for
//! predictable latency; lower priorities use `SegQueue` (unbounded, lock-free).

use crossbeam_queue::{ArrayQueue, SegQueue};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio_util::sync::CancellationToken;
use tracing::{debug, instrument, warn};

use super::priority::PriorityClass;
use super::task::{TaskBuilder, TaskHandle};

/// Capacity of the Critical-priority bounded queue.
/// 64 slots is sufficient: critical input events arrive at human typing speed
/// (~150 WPM = ~12.5 chars/sec). Even at 1000 events/sec burst, 64 slots
/// provide ~64ms of buffering before backpressure.
const CRITICAL_QUEUE_CAPACITY: usize = 64;

/// Per-priority-class cancellation token, child of the global shutdown token.
#[derive(Debug)]
pub struct ClassTokens {
    pub critical: CancellationToken,
    pub interactive: CancellationToken,
    pub user_blocking: CancellationToken,
    pub background: CancellationToken,
    pub idle: CancellationToken,
}

impl ClassTokens {
    /// Create class-level tokens as children of the global shutdown token.
    pub fn new(global: &CancellationToken) -> Self {
        Self {
            critical: global.child_token(),
            interactive: global.child_token(),
            user_blocking: global.child_token(),
            background: global.child_token(),
            idle: global.child_token(),
        }
    }

    #[inline]
    pub fn for_class(&self, class: PriorityClass) -> &CancellationToken {
        match class {
            PriorityClass::Critical => &self.critical,
            PriorityClass::Interactive => &self.interactive,
            PriorityClass::UserBlocking => &self.user_blocking,
            PriorityClass::Background => &self.background,
            PriorityClass::Idle => &self.idle,
        }
    }
}

/// The multi-queue dispatcher.
///
/// Tasks are enqueued by priority class and dequeued in strict priority order.
/// The scheduler tick increments age counters and promotes starved tasks.
pub struct MultiQueue {
    /// Bounded, allocation-free queue for Critical tasks.
    /// `ArrayQueue::push` is wait-free; `pop` is lock-free.
    critical: ArrayQueue<TaskHandle>,
    /// Unbounded lock-free queues for non-critical priorities.
    interactive: SegQueue<TaskHandle>,
    user_blocking: SegQueue<TaskHandle>,
    background: SegQueue<TaskHandle>,
    idle: SegQueue<TaskHandle>,

    /// Promotion staging area: tasks promoted from lower queues are moved
    /// here to avoid re-scanning the entire queue. Protected by a Mutex
    /// because promotions are rare (starvation path only). FIFO (`VecDeque`)
    /// so the task that has been starved longest — the first one promoted —
    /// is also the first one serviced; a `Vec` used as a LIFO stack here
    /// would service the *most recently* promoted task first, inverting the
    /// fairness guarantee promotion exists to provide.
    promoted: Mutex<VecDeque<TaskHandle>>,

    /// Global tick counter for starvation aging.
    tick_counter: AtomicU64,

    /// Per-class cancellation tokens.
    pub class_tokens: ClassTokens,

    /// Global shutdown token (parent of all class tokens).
    pub shutdown_token: CancellationToken,
}

impl MultiQueue {
    pub fn new() -> Self {
        let shutdown_token = CancellationToken::new();
        let class_tokens = ClassTokens::new(&shutdown_token);
        Self {
            critical: ArrayQueue::new(CRITICAL_QUEUE_CAPACITY),
            interactive: SegQueue::new(),
            user_blocking: SegQueue::new(),
            background: SegQueue::new(),
            idle: SegQueue::new(),
            promoted: Mutex::new(VecDeque::with_capacity(16)),
            tick_counter: AtomicU64::new(0),
            class_tokens,
            shutdown_token,
        }
    }

    /// Enqueue a task into the appropriate priority queue.
    ///
    /// For Critical tasks, this uses the bounded `ArrayQueue`. If the queue
    /// is full (sustained input burst exceeding 64 events), the task is
    /// dropped and a warning is emitted — this is a backpressure signal
    /// that the consumer is not keeping up.
    #[instrument(skip(self, handle), fields(task_id = %handle.descriptor.id, priority = %handle.descriptor.priority))]
    pub fn enqueue(&self, handle: TaskHandle) {
        match handle.descriptor.priority {
            PriorityClass::Critical => {
                // ArrayQueue::push returns Err if full — backpressure signal.
                // < 4ms budget: we cannot block here, so we drop and warn.
                if self.critical.push(handle).is_err() {
                    warn!("Critical queue full — dropping task. Consumer is not keeping up.");
                }
            }
            PriorityClass::Interactive => self.interactive.push(handle),
            PriorityClass::UserBlocking => self.user_blocking.push(handle),
            PriorityClass::Background => self.background.push(handle),
            PriorityClass::Idle => self.idle.push(handle),
        }
    }

    /// Dequeue the highest-priority available task.
    ///
    /// Priority order: promoted tasks first (starved tasks that have been
    /// aged up), then Critical → Interactive → UserBlocking → Background → Idle.
    pub fn dequeue(&self) -> Option<TaskHandle> {
        // 1. Check promoted tasks first (starvation prevention takes priority).
        {
            let mut promoted = self.promoted.lock();
            if let Some(handle) = promoted.pop_front() {
                return Some(handle);
            }
        }
        // 2. Drain queues in priority order.
        // Critical: bounded ArrayQueue — pop is lock-free, ~10ns.
        if let Some(h) = self.critical.pop() {
            return Some(h);
        }
        // Interactive through Idle: SegQueue — pop is lock-free, ~15ns.
        if let Some(h) = self.interactive.pop() {
            return Some(h);
        }
        if let Some(h) = self.user_blocking.pop() {
            return Some(h);
        }
        if let Some(h) = self.background.pop() {
            return Some(h);
        }
        self.idle.pop()
    }

    /// Advance the global tick counter. Called once per scheduler loop iteration.
    ///
    /// This does NOT scan queues for starvation — that would require O(n)
    /// traversal of lock-free queues. Instead, aging is checked lazily when
    /// a task is dequeued and before it is polled.
    #[inline]
    pub fn tick(&self) -> u64 {
        self.tick_counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Current tick count (for age comparison in dequeued tasks).
    #[inline]
    pub fn current_tick(&self) -> u64 {
        self.tick_counter.load(Ordering::Relaxed)
    }

    /// Promote a task to the promoted staging area.
    /// Called by the executor when a dequeued task's `should_promote()` is true.
    pub fn promote(&self, mut handle: TaskHandle) {
        if let Some(new_priority) = handle.descriptor.priority.promote() {
            debug!(
                task_id = %handle.descriptor.id,
                from = %handle.descriptor.priority,
                to = %new_priority,
                "Promoting starved task"
            );
            handle.descriptor.priority = new_priority;
            handle.descriptor.age_ticks = 0; // Reset age after promotion
                                             // Recompute deadline with new priority's budget
            handle.descriptor.deadline_at =
                handle.descriptor.provenance.created_at + new_priority.latency_budget();
            self.promoted.lock().push_back(handle);
        }
    }

    /// Create a TaskBuilder pre-wired with the correct class cancellation token.
    #[inline]
    pub fn builder(&self, priority: PriorityClass) -> TaskBuilder<'_> {
        TaskBuilder::new(priority, self.class_tokens.for_class(priority))
    }

    /// Returns true if all queues are empty.
    pub fn is_empty(&self) -> bool {
        self.critical.is_empty()
            && self.interactive.is_empty()
            && self.user_blocking.is_empty()
            && self.background.is_empty()
            && self.idle.is_empty()
            && self.promoted.lock().is_empty()
    }

    /// Initiate graceful shutdown: cancel all class tokens.
    pub fn shutdown(&self) {
        self.shutdown_token.cancel();
    }
}

impl Default for MultiQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::priority::PriorityClass;

    #[test]
    fn promoted_tasks_are_serviced_fifo_oldest_first() {
        // Regression: the promotion staging area was a `Vec` used as a LIFO
        // stack (`push`/`pop`), so the *most recently* promoted task was
        // serviced first — inverting the fairness guarantee starvation
        // promotion exists to provide (the longest-starved task should run
        // first).
        let queue = MultiQueue::new();

        let a = queue
            .builder(PriorityClass::Idle)
            .origin("test")
            .label("a")
            .spawn(async {});
        let b = queue
            .builder(PriorityClass::Idle)
            .origin("test")
            .label("b")
            .spawn(async {});
        let a_id = a.descriptor.id;
        let b_id = b.descriptor.id;

        // `a` starves and gets promoted first, then `b`.
        queue.promote(a);
        queue.promote(b);

        let first = queue.dequeue().expect("first promoted task");
        let second = queue.dequeue().expect("second promoted task");
        assert_eq!(
            first.descriptor.id, a_id,
            "the longest-starved task must be serviced first"
        );
        assert_eq!(second.descriptor.id, b_id);
    }
}
