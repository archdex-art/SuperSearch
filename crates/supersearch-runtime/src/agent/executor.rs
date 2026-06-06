//! Task graph executor — runs task nodes through the OS automation layer.
//!
//! Executes nodes in dependency order, supports parallel execution of
//! independent nodes, and streams runtime events for UI updates.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use super::task_graph::{TaskGraph, TaskNodeKind, TaskStatus, NodeId};
use crate::capability::gate::{CapabilityGate, GateDecision};
use crate::capability::namespace::Namespace;
use crate::capability::token::{CapabilityToken, Permission};
use crate::journal::entry::{EntryKind, JournalEntry};
use crate::journal::writer::JournalSender;

/// Hard upper bound on how long any single OS action may run before it is
/// killed. Keeps a hung helper process from wedging the synchronous IPC thread.
const ACTION_TIMEOUT: Duration = Duration::from_secs(15);

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
///
/// ## Capability mediation
/// Before any node runs, the executor maps it to a required
/// `(Namespace, Permission)` and asks the [`CapabilityGate`] whether the
/// agent's token authorizes it. A denied action never reaches the OS — no
/// process is spawned. Every decision (allow or deny) and every OS result is
/// appended to the journal for audit and deterministic replay.
pub struct AgentExecutor {
    /// The mediation point all privileged operations pass through.
    gate: Arc<CapabilityGate>,
    /// The agent's capability token, presented to the gate on every check.
    token: CapabilityToken,
    /// Audit/replay journal. `None` disables journaling (used in unit tests).
    journal: Option<JournalSender>,
    /// Monotonic clock origin for journal timestamps.
    boot: Instant,
}

impl AgentExecutor {
    /// Create an executor bound to a capability gate and the agent's token.
    pub fn new(
        gate: Arc<CapabilityGate>,
        token: CapabilityToken,
        journal: Option<JournalSender>,
    ) -> Self {
        Self { gate, token, journal, boot: Instant::now() }
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

                // Capability-mediated execution: authorize → (maybe) execute.
                let result = self.run_guarded(&kind, &label);

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

    /// Authorize a node against the capability gate, then execute it only if
    /// allowed. The gate decision and (on success) the OS result are journaled.
    fn run_guarded(&self, kind: &TaskNodeKind, label: &str) -> StepResult {
        if let Some((namespace, permission)) = Self::required_capability(kind) {
            let decision = self.gate.check(Some(&self.token), &namespace, permission);
            self.journal_decision(label, &namespace, permission, &decision);

            if let GateDecision::Denied { reason, .. } = &decision {
                warn!(label, %namespace, ?permission, %reason, "Action blocked by capability gate");
                return StepResult {
                    node_id: 0,
                    label: label.to_string(),
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "Capability denied: {} ({:?} on {})",
                        reason, permission, namespace
                    )),
                };
            }
        }

        let result = self.execute_node(kind);
        self.journal_result(label, &result);
        result
    }

    /// Map a task node to the `(Namespace, Permission)` it requires. `None`
    /// means the node is purely internal (e.g. `Noop`) and needs no grant.
    fn required_capability(kind: &TaskNodeKind) -> Option<(Namespace, Permission)> {
        let (ns, perm) = match kind {
            TaskNodeKind::LaunchApp { .. } => ("agent.process", Permission::ProcessSpawn),
            TaskNodeKind::QuitApp { .. } => ("agent.process", Permission::ProcessSignal),
            TaskNodeKind::SwitchApp { .. } => ("agent.window", Permission::WindowManipulate),
            TaskNodeKind::ListRunningApps => ("agent.process", Permission::ProcessInspect),
            TaskNodeKind::SystemInfo { .. } => ("agent.process", Permission::ProcessInspect),
            TaskNodeKind::SystemCommand { .. } => ("agent.input", Permission::InputSimulate),
            TaskNodeKind::OpenFile { .. } => ("agent.fs", Permission::FileRead),
            TaskNodeKind::FindFiles { .. } => ("agent.fs", Permission::DirectoryList),
            TaskNodeKind::OpenUrl { .. } | TaskNodeKind::WebSearch { .. } => {
                ("agent.network", Permission::NetworkConnect)
            }
            TaskNodeKind::ClipboardRead => ("agent.clipboard", Permission::ClipboardRead),
            TaskNodeKind::ClipboardWrite { .. } => ("agent.clipboard", Permission::ClipboardWrite),
            TaskNodeKind::Noop { .. } => return None,
        };
        Some((Namespace::new(ns), perm))
    }

    /// Append a capability-gate decision to the journal.
    fn journal_decision(
        &self,
        label: &str,
        namespace: &Namespace,
        permission: Permission,
        decision: &GateDecision,
    ) {
        let allowed = matches!(decision, GateDecision::Allowed { .. });
        let detail = match decision {
            GateDecision::Allowed { .. } => "allowed".to_string(),
            GateDecision::Denied { reason, .. } => format!("denied: {}", reason),
        };
        let payload = format!(
            "{{\"action\":\"{}\",\"namespace\":\"{}\",\"permission\":\"{:?}\",\"allowed\":{},\"detail\":\"{}\"}}",
            label, namespace, permission, allowed, detail
        );
        self.emit(EntryKind::CapabilityCheck, payload.into_bytes());
    }

    /// Append an OS-automation result to the journal.
    fn journal_result(&self, label: &str, result: &StepResult) {
        let payload = format!(
            "{{\"action\":\"{}\",\"success\":{}}}",
            label, result.success
        );
        self.emit(EntryKind::OsAutomationResult, payload.into_bytes());
    }

    /// Best-effort journal append. Journaling is non-fatal: a full or closed
    /// channel must never block or fail an action.
    fn emit(&self, kind: EntryKind, payload: Vec<u8>) {
        if let Some(journal) = &self.journal {
            let entry = JournalEntry::new(
                kind,
                self.boot.elapsed().as_nanos() as u64,
                "agent".into(),
                payload,
            );
            let _ = journal.send(entry);
        }
    }

    /// Execute a single task node kind.
    ///
    /// ## Security
    /// All nodes whose payloads contain user-derived data are executed by
    /// spawning the target binary directly with an argument vector — never by
    /// building a string and handing it to `sh -c`. This makes shell
    /// metacharacters (`;`, `|`, `$()`, backticks, quotes) inert, eliminating
    /// the command-injection surface. Only the trusted, constant scripts
    /// produced by the planner (`SystemCommand` / `SystemInfo` /
    /// `ListRunningApps`) are run through a shell, and those never interpolate
    /// user input.
    fn execute_node(&self, kind: &TaskNodeKind) -> StepResult {
        match kind {
            TaskNodeKind::LaunchApp { app_name, args } => {
                let label = format!("Launch {}", app_name);
                let argv: Vec<String> = if args.is_empty() {
                    vec!["-a".into(), app_name.clone()]
                } else {
                    let mut v = vec!["-n".into(), "-a".into(), app_name.clone(), "--args".into()];
                    v.extend(args.iter().cloned());
                    v
                };
                let arg_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
                self.run_argv("open", &arg_refs, &label)
            }
            TaskNodeKind::OpenFile { path } => {
                // `--` stops `open` from treating a leading-dash path as a flag.
                self.run_argv("open", &["--", path], &format!("Open {}", path))
            }
            TaskNodeKind::OpenUrl { url } => {
                self.run_argv("open", &["--", url], &format!("Open {}", url))
            }
            TaskNodeKind::WebSearch { query } => {
                let url = format!("https://www.google.com/search?q={}", percent_encode(query));
                self.run_argv("open", &["--", &url], &format!("Web Search for \"{}\"", query))
            }
            TaskNodeKind::FindFiles { query } => {
                // Spotlight search via argv (no shell); cap results in Rust.
                let mut r = self.run_argv_output("mdfind", &["-name", query], &format!("Find {}", query));
                if r.success {
                    let capped: String = r.output.lines().take(20).collect::<Vec<_>>().join("\n");
                    r.output = capped;
                }
                r
            }
            TaskNodeKind::ClipboardRead => {
                self.run_argv_output("pbpaste", &[], "Read clipboard")
            }
            TaskNodeKind::ClipboardWrite { content } => {
                self.run_stdin("pbcopy", &[], content, "Write to clipboard")
            }
            TaskNodeKind::SystemCommand { script, label } => {
                // Trusted, constant script generated by the planner.
                self.run_shell(script, label)
            }
            TaskNodeKind::SystemInfo { command, label } => {
                // Trusted, constant command generated by the planner.
                self.run_shell_with_output(command, label)
            }
            TaskNodeKind::ListRunningApps => {
                self.run_argv_output(
                    "osascript",
                    &["-e", "tell application \"System Events\" to get name of every process whose background only is false"],
                    "List running apps",
                )
            }
            TaskNodeKind::QuitApp { app_name } => {
                // App name passed as an AppleScript argv item — not interpolated
                // into the script source — so it cannot break out of the string.
                self.run_argv(
                    "osascript",
                    &["-e", "on run argv", "-e", "tell application (item 1 of argv) to quit", "-e", "end run", "--", app_name],
                    &format!("Quit {}", app_name),
                )
            }
            TaskNodeKind::SwitchApp { app_name } => {
                self.run_argv(
                    "osascript",
                    &["-e", "on run argv", "-e", "tell application (item 1 of argv) to activate", "-e", "end run", "--", app_name],
                    &format!("Switch to {}", app_name),
                )
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

    /// Spawn a program directly with an argument vector (no shell).
    fn run_argv(&self, program: &str, args: &[&str], label: &str) -> StepResult {
        debug!(program, ?args, label, "Executing argv command");
        let mut cmd = Command::new(program);
        cmd.args(args);
        Self::run_command(cmd, None, label, false)
    }

    /// Spawn a program directly and capture its stdout.
    fn run_argv_output(&self, program: &str, args: &[&str], label: &str) -> StepResult {
        debug!(program, ?args, label, "Executing argv command (capturing output)");
        let mut cmd = Command::new(program);
        cmd.args(args);
        Self::run_command(cmd, None, label, true)
    }

    /// Spawn a program directly, writing `input` to its stdin.
    fn run_stdin(&self, program: &str, args: &[&str], input: &str, label: &str) -> StepResult {
        debug!(program, label, "Executing argv command with stdin");
        let mut cmd = Command::new(program);
        cmd.args(args);
        Self::run_command(cmd, Some(input), label, false)
    }

    /// Run a trusted, constant shell command, returning success/failure.
    ///
    /// Only used for planner-generated constant scripts — never user input.
    fn run_shell(&self, cmd: &str, label: &str) -> StepResult {
        debug!(cmd, label, "Executing trusted shell command");
        let mut command = Command::new("sh");
        command.arg("-c").arg(cmd);
        Self::run_command(command, None, label, false)
    }

    /// Run a trusted, constant shell command and capture its stdout.
    fn run_shell_with_output(&self, cmd: &str, label: &str) -> StepResult {
        debug!(cmd, label, "Executing trusted shell command (capturing output)");
        let mut command = Command::new("sh");
        command.arg("-c").arg(cmd);
        Self::run_command(command, None, label, true)
    }

    /// Spawn `cmd`, optionally feeding `input` to its stdin, and wait for it to
    /// exit — but never longer than [`ACTION_TIMEOUT`]. A process that overruns
    /// the deadline is killed so a hung `osascript`/`open` can't wedge the
    /// caller (which, in the app, is the synchronous IPC thread).
    fn run_command(
        cmd: Command,
        input: Option<&str>,
        label: &str,
        capture: bool,
    ) -> StepResult {
        Self::run_command_with(cmd, input, label, capture, ACTION_TIMEOUT)
    }

    /// As [`run_command`], but with an explicit timeout (used by tests).
    fn run_command_with(
        mut cmd: Command,
        input: Option<&str>,
        label: &str,
        capture: bool,
        timeout: Duration,
    ) -> StepResult {
        cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return Self::err(label, e),
        };

        // Write stdin (if any) and close it so the child observes EOF.
        if let Some(data) = input {
            if let Some(mut stdin) = child.stdin.take() {
                if let Err(e) = stdin.write_all(data.as_bytes()) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Self::err(label, e);
                }
            }
        }

        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => return Self::finish(child.wait_with_output(), label, capture),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        error!(label, timeout_ms = timeout.as_millis() as u64, "Action timed out — killed");
                        return StepResult {
                            node_id: 0,
                            label: label.to_string(),
                            success: false,
                            output: String::new(),
                            error: Some(format!("Timed out after {:?}", timeout)),
                        };
                    }
                    std::thread::sleep(Duration::from_millis(15));
                }
                Err(e) => return Self::err(label, e),
            }
        }
    }

    /// Normalize a completed `Command::output()` into a `StepResult`.
    fn finish(
        output: std::io::Result<std::process::Output>,
        label: &str,
        capture: bool,
    ) -> StepResult {
        match output {
            Ok(output) => {
                let success = output.status.success();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let out = if capture {
                    if stdout.is_empty() && success {
                        format!("✓ {} (no output)", label)
                    } else {
                        stdout
                    }
                } else if success {
                    format!("✓ {}", label)
                } else {
                    format!("✗ {}: {}", label, stderr)
                };
                StepResult {
                    node_id: 0,
                    label: label.to_string(),
                    success,
                    output: out,
                    error: if success { None } else { Some(stderr) },
                }
            }
            Err(e) => Self::err(label, e),
        }
    }

    fn err(label: &str, e: impl std::fmt::Display) -> StepResult {
        StepResult {
            node_id: 0,
            label: label.to_string(),
            success: false,
            output: String::new(),
            error: Some(format!("Failed to execute: {}", e)),
        }
    }
}

/// Minimal percent-encoding for query strings (RFC 3986 unreserved set kept).
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::registry::CapabilityRegistry;

    /// Build an executor whose token grants exactly `perms` in the `agent`
    /// namespace — letting tests assert both the allow and deny paths.
    fn executor_with(perms: Vec<Permission>) -> AgentExecutor {
        let registry = Arc::new(CapabilityRegistry::new());
        let gate = Arc::new(CapabilityGate::new(registry.clone()));
        let token = registry.grant(
            Namespace::new("agent"),
            perms,
            "agent".into(),
            None,
            "test".into(),
        );
        AgentExecutor::new(gate, token, None)
    }

    #[test]
    fn percent_encode_neutralizes_shell_metacharacters() {
        // Characters an attacker would use to break out of a shell string are
        // all encoded, so they can never reach a shell even via the URL.
        let encoded = percent_encode("a\"; rm -rf ~ #");
        assert!(!encoded.contains('"'));
        assert!(!encoded.contains(';'));
        assert!(!encoded.contains(' '));
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(percent_encode("rust-lang_2024.~"), "rust-lang_2024.~");
    }

    #[test]
    fn long_running_action_is_killed_at_deadline() {
        // A process that outlives the deadline must be killed and reported as a
        // timeout rather than blocking the caller indefinitely.
        let start = Instant::now();
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let result = AgentExecutor::run_command_with(
            cmd,
            None,
            "sleep",
            false,
            Duration::from_millis(200),
        );
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("Timed out"));
        assert!(start.elapsed() < Duration::from_secs(5), "did not honor the deadline");
    }

    #[test]
    fn clipboard_write_roundtrips_untrusted_content() {
        // A payload full of shell metacharacters must be written verbatim and
        // must not execute. (pbcopy/pbpaste are macOS-only.)
        if cfg!(not(target_os = "macos")) {
            return;
        }
        let exec = executor_with(vec![Permission::ClipboardRead, Permission::ClipboardWrite]);
        let payload = "$(touch /tmp/supersearch_pwned); `id`; \"';";
        let write = exec.run_guarded(
            &TaskNodeKind::ClipboardWrite { content: payload.to_string() },
            "Write to clipboard",
        );
        assert!(write.success, "clipboard write failed: {:?}", write.error);

        let read = exec.run_guarded(&TaskNodeKind::ClipboardRead, "Read clipboard");
        assert!(read.success);
        assert_eq!(read.output, payload, "payload was altered or interpreted");
        assert!(
            !std::path::Path::new("/tmp/supersearch_pwned").exists(),
            "injection executed — sandbox breached"
        );
    }

    #[test]
    fn action_without_capability_is_blocked_before_touching_the_os() {
        // A token that lacks ClipboardWrite must not be able to run the write,
        // and the OS action must never fire (no clipboard mutation, no spawn).
        let exec = executor_with(vec![Permission::ClipboardRead]); // no write grant
        let sentinel = "supersearch-capability-denied-sentinel";
        let result = exec.run_guarded(
            &TaskNodeKind::ClipboardWrite { content: sentinel.to_string() },
            "Write to clipboard",
        );
        assert!(!result.success, "denied action unexpectedly succeeded");
        assert!(
            result.error.as_deref().unwrap_or_default().contains("Capability denied"),
            "expected a capability-denied error, got {:?}",
            result.error
        );

        // Prove the OS write never happened: the clipboard must not contain the
        // sentinel. (macOS-only assertion; the deny decision itself is
        // platform-independent and already checked above.)
        if cfg!(target_os = "macos") {
            let reader = executor_with(vec![Permission::ClipboardRead]);
            let read = reader.run_guarded(&TaskNodeKind::ClipboardRead, "Read clipboard");
            assert_ne!(read.output, sentinel, "blocked write still reached the OS");
        }
    }

    #[test]
    fn noop_requires_no_capability() {
        // A token with zero permissions can still run a Noop.
        let exec = executor_with(vec![]);
        let r = exec.run_guarded(&TaskNodeKind::Noop { reason: "nothing".into() }, "noop");
        assert!(r.success);
    }
}
