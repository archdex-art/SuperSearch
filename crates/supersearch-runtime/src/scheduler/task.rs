//! Task descriptors, handles, and cancellation tokens.
//!
//! ## Governance Isolation
//! This struct intentionally omits token budgets, inference ceilings, and API
//! rate limits. Those are governance concerns handled by middleware that wraps
//! the future *before* it reaches the scheduler.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use super::priority::PriorityClass;

/// Monotonically increasing task identifier. Lock-free atomic counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(pub(crate) u64);

impl TaskId {
    #[inline]
    pub fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        TaskId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
    #[inline]
    pub const fn raw(self) -> u64 { self.0 }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "task#{}", self.0)
    }
}

/// Hierarchical cancellation: Global → PriorityClass → Task.
/// Cancelling a class token cancels all tasks in that class.
#[derive(Debug, Clone)]
pub struct CancellationHandle {
    task_token: CancellationToken,
    /// Reserved: cancels every task in the class at once. Held now so handles
    /// carry the class link; not yet exercised by the scheduler loop.
    #[allow(dead_code)]
    class_token: CancellationToken,
}

impl CancellationHandle {
    #[inline]
    pub fn new(class_token: &CancellationToken) -> Self {
        Self {
            task_token: class_token.child_token(),
            class_token: class_token.clone(),
        }
    }
    /// Non-blocking check (~1ns atomic load on x86_64).
    #[inline]
    pub fn is_cancelled(&self) -> bool { self.task_token.is_cancelled() }
    #[inline]
    pub fn cancel(&self) { self.task_token.cancel(); }
    #[inline]
    pub async fn cancelled(&self) { self.task_token.cancelled().await; }
    #[inline]
    pub fn token(&self) -> &CancellationToken { &self.task_token }
}

/// Source provenance for event journal replay.
#[derive(Debug, Clone)]
pub struct TaskProvenance {
    pub origin: &'static str,
    pub label: &'static str,
    pub created_at: Instant,
}

/// Complete metadata envelope for a schedulable unit of work.
/// NO governance fields (token budgets, inference ceilings) — see constraint #6.
#[derive(Debug, Clone)]
pub struct TaskDescriptor {
    pub id: TaskId,
    pub priority: PriorityClass,
    /// Absolute deadline: `created_at + priority.latency_budget()`.
    pub deadline_at: Instant,
    pub poll_budget: u32,
    pub age_ticks: u64,
    pub cancellation: CancellationHandle,
    pub provenance: TaskProvenance,
    /// Critical tasks bypass the reactive dependency graph on initial frame.
    pub fast_path_bypass: bool,
}

impl TaskDescriptor {
    pub fn new(
        priority: PriorityClass,
        class_token: &CancellationToken,
        origin: &'static str,
        label: &'static str,
    ) -> Self {
        let now = Instant::now();
        Self {
            id: TaskId::next(),
            priority,
            deadline_at: now + priority.latency_budget(),
            poll_budget: priority.poll_budget(),
            age_ticks: 0,
            cancellation: CancellationHandle::new(class_token),
            provenance: TaskProvenance { origin, label, created_at: now },
            fast_path_bypass: matches!(priority, PriorityClass::Critical),
        }
    }

    /// Cost: ~25ns (`Instant::now()` via `clock_gettime` on x86_64).
    #[inline]
    pub fn is_overdue(&self) -> bool { Instant::now() >= self.deadline_at }

    #[inline]
    pub fn remaining_budget(&self) -> Duration {
        self.deadline_at.saturating_duration_since(Instant::now())
    }

    #[inline]
    pub fn should_promote(&self) -> bool {
        self.age_ticks >= self.priority.aging_threshold_ticks()
    }
}

/// Type-erased pinned future. Vtable dispatch (~2ns) is negligible vs 3.8ms budget.
pub type BoxFuture<T = ()> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// A complete schedulable unit: descriptor + the actual work.
pub struct TaskHandle {
    pub descriptor: TaskDescriptor,
    pub future: Option<BoxFuture>,
}

impl TaskHandle {
    pub fn new<F>(descriptor: TaskDescriptor, future: F) -> Self
    where F: Future<Output = ()> + Send + 'static,
    {
        Self { descriptor, future: Some(Box::pin(future)) }
    }
}

impl std::fmt::Debug for TaskHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskHandle")
            .field("descriptor", &self.descriptor)
            .field("has_future", &self.future.is_some())
            .finish()
    }
}

/// Ergonomic builder for constructing tasks.
pub struct TaskBuilder<'a> {
    priority: PriorityClass,
    class_token: &'a CancellationToken,
    origin: &'static str,
    label: &'static str,
    fast_path: Option<bool>,
    poll_budget_override: Option<u32>,
}

impl<'a> TaskBuilder<'a> {
    #[inline]
    pub fn new(priority: PriorityClass, class_token: &'a CancellationToken) -> Self {
        Self { priority, class_token, origin: "unknown", label: "unnamed", fast_path: None, poll_budget_override: None }
    }
    #[inline] pub fn origin(mut self, v: &'static str) -> Self { self.origin = v; self }
    #[inline] pub fn label(mut self, v: &'static str) -> Self { self.label = v; self }
    #[inline] pub fn fast_path(mut self, v: bool) -> Self { self.fast_path = Some(v); self }
    #[inline] pub fn poll_budget(mut self, v: u32) -> Self { self.poll_budget_override = Some(v); self }

    pub fn spawn<F>(self, future: F) -> TaskHandle
    where F: Future<Output = ()> + Send + 'static,
    {
        let mut desc = TaskDescriptor::new(self.priority, self.class_token, self.origin, self.label);
        if let Some(fp) = self.fast_path { desc.fast_path_bypass = fp; }
        if let Some(b) = self.poll_budget_override { desc.poll_budget = b; }
        TaskHandle::new(desc, future)
    }
}
