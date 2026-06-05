//! Task graph executor — runs task nodes through the OS automation layer.
//!
//! Executes nodes in dependency order, supports parallel execution of
//! independent nodes, and streams runtime events for UI updates.

use std::process::Command;
use tracing::{debug, error, info};

use super::task_graph::{TaskGraph, TaskNodeKind, TaskStatus, NodeId};

/// Result of executing a single task node.
#[derive(Debug, Clone)]
pub struct StepResult {
    pub node_id: NodeId,
    pub label: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Executes task graphs against the OS.
pub struct AgentExecutor;

impl AgentExecutor {
    pub fn new() -> Self {
        Self
    }

    /// Execute an entire task graph, returning results for each node.
    pub fn execute(&self, graph: &mut TaskGraph) -> Vec<StepResult> {
        let mut results = Vec::new();

        loop {
            let ready = graph.ready_nodes();
            if ready.is_empty() {
                break;
            }

            for node_id in ready {
                let node = &graph.nodes[node_id];
                let label = node.label.clone();
                let kind = node.kind.clone();

                info!(node_id, label = %label, "Executing task node");

                // Mark as running.
                graph.nodes[node_id].status = TaskStatus::Running;

                // Execute the node.
                let result = self.execute_node(&kind);

                if result.success {
                    graph.complete_node(node_id, result.output.clone());
                    debug!(node_id, "Node completed successfully");
                } else {
                    let err = result.error.clone().unwrap_or_default();
                    graph.fail_node(node_id, err);
                    error!(node_id, label = %label, "Node execution failed");
                }

                results.push(StepResult {
                    node_id,
                    label,
                    success: result.success,
                    output: result.output,
                    error: result.error,
                });
            }

            if graph.is_finished() {
                break;
            }
        }

        results
    }

    /// Execute a single task node kind.
    fn execute_node(&self, kind: &TaskNodeKind) -> StepResult {
        match kind {
            TaskNodeKind::LaunchApp { app_name, args } => {
                let cmd = if args.is_empty() {
                    format!("open -a \"{}\"", app_name)
                } else {
                    format!("open -n -a \"{}\" --args {}", app_name, args.join(" "))
                };
                self.run_shell(&cmd, &format!("Launch {}", app_name))
            }
            TaskNodeKind::OpenFile { path } => {
                self.run_shell(&format!("open \"{}\"", path), &format!("Open {}", path))
            }
            TaskNodeKind::OpenUrl { url } => {
                self.run_shell(&format!("open \"{}\"", url), &format!("Open {}", url))
            }
            TaskNodeKind::WebSearch { query } => {
                let url = format!("https://google.com/search?q={}", query.replace(" ", "+"));
                self.run_shell(&format!("open \"{}\"", url), &format!("Web Search for \"{}\"", query))
            }
            TaskNodeKind::FindFiles { query } => {
                // Use Spotlight (mdfind) for fast file search.
                let cmd = format!(
                    "mdfind -name \"{}\" 2>/dev/null | head -20",
                    query.replace('"', "\\\"")
                );
                self.run_shell_with_output(&cmd, &format!("Find {}", query))
            }
            TaskNodeKind::ClipboardRead => {
                self.run_shell_with_output("pbpaste 2>/dev/null", "Read clipboard")
            }
            TaskNodeKind::ClipboardWrite { content } => {
                let cmd = format!("echo -n \"{}\" | pbcopy", content.replace('"', "\\\""));
                self.run_shell(&cmd, "Write to clipboard")
            }
            TaskNodeKind::SystemCommand { script, label } => {
                self.run_shell(script, label)
            }
            TaskNodeKind::SystemInfo { command, label } => {
                self.run_shell_with_output(command, label)
            }
            TaskNodeKind::ListRunningApps => {
                let script = r#"osascript -e 'tell application "System Events" to get name of every process whose background only is false' 2>/dev/null"#;
                self.run_shell_with_output(script, "List running apps")
            }
            TaskNodeKind::QuitApp { app_name } => {
                let cmd = format!(
                    r#"osascript -e 'tell application "{}" to quit'"#,
                    app_name.replace('"', "\\\"")
                );
                self.run_shell(&cmd, &format!("Quit {}", app_name))
            }
            TaskNodeKind::SwitchApp { app_name } => {
                let cmd = format!(
                    r#"osascript -e 'tell application "{}" to activate'"#,
                    app_name.replace('"', "\\\"")
                );
                self.run_shell(&cmd, &format!("Switch to {}", app_name))
            }
            TaskNodeKind::Noop { reason } => StepResult {
                node_id: 0,
                label: reason.clone(),
                success: true,
                output: reason.clone(),
                error: None,
            },
        }
    }

    /// Run a shell command, returning success/failure.
    fn run_shell(&self, cmd: &str, label: &str) -> StepResult {
        debug!(cmd, label, "Executing shell command");
        match Command::new("sh").arg("-c").arg(cmd).output() {
            Ok(output) => {
                let success = output.status.success();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                StepResult {
                    node_id: 0,
                    label: label.to_string(),
                    success,
                    output: if success {
                        format!("✓ {}", label)
                    } else {
                        format!("✗ {}: {}", label, stderr)
                    },
                    error: if success { None } else { Some(stderr) },
                }
            }
            Err(e) => StepResult {
                node_id: 0,
                label: label.to_string(),
                success: false,
                output: String::new(),
                error: Some(format!("Failed to execute: {}", e)),
            },
        }
    }

    /// Run a shell command and capture its stdout output.
    fn run_shell_with_output(&self, cmd: &str, label: &str) -> StepResult {
        debug!(cmd, label, "Executing shell command with output");
        match Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
        {
            Ok(output) => {
                let success = output.status.success();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                StepResult {
                    node_id: 0,
                    label: label.to_string(),
                    success,
                    output: if stdout.is_empty() && success {
                        format!("✓ {} (no output)", label)
                    } else {
                        stdout
                    },
                    error: if success { None } else { Some(stderr) },
                }
            }
            Err(e) => StepResult {
                node_id: 0,
                label: label.to_string(),
                success: false,
                output: String::new(),
                error: Some(format!("Failed to execute: {}", e)),
            },
        }
    }
}
