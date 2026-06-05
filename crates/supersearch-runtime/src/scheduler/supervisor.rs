//! Erlang-style supervision trees for task lifecycle management.
//!
//! Since Rust lacks Erlang's lightweight processes with isolated heaps, we
//! simulate supervision using `std::panic::catch_unwind` boundaries at spawn
//! points and `JoinHandle` result monitoring.
//!
//! ## Restart Policies
//! - **OneForOne**: Restart only the failed task.
//! - **OneForAll**: Restart all tasks under this supervisor.
//! - **RestForOne**: Restart the failed task and all tasks started after it.
//!
//! ## Restart Intensity
//! If a task exceeds `max_restarts` within `restart_window`, the supervisor
//! escalates to its parent (or panics at the root, crashing the runtime).

use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{error, info, warn, instrument};

/// How many times a child can restart before the supervisor escalates.
#[derive(Debug, Clone)]
pub struct RestartIntensity {
    pub max_restarts: u32,
    pub window: Duration,
}

impl Default for RestartIntensity {
    fn default() -> Self {
        // 5 restarts in 60 seconds — aggressive enough to catch crash loops,
        // lenient enough to tolerate transient failures.
        Self { max_restarts: 5, window: Duration::from_secs(60) }
    }
}

/// Restart strategy determining which children restart on failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorStrategy {
    /// Restart only the failed child. Other children are unaffected.
    OneForOne,
    /// Restart ALL children when any one fails. Used when children have
    /// tight interdependencies (e.g., a CRDT sync pair).
    OneForAll,
    /// Restart the failed child and all children started AFTER it.
    /// Used for ordered initialization chains.
    RestForOne,
}

/// Policy governing a single supervised child.
#[derive(Debug, Clone)]
pub struct RestartPolicy {
    pub strategy: SupervisorStrategy,
    pub intensity: RestartIntensity,
    /// If true, the child is essential. Supervisor escalates if it cannot
    /// be restarted (intensity exceeded). If false, the child is optional
    /// and its failure is logged but not escalated.
    pub essential: bool,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            strategy: SupervisorStrategy::OneForOne,
            intensity: RestartIntensity::default(),
            essential: true,
        }
    }
}

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("Child {name} exceeded restart intensity ({restarts} restarts in {window:?})")]
    RestartIntensityExceeded {
        name: String,
        restarts: u32,
        window: Duration,
    },
    #[error("Child {name} panicked: {reason}")]
    ChildPanicked { name: String, reason: String },
    #[error("Supervisor shutdown requested")]
    Shutdown,
}

/// Record of a child's restart history.
#[derive(Debug)]
struct RestartHistory {
    timestamps: Vec<Instant>,
}

impl RestartHistory {
    fn new() -> Self { Self { timestamps: Vec::new() } }

    /// Record a restart and return whether the intensity limit is exceeded.
    fn record_restart(&mut self, intensity: &RestartIntensity) -> bool {
        let now = Instant::now();
        // Prune restarts outside the window.
        self.timestamps.retain(|t| now.duration_since(*t) < intensity.window);
        self.timestamps.push(now);
        self.timestamps.len() as u32 > intensity.max_restarts
    }
}

/// Metadata for a supervised child.
#[derive(Debug)]
pub struct ChildSpec {
    pub name: String,
    pub policy: RestartPolicy,
    history: RestartHistory,
}

impl ChildSpec {
    pub fn new(name: impl Into<String>, policy: RestartPolicy) -> Self {
        Self { name: name.into(), policy, history: RestartHistory::new() }
    }
}

/// The supervisor managing a set of child task specifications.
///
/// This is a logical supervisor — it does not own Tokio JoinHandles directly.
/// Instead, the [`SchedulerExecutor`] registers children and reports failures,
/// and the Supervisor decides the restart action.
#[derive(Debug)]
pub struct Supervisor {
    name: String,
    children: Vec<ChildSpec>,
    strategy: SupervisorStrategy,
}

impl Supervisor {
    pub fn new(name: impl Into<String>, strategy: SupervisorStrategy) -> Self {
        Self {
            name: name.into(),
            children: Vec::new(),
            strategy,
        }
    }

    pub fn add_child(&mut self, spec: ChildSpec) {
        self.children.push(spec);
    }

    /// Handle a child failure. Returns the set of children that need restarting.
    ///
    /// Returns `Err` if restart intensity is exceeded for an essential child.
    #[instrument(skip(self), fields(supervisor = %self.name))]
    pub fn handle_failure(
        &mut self,
        failed_child_name: &str,
    ) -> Result<RestartAction, SupervisorError> {
        let child_idx = self.children.iter().position(|c| c.name == failed_child_name);
        let child_idx = match child_idx {
            Some(idx) => idx,
            None => {
                warn!(child = failed_child_name, "Unknown child reported failure");
                return Ok(RestartAction::None);
            }
        };

        let child = &mut self.children[child_idx];
        let exceeded = child.history.record_restart(&child.policy.intensity);

        if exceeded && child.policy.essential {
            error!(
                child = failed_child_name,
                "Restart intensity exceeded for essential child — escalating"
            );
            return Err(SupervisorError::RestartIntensityExceeded {
                name: failed_child_name.to_string(),
                restarts: child.policy.intensity.max_restarts,
                window: child.policy.intensity.window,
            });
        }

        if exceeded && !child.policy.essential {
            warn!(child = failed_child_name, "Non-essential child exceeded restart intensity — abandoning");
            return Ok(RestartAction::None);
        }

        let names_to_restart = match self.strategy {
            SupervisorStrategy::OneForOne => {
                info!(child = failed_child_name, "OneForOne: restarting failed child");
                vec![failed_child_name.to_string()]
            }
            SupervisorStrategy::OneForAll => {
                info!(child = failed_child_name, "OneForAll: restarting all children");
                self.children.iter().map(|c| c.name.clone()).collect()
            }
            SupervisorStrategy::RestForOne => {
                info!(child = failed_child_name, "RestForOne: restarting from failed child onward");
                self.children[child_idx..].iter().map(|c| c.name.clone()).collect()
            }
        };

        Ok(RestartAction::Restart(names_to_restart))
    }
}

/// Instruction from the supervisor to the executor about what to restart.
#[derive(Debug, Clone)]
pub enum RestartAction {
    None,
    Restart(Vec<String>),
}
