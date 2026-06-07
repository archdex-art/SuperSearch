//! Task graph executor — runs task nodes through the OS automation layer.
//!
//! Executes nodes in dependency order, supports parallel execution of
//! independent nodes, and streams runtime events for UI updates.
//!
//! ## Platform isolation
//! The executor owns capability mediation, journaling, and graph traversal —
//! none of which is OS-specific. Every actual OS call is delegated to a
//! [`PlatformBackend`](crate::platform::PlatformBackend) selected for the
//! compile target, so this module contains no `open`/`osascript`/`mdfind`/Win32
//! references and no `cfg(target_os)` branching. Swapping or adding an OS means
//! writing a backend, not touching the executor.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use super::task_graph::{TaskGraph, TaskNodeKind, TaskStatus, NodeId};
use crate::capability::gate::{CapabilityGate, GateDecision};
use crate::capability::namespace::Namespace;
use crate::capability::token::{CapabilityToken, Permission};
use crate::journal::entry::{EntryKind, JournalEntry};
use crate::journal::writer::JournalSender;
use crate::platform::{default_backend, PlatformBackend};

pub use crate::platform::StepResult;

/// Hard upper bound on how long any single OS action may run before it is
/// killed. Keeps a hung helper process from wedging the synchronous IPC thread.
const ACTION_TIMEOUT: Duration = Duration::from_secs(15);

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
    /// The OS automation backend every action is dispatched to.
    backend: Arc<dyn PlatformBackend>,
    /// Monotonic clock origin for journal timestamps.
    boot: Instant,
}

impl AgentExecutor {
    /// Create an executor bound to a capability gate and the agent's token,
    /// using the platform backend selected for this build target.
    pub fn new(
        gate: Arc<CapabilityGate>,
        token: CapabilityToken,
        journal: Option<JournalSender>,
    ) -> Self {
        Self::with_backend(gate, token, journal, default_backend())
    }

    /// Create an executor with an explicit platform backend. Used by tests to
    /// inject a deterministic or fake backend; production uses [`new`].
    pub fn with_backend(
        gate: Arc<CapabilityGate>,
        token: CapabilityToken,
        journal: Option<JournalSender>,
        backend: Arc<dyn PlatformBackend>,
    ) -> Self {
        Self { gate, token, journal, backend, boot: Instant::now() }
    }

    /// Execute an entire task graph, returning results for each node.
    ///
    /// Honors the per-node IR: each node's [`timeout`](TaskNode::timeout) bounds
    /// its OS call, [`retry_policy`](TaskNode::retry_policy) re-runs transient
    /// failures, and when a node ultimately fails, its (transitive) dependents
    /// are marked `Skipped` and reported — so multi-step results are never
    /// silently truncated.
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
                let timeout = node.timeout.unwrap_or(ACTION_TIMEOUT);
                let max_retries = node.retry_policy.max_retries;

                info!(node_id, label = %label, "Executing task node");
                graph.nodes[node_id].status = TaskStatus::Running;

                // Capability-mediated execution, retrying transient failures.
                let mut attempts = 0;
                let mut result = self.run_guarded(&kind, &label, timeout);
                while !result.success && attempts < max_retries {
                    attempts += 1;
                    warn!(node_id, attempt = attempts, max_retries, label = %label, "Retrying node");
                    result = self.run_guarded(&kind, &label, timeout);
                }
                graph.nodes[node_id].retry_policy.current_retries = attempts;

                let success = result.success;
                if success {
                    graph.complete_node(node_id, result.output.clone());
                    debug!(node_id, "Node completed successfully");
                } else {
                    graph.fail_node(node_id, result.error.clone().unwrap_or_default());
                    error!(node_id, label = %label, attempts, "Node execution failed");
                }

                results.push(StepResult {
                    node_id,
                    label: label.clone(),
                    success,
                    output: result.output,
                    error: result.error,
                });

                // A failed node's dependents can never run — skip and report.
                if !success {
                    for skipped_id in self.cascade_skip(graph, node_id) {
                        results.push(StepResult {
                            node_id: skipped_id,
                            label: graph.nodes[skipped_id].label.clone(),
                            success: false,
                            output: String::new(),
                            error: Some(format!("Skipped — prerequisite '{}' failed", label)),
                        });
                    }
                }
            }

            if graph.is_finished() {
                break;
            }
        }

        results
    }

    /// Mark every (transitive) dependent of a failed node as `Skipped`,
    /// returning their ids in discovery order.
    fn cascade_skip(&self, graph: &mut TaskGraph, failed_id: NodeId) -> Vec<NodeId> {
        let edges: Vec<(NodeId, NodeId)> = graph
            .edges
            .iter()
            .map(|e| (e.prerequisite, e.dependent))
            .collect();

        let mut skipped = Vec::new();
        let mut stack = vec![failed_id];
        while let Some(cur) = stack.pop() {
            for &(prereq, dep) in &edges {
                let pending = graph
                    .nodes
                    .get(dep)
                    .map(|n| n.status == TaskStatus::Pending)
                    .unwrap_or(false);
                if prereq == cur && pending {
                    graph.skip_node(dep);
                    skipped.push(dep);
                    stack.push(dep);
                }
            }
        }
        skipped
    }

    /// Authorize a node against the capability gate, then execute it only if
    /// allowed. The gate decision and (on success) the OS result are journaled.
    fn run_guarded(&self, kind: &TaskNodeKind, label: &str, timeout: Duration) -> StepResult {
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

        let result = self.execute_node(kind, timeout);
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

    /// Translate a task node into a call on the platform backend.
    ///
    /// This method is intentionally OS-agnostic: it computes presentation labels
    /// and the small amount of platform-independent policy (the search-result
    /// URL, capping Spotlight output) and hands every actual OS primitive to the
    /// [`PlatformBackend`]. All security properties of the underlying spawn
    /// (argv-only execution of user data, trusted-script isolation, per-action
    /// timeout) live in the backend and the shared
    /// [`exec`](crate::platform::exec) engine.
    fn execute_node(&self, kind: &TaskNodeKind, timeout: Duration) -> StepResult {
        let backend = self.backend.as_ref();
        match kind {
            TaskNodeKind::LaunchApp { app_name, args } => {
                backend.launch_app(app_name, args, &format!("Launch {}", app_name), timeout)
            }
            TaskNodeKind::OpenFile { path } => {
                backend.open_path(path, &format!("Open {}", path), timeout)
            }
            TaskNodeKind::OpenUrl { url } => {
                backend.open_url(url, &format!("Open {}", url), timeout)
            }
            TaskNodeKind::WebSearch { query } => {
                let url = format!("https://www.google.com/search?q={}", percent_encode(query));
                backend.open_url(&url, &format!("Web Search for \"{}\"", query), timeout)
            }
            TaskNodeKind::FindFiles { query } => {
                let mut r = backend.find_files(query, &format!("Find {}", query), timeout);
                if r.success {
                    // Cap result volume in Rust — platform-independent policy.
                    r.output = r.output.lines().take(20).collect::<Vec<_>>().join("\n");
                }
                r
            }
            TaskNodeKind::ClipboardRead => backend.clipboard_read("Read clipboard", timeout),
            TaskNodeKind::ClipboardWrite { content } => {
                backend.clipboard_write(content, "Write to clipboard", timeout)
            }
            TaskNodeKind::SystemCommand { script, label } => {
                // Trusted, constant script generated by the planner.
                backend.run_trusted_script(script, label, false, timeout)
            }
            TaskNodeKind::SystemInfo { command, label } => {
                // Trusted, constant command generated by the planner.
                backend.run_trusted_script(command, label, true, timeout)
            }
            TaskNodeKind::ListRunningApps => backend.list_running_apps("List running apps", timeout),
            TaskNodeKind::QuitApp { app_name } => {
                backend.quit_app(app_name, &format!("Quit {}", app_name), timeout)
            }
            TaskNodeKind::SwitchApp { app_name } => {
                backend.switch_app(app_name, &format!("Switch to {}", app_name), timeout)
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
    fn failed_prerequisite_skips_and_reports_dependents() {
        // A two-node sequential graph where the first node fails: the second
        // must be reported as Skipped, not silently dropped.
        let exec = executor_with(vec![Permission::ClipboardRead]); // lacks ClipboardWrite
        let mut graph = TaskGraph::new("multi");
        // Node 0 will be DENIED (no ClipboardWrite grant) → fails.
        let n0 = graph.add_node("write", TaskNodeKind::ClipboardWrite { content: "x".into() });
        let n1 = graph.add_node("read", TaskNodeKind::ClipboardRead);
        graph.add_edge(n0, n1); // n1 depends on n0

        let results = exec.execute(&mut graph);
        assert_eq!(results.len(), 2, "both nodes should be reported");
        assert!(!results[0].success);
        assert!(!results[1].success);
        assert!(results[1].error.as_deref().unwrap_or_default().contains("Skipped"));
        assert_eq!(graph.nodes[n1].status, TaskStatus::Skipped);
    }

    #[test]
    fn per_node_timeout_is_honored() {
        // A node with a short timeout running a slow trusted script must time out.
        let exec = executor_with(vec![Permission::InputSimulate]);
        let mut graph = TaskGraph::new("slow");
        let id = graph.add_node(
            "slow",
            TaskNodeKind::SystemCommand { script: "sleep 30".into(), label: "slow".into() },
        );
        graph.nodes[id].timeout = Some(Duration::from_millis(200));
        let start = Instant::now();
        let results = exec.execute(&mut graph);
        assert!(!results[0].success);
        assert!(results[0].error.as_deref().unwrap_or_default().contains("Timed out"));
        assert!(start.elapsed() < Duration::from_secs(5));
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
            ACTION_TIMEOUT,
        );
        assert!(write.success, "clipboard write failed: {:?}", write.error);

        let read = exec.run_guarded(&TaskNodeKind::ClipboardRead, "Read clipboard", ACTION_TIMEOUT);
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
            ACTION_TIMEOUT,
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
            let read = reader.run_guarded(&TaskNodeKind::ClipboardRead, "Read clipboard", ACTION_TIMEOUT);
            assert_ne!(read.output, sentinel, "blocked write still reached the OS");
        }
    }

    /// A backend that performs no OS calls and records what it was asked to do,
    /// proving the executor routes every action through the [`PlatformBackend`]
    /// seam — on any OS, not just macOS.
    #[derive(Default)]
    struct RecordingBackend {
        calls: std::sync::Mutex<Vec<String>>,
    }
    impl RecordingBackend {
        fn ok(&self, what: String, label: &str) -> StepResult {
            self.calls.lock().unwrap().push(what);
            StepResult { node_id: 0, label: label.into(), success: true, output: String::new(), error: None }
        }
    }
    impl PlatformBackend for RecordingBackend {
        fn launch_app(&self, a: &str, _args: &[String], l: &str, _t: Duration) -> StepResult { self.ok(format!("launch:{a}"), l) }
        fn open_path(&self, p: &str, l: &str, _t: Duration) -> StepResult { self.ok(format!("open_path:{p}"), l) }
        fn open_url(&self, u: &str, l: &str, _t: Duration) -> StepResult { self.ok(format!("open_url:{u}"), l) }
        fn find_files(&self, q: &str, l: &str, _t: Duration) -> StepResult { self.ok(format!("find:{q}"), l) }
        fn clipboard_read(&self, l: &str, _t: Duration) -> StepResult { self.ok("clip_read".into(), l) }
        fn clipboard_write(&self, c: &str, l: &str, _t: Duration) -> StepResult { self.ok(format!("clip_write:{c}"), l) }
        fn list_running_apps(&self, l: &str, _t: Duration) -> StepResult { self.ok("list_apps".into(), l) }
        fn quit_app(&self, a: &str, l: &str, _t: Duration) -> StepResult { self.ok(format!("quit:{a}"), l) }
        fn switch_app(&self, a: &str, l: &str, _t: Duration) -> StepResult { self.ok(format!("switch:{a}"), l) }
        fn run_trusted_script(&self, s: &str, l: &str, _c: bool, _t: Duration) -> StepResult { self.ok(format!("script:{s}"), l) }
    }

    /// Build a fully-granted executor wired to a recording backend.
    fn recording_executor() -> (AgentExecutor, Arc<RecordingBackend>) {
        let registry = Arc::new(CapabilityRegistry::new());
        let gate = Arc::new(CapabilityGate::new(registry.clone()));
        let token = registry.grant(
            Namespace::new("agent"),
            vec![Permission::NetworkConnect, Permission::ProcessSpawn],
            "agent".into(),
            None,
            "test".into(),
        );
        let backend = Arc::new(RecordingBackend::default());
        let exec = AgentExecutor::with_backend(gate, token, None, backend.clone());
        (exec, backend)
    }

    #[test]
    fn executor_dispatches_every_action_through_the_backend() {
        // WebSearch must reach the backend as a fully-formed, percent-encoded
        // open_url — the executor owns that platform-independent policy, the
        // backend owns the OS call. No real process is ever spawned here.
        let (exec, backend) = recording_executor();
        let r = exec.run_guarded(
            &TaskNodeKind::WebSearch { query: "a b&c".into() },
            "Web Search",
            ACTION_TIMEOUT,
        );
        assert!(r.success);
        let calls = backend.calls.lock().unwrap();
        assert_eq!(
            calls.as_slice(),
            ["open_url:https://www.google.com/search?q=a%20b%26c"],
            "WebSearch should dispatch one encoded open_url to the backend"
        );
    }

    #[test]
    fn denied_action_never_reaches_the_backend() {
        // The capability gate must short-circuit before the backend is touched.
        let (exec, backend) = recording_executor(); // lacks ClipboardWrite
        let r = exec.run_guarded(
            &TaskNodeKind::ClipboardWrite { content: "x".into() },
            "Write to clipboard",
            ACTION_TIMEOUT,
        );
        assert!(!r.success);
        assert!(backend.calls.lock().unwrap().is_empty(), "denied action reached the backend");
    }

    #[test]
    fn noop_requires_no_capability() {
        // A token with zero permissions can still run a Noop.
        let exec = executor_with(vec![]);
        let r = exec.run_guarded(&TaskNodeKind::Noop { reason: "nothing".into() }, "noop", ACTION_TIMEOUT);
        assert!(r.success);
    }
}
