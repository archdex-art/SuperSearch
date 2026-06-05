//! The top-level runtime kernel — orchestrator of all subsystems.
//!
//! The `RuntimeKernel` is the entry point that wires together:
//! - Scheduler (Module 1): CPU time-slicing
//! - Event Journal (Module 2): Deterministic replay
//! - Capability System (Module 3): Security mediation
//! - Reactive Graph (Module 4): State management
//! - Plugin Runtime (Module 5): Sandboxed plugins
//! - OS Automation (Module 6): Privileged operations
//!
//! ## Boot Sequence
//! 1. Initialize capability registry with kernel-level secret key.
//! 2. Create scheduler with multi-queue and supervisor.
//! 3. Start journal writer at Background priority.
//! 4. Initialize reactive dependency graph.
//! 5. Initialize plugin host.
//! 6. Enter main scheduler loop.

use std::sync::Arc;
use tracing::{info, error};

use crate::scheduler::queue::MultiQueue;
use crate::scheduler::executor::{SchedulerExecutor, SchedulerConfig, NoopFastPathSink};
use crate::scheduler::supervisor::{Supervisor, SupervisorStrategy, ChildSpec, RestartPolicy};
use crate::capability::registry::CapabilityRegistry;
use crate::capability::gate::CapabilityGate;
use crate::journal::writer::{JournalWriter, JournalSender};
use crate::reactive::graph::DependencyGraph;
use crate::reactive::reconcile::ReconciliationEngine;
use crate::plugin::host::PluginHost;
use crate::agent::AgentController;
use super::automation::OsAutomation;
use super::process::ProcessManager;

/// Configuration for the runtime kernel.
#[derive(Debug, Clone)]
pub struct KernelConfig {
    /// Scheduler configuration.
    pub scheduler: SchedulerConfig,
    /// Journal directory path.
    pub journal_dir: String,
    /// Journal channel buffer size.
    pub journal_channel_size: usize,
    /// Maximum managed processes.
    pub max_processes: usize,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            scheduler: SchedulerConfig::default(),
            journal_dir: "./data/journal".into(),
            journal_channel_size: 4096,
            max_processes: 64,
        }
    }
}

/// The runtime kernel — top-level orchestrator.
///
/// ## Ownership Model
/// ```text
/// RuntimeKernel
///   ├── scheduler_queue: Arc<MultiQueue>       (shared with executor)
///   ├── scheduler_executor: SchedulerExecutor  (owned, runs the loop)
///   ├── capability_registry: Arc<Registry>     (shared with gate + host)
///   ├── capability_gate: Arc<Gate>             (shared with automation + IPC)
///   ├── journal_sender: JournalSender          (cloneable, shared with all)
///   ├── dependency_graph: DependencyGraph       (owned)
///   ├── reconciliation: ReconciliationEngine    (owned)
///   ├── plugin_host: PluginHost                 (owned)
///   ├── os_automation: OsAutomation             (owned)
///   └── process_manager: ProcessManager         (owned)
/// ```
pub struct RuntimeKernel {
    pub queue: Arc<MultiQueue>,
    pub registry: Arc<CapabilityRegistry>,
    pub gate: Arc<CapabilityGate>,
    pub journal_sender: JournalSender,
    pub dependency_graph: DependencyGraph,
    pub reconciliation: ReconciliationEngine,
    pub plugin_host: PluginHost,
    pub os_automation: OsAutomation,
    pub process_manager: ProcessManager,
    /// The agentic AI controller (thread-safe, lock-free reads).
    pub agent: Arc<AgentController>,

    /// The journal writer handle (runs as a Background task).
    journal_writer: Option<JournalWriter>,
    /// The supervisor for top-level system tasks.
    supervisor: Supervisor,
    /// Scheduler configuration (needed to construct the executor).
    scheduler_config: SchedulerConfig,
}

impl RuntimeKernel {
    /// Bootstrap the runtime kernel.
    ///
    /// This creates all subsystems but does NOT start the scheduler loop.
    /// Call `run()` to enter the main loop.
    pub fn boot(config: KernelConfig) -> Self {
        info!("Booting SuperSearch Runtime Kernel");

        // 1. Capability system.
        let registry = Arc::new(CapabilityRegistry::new());
        let gate = Arc::new(CapabilityGate::new(registry.clone()));
        info!("Capability system initialized");

        // 2. Scheduler.
        let queue = Arc::new(MultiQueue::new());
        let mut supervisor = Supervisor::new("root", SupervisorStrategy::OneForOne);

        // Register system-level supervised children.
        supervisor.add_child(ChildSpec::new("journal_writer", RestartPolicy::default()));
        supervisor.add_child(ChildSpec::new("plugin_host", RestartPolicy::default()));
        supervisor.add_child(ChildSpec::new("scheduler_loop", RestartPolicy::default()));
        info!("Scheduler and supervisor initialized");

        // 3. Event Journal.
        let (journal_writer, journal_sender) = JournalWriter::new(
            &config.journal_dir,
            config.journal_channel_size,
        );
        info!(dir = %config.journal_dir, "Journal writer created");

        // 4. Reactive graph.
        let dependency_graph = DependencyGraph::new();
        let reconciliation = ReconciliationEngine::new();
        info!("Reactive dependency graph initialized");

        // 5. Plugin host.
        let plugin_host = PluginHost::new(registry.clone(), gate.clone());
        info!("Plugin host initialized");

        // 6. OS automation.
        let os_automation = OsAutomation::new(gate.clone());
        let process_manager = ProcessManager::new(config.max_processes);
        info!("OS automation and process manager initialized");

        // 7. Agent controller.
        let agent = Arc::new(AgentController::new());
        info!("Agent controller initialized");

        info!("Runtime kernel boot complete — ready to run");

        Self {
            queue,
            registry,
            gate,
            journal_sender,
            dependency_graph,
            reconciliation,
            plugin_host,
            os_automation,
            process_manager,
            agent,
            journal_writer: Some(journal_writer),
            supervisor,
            scheduler_config: config.scheduler,
        }
    }

    /// Run the kernel — starts the journal writer and scheduler executor.
    ///
    /// This method takes ownership and runs until shutdown is requested.
    pub async fn run(mut self) {
        info!("Starting runtime kernel main loop");

        // Start the journal writer as a background Tokio task.
        let mut journal_writer = self.journal_writer.take()
            .expect("Journal writer already taken");
        let journal_handle = tokio::spawn(async move {
            if let Err(e) = journal_writer.run().await {
                error!(error = %e, "Journal writer failed");
            }
        });

        // Create and run the scheduler executor.
        let fast_path_sink = Arc::new(NoopFastPathSink);
        let mut executor = SchedulerExecutor::new(
            self.queue.clone(),
            self.scheduler_config.clone(),
            self.supervisor,
            fast_path_sink,
        );

        // Run the scheduler loop (blocks until shutdown).
        executor.run().await;

        // Shutdown: wait for journal to flush.
        info!("Scheduler loop exited — waiting for journal flush");
        let _ = journal_handle.await;

        info!("Runtime kernel shutdown complete");
    }

    /// Request graceful shutdown.
    pub fn shutdown(&self) {
        info!("Shutdown requested");
        self.queue.shutdown();
    }
}
