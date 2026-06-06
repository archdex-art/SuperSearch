//! Application state — thread-safe wrapper around the RuntimeKernel.
//!
//! Tauri manages this as shared state accessible from all command handlers.

use std::sync::Arc;
use std::time::Instant;

use supersearch_runtime::kernel::RuntimeKernel;
use supersearch_runtime::journal::writer::JournalSender;
use supersearch_runtime::scheduler::queue::MultiQueue;
use supersearch_runtime::capability::registry::CapabilityRegistry;
use supersearch_runtime::capability::gate::CapabilityGate;
use supersearch_runtime::agent::AgentController;

/// Shared application state managed by Tauri.
///
/// This is the single bridge between the Tauri IPC layer and the Rust kernel.
/// Command handlers receive `tauri::State<AppState>` and access kernel
/// subsystems through this struct.
pub struct AppState {
    /// The kernel's multi-queue scheduler (thread-safe, lock-free reads).
    pub queue: Arc<MultiQueue>,
    /// The capability registry (thread-safe via DashMap).
    pub registry: Arc<CapabilityRegistry>,
    /// The capability gate — mediates extension/agent OS access.
    pub gate: Arc<CapabilityGate>,
    /// Journal sender for emitting events (cloneable, thread-safe).
    pub journal_sender: JournalSender,
    /// When the kernel was booted (for uptime telemetry).
    pub boot_instant: Instant,
    /// Total active capabilities at boot (cached for fast telemetry).
    pub boot_time_ms: u64,
    /// The agentic AI controller (thread-safe).
    pub agent: Arc<AgentController>,
}

impl AppState {
    /// Create AppState from a booted RuntimeKernel.
    ///
    /// This extracts the thread-safe handles needed by command handlers
    /// while the kernel itself runs on a background Tokio task.
    pub fn from_kernel(kernel: &RuntimeKernel, boot_duration_ms: u64) -> Self {
        Self {
            queue: kernel.queue.clone(),
            registry: kernel.registry.clone(),
            gate: kernel.gate.clone(),
            journal_sender: kernel.journal_sender.clone(),
            boot_instant: Instant::now(),
            boot_time_ms: boot_duration_ms,
            agent: kernel.agent.clone(),
        }
    }
}
