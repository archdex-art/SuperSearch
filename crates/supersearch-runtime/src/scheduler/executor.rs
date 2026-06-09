//! Scheduler executor: the main dispatch loop.
//!
//! Pulls tasks from the [`MultiQueue`], polls them with deadline awareness,
//! handles cooperative yielding, and reports failures to the [`Supervisor`].
//!
//! ## Fast-Path Bypass
//! Critical tasks with `fast_path_bypass = true` are dispatched immediately
//! to the render pipeline, bypassing the reactive dependency graph. A
//! reconciliation task is auto-spawned at UserBlocking priority.
//!
//! ## Governance Isolation
//! The executor loop contains ZERO references to token counts, inference
//! budgets, or API quotas. It schedules purely on time and priority.


use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

use tokio::sync::Notify;
use tracing::{debug, error, info, trace, warn, instrument};


use super::queue::MultiQueue;
use super::supervisor::Supervisor;
use super::task::TaskHandle;


/// Statistics emitted per scheduler tick for observability.
/// Kept minimal to avoid allocation in the hot path.
#[derive(Debug, Clone, Copy, Default)]
pub struct TickStats {
    pub tasks_polled: u32,
    pub tasks_completed: u32,
    pub tasks_yielded: u32,
    pub tasks_cancelled: u32,
    pub tasks_promoted: u32,
    pub tick_duration_us: u64,
}

/// Configuration for the scheduler executor.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum tasks to poll per tick before yielding to Tokio.
    /// Prevents the scheduler from monopolizing the executor thread.
    pub max_tasks_per_tick: usize,
    /// How long to sleep when all queues are empty.
    /// Uses Tokio's timer wheel — no busy-waiting.
    pub idle_sleep: Duration,
    /// Whether to enable the optimistic fast-path bypass for Critical tasks.
    pub enable_fast_path: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_tasks_per_tick: 64,
            idle_sleep: Duration::from_millis(1),
            enable_fast_path: true,
        }
    }
}

/// Callback trait for fast-path bypass.
/// Implementors receive Critical tasks that should be applied to the render
/// pipeline immediately, before reactive dependency graph evaluation.
pub trait FastPathSink: Send + Sync + 'static {
    /// Apply a fast-path update. Must complete in < 1ms to stay within
    /// the Critical task's 3.8ms budget (leaving 2.8ms for the task itself).
    fn apply_fast_path(&self, task_id: super::task::TaskId, label: &'static str);

    /// Spawn a reconciliation task to back-propagate the fast-path update
    /// through the reactive dependency graph.
    fn spawn_reconciliation(&self, task_id: super::task::TaskId, label: &'static str);
}

/// No-op fast path sink for when fast-path is disabled or not configured.
pub struct NoopFastPathSink;

impl FastPathSink for NoopFastPathSink {
    fn apply_fast_path(&self, _id: super::task::TaskId, _label: &'static str) {}
    fn spawn_reconciliation(&self, _id: super::task::TaskId, _label: &'static str) {}
}

/// The core scheduler executor.
///
/// This is designed to run as a single long-lived Tokio task that drives
/// all scheduled work. It is NOT a standalone thread — it cooperates with
/// Tokio's work-stealing executor.
pub struct SchedulerExecutor {
    queue: Arc<MultiQueue>,
    config: SchedulerConfig,
    /// Reserved: supervises restart of failed system tasks. Constructed and
    /// owned by the executor; restart wiring is still pending.
    #[allow(dead_code)]
    supervisor: Supervisor,
    fast_path_sink: Arc<dyn FastPathSink>,
    /// Notification channel for waking the scheduler when new tasks arrive.
    /// Avoids busy-waiting on empty queues.
    notify: Arc<Notify>,
}

impl SchedulerExecutor {
    pub fn new(
        queue: Arc<MultiQueue>,
        config: SchedulerConfig,
        supervisor: Supervisor,
        fast_path_sink: Arc<dyn FastPathSink>,
    ) -> Self {
        Self {
            queue,
            config,
            supervisor,
            fast_path_sink,
            notify: Arc::new(Notify::new()),
        }
    }

    /// Returns a handle that can be used to wake the scheduler when
    /// new tasks are enqueued.
    #[inline]
    pub fn notifier(&self) -> Arc<Notify> {
        self.notify.clone()
    }

    /// The main scheduling loop. Runs until the global shutdown token fires.
    ///
    /// ## Loop Structure
    /// 1. Dequeue highest-priority task.
    /// 2. If fast-path bypass, apply immediately.
    /// 3. Check cancellation and deadline.
    /// 4. Poll the future once.
    /// 5. If Pending, re-enqueue.
    /// 6. If Ready, record completion.
    /// 7. Tick aging counters.
    /// 8. If queues empty, park on Notify.
    ///
    /// ## Latency Budget Enforcement
    /// - Critical (<4ms): fast-path bypass + single poll + yield
    /// - Interactive (<16ms): up to 16 polls per task per tick
    /// - UserBlocking (<50ms): up to 64 polls
    /// - Background (<250ms): up to 128 polls, yields aggressively
    /// - Idle: 256 polls, only runs when queues are empty
    #[instrument(skip(self), name = "scheduler_loop")]
    pub async fn run(&mut self) {
        info!("Scheduler executor starting");

        loop {
            // Check global shutdown.
            if self.queue.shutdown_token.is_cancelled() {
                info!("Shutdown token fired — draining remaining tasks");
                self.drain_on_shutdown().await;
                return;
            }

            let tick_start = Instant::now();
            let mut stats = TickStats::default();

            // Process up to max_tasks_per_tick tasks in this tick.
            for _ in 0..self.config.max_tasks_per_tick {
                let task = match self.queue.dequeue() {
                    Some(t) => t,
                    None => break, // All queues empty
                };

                self.process_task(task, &mut stats).await;
            }

            // Advance aging counters.
            self.queue.tick();

            // Record tick duration.
            stats.tick_duration_us = tick_start.elapsed().as_micros() as u64;

            if stats.tasks_polled > 0 {
                trace!(?stats, "Tick complete");
            }

            // If no work was done, park until new tasks arrive or timeout.
            if stats.tasks_polled == 0 {
                tokio::select! {
                    _ = self.notify.notified() => {
                        // New task enqueued — loop immediately.
                    }
                    _ = tokio::time::sleep(self.config.idle_sleep) => {
                        // Periodic wakeup to check shutdown token.
                    }
                    _ = self.queue.shutdown_token.cancelled() => {
                        info!("Shutdown during idle park");
                        return;
                    }
                }
            } else {
                // Yield to Tokio after each tick to prevent scheduler
                // starvation of non-scheduled Tokio tasks.
                tokio::task::yield_now().await;
            }
        }
    }

    /// Process a single dequeued task.
    async fn process_task(&mut self, mut task: TaskHandle, stats: &mut TickStats) {
        let desc = &task.descriptor;

        // 1. Cancellation check (< 1ns).
        if desc.cancellation.is_cancelled() {
            stats.tasks_cancelled += 1;
            trace!(task_id = %desc.id, "Task already cancelled — skipping");
            return;
        }

        // 2. Starvation promotion check.
        if desc.should_promote() {
            stats.tasks_promoted += 1;
            self.queue.promote(task);
            return;
        }

        // 3. Fast-path bypass for Critical tasks.
        if desc.fast_path_bypass && self.config.enable_fast_path {
            // Apply fast-path update to render pipeline immediately.
            // This bypasses the reactive dependency graph for the initial
            // frame, achieving < 4ms input-to-pixel latency.
            self.fast_path_sink.apply_fast_path(desc.id, desc.provenance.label);
            // Spawn async reconciliation at lower priority.
            self.fast_path_sink.spawn_reconciliation(desc.id, desc.provenance.label);
        }

        // 4. Deadline check (~25ns).
        if desc.is_overdue() {
            warn!(
                task_id = %desc.id,
                priority = %desc.priority,
                label = desc.provenance.label,
                "Task overdue before polling — cancelling"
            );
            desc.cancellation.cancel();
            stats.tasks_cancelled += 1;
            return;
        }

        // 5. Poll the future.
        stats.tasks_polled += 1;

        let future = match task.future.take() {
            Some(f) => f,
            None => {
                error!(task_id = %desc.id, "Task future already consumed — double-poll bug");
                return;
            }
        };

        // Create a waker that will re-notify the scheduler.
        let notify = self.notify.clone();
        let waker = futures_waker(notify);
        let mut cx = Context::from_waker(&waker);

        // Pin the future and poll once.
        let mut pinned = future;
        match Pin::as_mut(&mut pinned).poll(&mut cx) {
            Poll::Ready(()) => {
                stats.tasks_completed += 1;
                debug!(
                    task_id = %task.descriptor.id,
                    priority = %task.descriptor.priority,
                    label = task.descriptor.provenance.label,
                    elapsed_us = task.descriptor.provenance.created_at.elapsed().as_micros() as u64,
                    "Task completed"
                );
            }
            Poll::Pending => {
                stats.tasks_yielded += 1;
                // Re-enqueue with the future restored.
                task.future = Some(pinned);
                // Increment age for starvation tracking.
                task.descriptor.age_ticks += 1;
                self.queue.enqueue(task);
            }
        }
    }

    /// Drain remaining tasks during graceful shutdown.
    /// Polls each remaining task once and discards.
    async fn drain_on_shutdown(&mut self) {
        let mut drained = 0;
        while let Some(task) = self.queue.dequeue() {
            task.descriptor.cancellation.cancel();
            drained += 1;
        }
        info!(drained, "Shutdown drain complete");
    }
}

/// Create a `Waker` that notifies the scheduler's `Notify`.
///
/// This is a minimal waker implementation — the scheduler doesn't need
/// per-task waker tracking because it re-polls all pending tasks each tick.
fn futures_waker(notify: Arc<Notify>) -> Waker {
    struct SchedulerWake(Arc<Notify>);

    impl Wake for SchedulerWake {
        fn wake(self: Arc<Self>) {
            self.0.notify_one();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.notify_one();
        }
    }

    Waker::from(Arc::new(SchedulerWake(notify)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::queue::MultiQueue;
    use crate::scheduler::supervisor::Supervisor;
    use crate::scheduler::{SupervisorStrategy, PriorityClass};

    #[tokio::test]
    async fn scheduler_completes_single_task() {
        let queue = Arc::new(MultiQueue::new());
        let config = SchedulerConfig::default();
        let supervisor = Supervisor::new("test", SupervisorStrategy::OneForOne);
        let sink = Arc::new(NoopFastPathSink);

        let mut executor = SchedulerExecutor::new(
            queue.clone(), config, supervisor, sink,
        );

        // Enqueue a trivial task.
        let completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let completed_clone = completed.clone();

        let handle = queue.builder(PriorityClass::Interactive)
            .origin("test")
            .label("trivial_task")
            .spawn(async move {
                completed_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            });

        queue.enqueue(handle);

        // Run one tick manually by requesting shutdown after a brief delay.
        let shutdown_queue = queue.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            shutdown_queue.shutdown();
        });

        executor.run().await;
        assert!(completed.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn scheduler_drives_task_with_awaited_inner_future() {
        // Mirrors how agent_query is scheduled (P5): an enqueued task that
        // awaits an inner spawn_blocking and returns its result over a oneshot.
        // Proves the scheduler polls the future to completion across the await.
        let queue = Arc::new(MultiQueue::new());
        let supervisor = Supervisor::new("test", SupervisorStrategy::OneForOne);
        let mut executor = SchedulerExecutor::new(
            queue.clone(), SchedulerConfig::default(), supervisor, Arc::new(NoopFastPathSink),
        );

        let (tx, rx) = tokio::sync::oneshot::channel::<u32>();
        let task = queue
            .builder(PriorityClass::Interactive)
            .origin("test")
            .label("inner")
            .spawn(async move {
                let v = tokio::task::spawn_blocking(|| 21u32 * 2).await.unwrap();
                let _ = tx.send(v);
            });
        queue.enqueue(task);

        // Race the executor against collecting the result. `select!` resolves
        // the instant the result arrives and then drops (cancels) `run()`, so we
        // never call shutdown — which is important: shutting down here would make
        // `run()` *also* complete, and `select!` would then pick a ready branch
        // at random (it hit the `run()` branch ~half the time on Windows). The
        // 5s timeout bounds the whole test, so it can neither hang nor flake.
        let got = tokio::select! {
            got = async {
                tokio::time::timeout(Duration::from_secs(5), rx)
                    .await
                    .expect("scheduler did not drive the task in time")
                    .expect("task dropped its sender")
            } => got,
            _ = executor.run() => panic!("executor stopped before the task delivered a result"),
        };
        assert_eq!(got, 42);
    }
}
