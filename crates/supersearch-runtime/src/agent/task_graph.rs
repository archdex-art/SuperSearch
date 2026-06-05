//! Task Graph IR — directed acyclic graph of executable actions.
//!
//! Workflows compile into `TaskGraph` rather than linear `Vec<Step>`.
//! This enables parallel execution, dependency resolution, retry policies,
//! rollback, and deterministic replay.

use serde::{Deserialize, Serialize};
use std::time::Duration;

// ─── Core Types ──────────────────────────────────────────────────────

/// A unique node identifier within a task graph.
pub type NodeId = usize;

/// A complete task graph ready for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGraph {
    /// Ordered list of task nodes.
    pub nodes: Vec<TaskNode>,
    /// Dependency edges (from → to).  `to` must complete before `from` starts.
    pub edges: Vec<TaskEdge>,
    /// Execution metadata.
    pub metadata: ExecutionMetadata,
}

/// A single executable node in the task graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    /// Unique ID within this graph.
    pub id: NodeId,
    /// Human-readable label.
    pub label: String,
    /// The action to execute.
    pub kind: TaskNodeKind,
    /// Current execution status.
    pub status: TaskStatus,
    /// Retry policy for this node.
    pub retry_policy: RetryPolicy,
    /// Maximum time allowed for this node.
    pub timeout: Option<Duration>,
    /// Result payload (set after execution).
    pub result: Option<String>,
    /// Error message (set on failure).
    pub error: Option<String>,
}

/// The kind of action a task node performs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskNodeKind {
    /// Launch an application.
    LaunchApp { app_name: String, args: Vec<String> },
    /// Open a file with the default handler.
    OpenFile { path: String },
    /// Open a URL in the default browser.
    OpenUrl { url: String },
    /// Search the web.
    WebSearch { query: String },
    /// Search for files.
    FindFiles { query: String },
    /// Read clipboard.
    ClipboardRead,
    /// Write to clipboard.
    ClipboardWrite { content: String },
    /// Execute a system command (AppleScript / shell).
    SystemCommand { script: String, label: String },
    /// Query system information.
    SystemInfo { command: String, label: String },
    /// List running applications.
    ListRunningApps,
    /// Quit an application.
    QuitApp { app_name: String },
    /// Focus/activate an application.
    SwitchApp { app_name: String },
    /// No-op placeholder (used in graph construction).
    Noop { reason: String },
}

/// Dependency edge: `prerequisite` must complete before `dependent` starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEdge {
    /// Node that must finish first.
    pub prerequisite: NodeId,
    /// Node that depends on the prerequisite.
    pub dependent: NodeId,
}

/// Execution lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Waiting for prerequisites.
    Pending,
    /// Currently executing.
    Running,
    /// Completed successfully.
    Completed,
    /// Failed (will retry if policy allows).
    Failed,
    /// Skipped (prerequisite failed, no retry).
    Skipped,
    /// Cancelled by user or system.
    Cancelled,
}

/// Retry policy for a task node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum retry attempts (0 = no retries).
    pub max_retries: u32,
    /// Current retry count.
    pub current_retries: u32,
}

/// Top-level metadata for a task graph execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    /// Unique execution ID.
    pub execution_id: String,
    /// Human-readable description of the workflow.
    pub description: String,
    /// Total number of nodes.
    pub total_steps: usize,
    /// Number of completed nodes.
    pub completed_steps: usize,
    /// Number of failed nodes.
    pub failed_steps: usize,
    /// Overall status.
    pub status: TaskStatus,
}

// ─── Constructors ────────────────────────────────────────────────────

impl TaskGraph {
    /// Create a new empty task graph.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            metadata: ExecutionMetadata {
                execution_id: generate_id(),
                description: description.into(),
                total_steps: 0,
                completed_steps: 0,
                failed_steps: 0,
                status: TaskStatus::Pending,
            },
        }
    }

    /// Add a node and return its ID.
    pub fn add_node(&mut self, label: impl Into<String>, kind: TaskNodeKind) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(TaskNode {
            id,
            label: label.into(),
            kind,
            status: TaskStatus::Pending,
            retry_policy: RetryPolicy::default(),
            timeout: Some(Duration::from_secs(10)),
            result: None,
            error: None,
        });
        self.metadata.total_steps = self.nodes.len();
        id
    }

    /// Add a dependency edge: `prerequisite` must finish before `dependent`.
    pub fn add_edge(&mut self, prerequisite: NodeId, dependent: NodeId) {
        self.edges.push(TaskEdge { prerequisite, dependent });
    }

    /// Get nodes that have no unfinished prerequisites (ready to execute).
    pub fn ready_nodes(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|node| {
                node.status == TaskStatus::Pending
                    && self.edges.iter().all(|edge| {
                        if edge.dependent == node.id {
                            self.nodes.get(edge.prerequisite)
                                .map(|n| n.status == TaskStatus::Completed)
                                .unwrap_or(true)
                        } else {
                            true
                        }
                    })
            })
            .map(|n| n.id)
            .collect()
    }

    /// Mark a node as completed.
    pub fn complete_node(&mut self, id: NodeId, result: String) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = TaskStatus::Completed;
            node.result = Some(result);
            self.metadata.completed_steps += 1;
        }
        self.update_overall_status();
    }

    /// Mark a node as failed.
    pub fn fail_node(&mut self, id: NodeId, error: String) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = TaskStatus::Failed;
            node.error = Some(error);
            self.metadata.failed_steps += 1;
        }
        self.update_overall_status();
    }

    /// Check if the entire graph is finished (all nodes completed or failed/skipped).
    pub fn is_finished(&self) -> bool {
        self.nodes.iter().all(|n| matches!(
            n.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Skipped | TaskStatus::Cancelled
        ))
    }

    /// Update overall metadata status.
    fn update_overall_status(&mut self) {
        if self.is_finished() {
            if self.metadata.failed_steps > 0 {
                self.metadata.status = TaskStatus::Failed;
            } else {
                self.metadata.status = TaskStatus::Completed;
            }
        } else if self.metadata.completed_steps > 0 || self.nodes.iter().any(|n| n.status == TaskStatus::Running) {
            self.metadata.status = TaskStatus::Running;
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self { max_retries: 0, current_retries: 0 }
    }
}

/// Generate a short unique execution ID.
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("exec-{:x}", ts & 0xFFFF_FFFF)
}
