//! Agent Controller — top-level orchestrator.
//!
//! The single entry point for all agent interactions. Receives natural-language
//! queries, orchestrates the pipeline (classify → plan → execute), and returns
//! structured responses.
//!
//! Thread-safe: Can be shared across Tauri command handlers via `Arc`.

use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::context::ContextEngine;
use super::executor::{AgentExecutor, StepResult};
use super::memory::AgentMemory;
use super::patterns::{AgentIntent, PatternEngine};
use super::planner::TaskPlanner;
use super::task_graph::TaskGraph;
use crate::capability::gate::CapabilityGate;
use crate::capability::token::CapabilityToken;
use crate::journal::writer::JournalSender;

/// Request payload from the frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentRequest {
    /// The natural-language query from the user.
    pub query: String,
}

/// Structured response sent back to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    /// The original query.
    pub query: String,
    /// The classified intent (human-readable).
    pub intent: String,
    /// The task graph (plan) that was generated.
    pub plan: PlanSummary,
    /// Execution results for each step.
    pub steps: Vec<StepSummary>,
    /// Overall success/failure.
    pub success: bool,
    /// Human-readable summary of what happened.
    pub summary: String,
    /// Total execution time in milliseconds.
    pub duration_ms: u64,
}

/// Summary of the execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSummary {
    pub description: String,
    pub total_steps: usize,
    pub execution_id: String,
}

/// Summary of a single execution step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepSummary {
    pub label: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// The agent controller — thread-safe orchestrator.
pub struct AgentController {
    /// Pattern-based intent classifier.
    pattern_engine: PatternEngine,
    /// Intent → TaskGraph compiler.
    planner: TaskPlanner,
    /// TaskGraph executor.
    executor: AgentExecutor,
    /// Short-term interaction memory.
    memory: Mutex<AgentMemory>,
    /// System context awareness.
    context: Mutex<ContextEngine>,
}

impl AgentController {
    /// Create a new agent controller with all subsystems.
    ///
    /// The controller does not hold ambient OS authority: it executes actions
    /// only through `gate`, presenting `token`. Every decision is journaled via
    /// `journal`.
    pub fn new(
        gate: Arc<CapabilityGate>,
        token: CapabilityToken,
        journal: Option<JournalSender>,
    ) -> Self {
        info!("Agent controller initialized");
        Self {
            pattern_engine: PatternEngine::new(),
            planner: TaskPlanner::new(),
            executor: AgentExecutor::new(gate, token, journal),
            memory: Mutex::new(AgentMemory::new()),
            context: Mutex::new(ContextEngine::new()),
        }
    }

    /// Process a natural-language query through the full agent pipeline.
    ///
    /// Pipeline: Query → Classify → Plan → Execute → Response
    pub fn process_query(&self, query: &str) -> AgentResponse {
        let start = std::time::Instant::now();
        let query = query.trim();

        info!(query, "Agent processing query");

        // 1. Classify intent.
        let intent = self.pattern_engine.classify(query);
        let intent_label = self.intent_label(&intent);

        // 2. Compile into task graph.
        let mut graph = self.planner.plan(&intent);

        // 3. Execute the graph.
        let results = self.executor.execute(&mut graph);

        // 4. Build response.
        let duration_ms = start.elapsed().as_millis() as u64;
        let success = results.iter().all(|r| r.success);
        let summary = self.build_summary(&intent, &results, &graph);

        // 5. Record in memory.
        {
            let mut mem = self.memory.lock();
            mem.record(query, &intent_label, success, &summary);
        }

        // 6. Update context for app-related intents.
        self.update_context(&intent);

        let steps: Vec<StepSummary> = results
            .into_iter()
            .map(|r| StepSummary {
                label: r.label,
                success: r.success,
                output: r.output,
                error: r.error,
            })
            .collect();

        info!(
            query,
            intent = %intent_label,
            success,
            duration_ms,
            steps = steps.len(),
            "Agent query completed"
        );

        AgentResponse {
            query: query.to_string(),
            intent: intent_label,
            plan: PlanSummary {
                description: graph.metadata.description.clone(),
                total_steps: graph.metadata.total_steps,
                execution_id: graph.metadata.execution_id.clone(),
            },
            steps,
            success,
            summary,
            duration_ms,
        }
    }

    /// Check if a query looks like an agent command (vs a simple search).
    ///
    /// Returns true if the query matches known intent patterns.
    pub fn is_agent_query(&self, query: &str) -> bool {
        let intent = self.pattern_engine.classify(query);
        !matches!(intent, AgentIntent::Unknown { .. })
    }

    /// Get recent memory entries.
    pub fn recent_history(&self, n: usize) -> Vec<super::memory::MemoryEntry> {
        let mem = self.memory.lock();
        mem.recent(n).into_iter().cloned().collect()
    }

    // ─── Private helpers ─────────────────────────────────────────────

    fn intent_label(&self, intent: &AgentIntent) -> String {
        match intent {
            AgentIntent::LaunchApp { app_name, .. } => format!("Launch App: {}", app_name),
            AgentIntent::OpenFile { path } => format!("Open File: {}", path),
            AgentIntent::OpenUrl { url } => format!("Open URL: {}", url),
            AgentIntent::WebSearch { query } => format!("Web Search: {}", query),
            AgentIntent::FindFiles { query } => format!("Find Files: {}", query),
            AgentIntent::ClipboardRead => "Read Clipboard".into(),
            AgentIntent::ClipboardWrite { .. } => "Write Clipboard".into(),
            AgentIntent::SystemCommand { command } => format!("System: {:?}", command),
            AgentIntent::ListRunningApps => "List Running Apps".into(),
            AgentIntent::SystemInfo { kind } => format!("System Info: {:?}", kind),
            AgentIntent::MultiStep { intents } => format!("Multi-Step ({} actions)", intents.len()),
            AgentIntent::QuitApp { app_name } => format!("Quit App: {}", app_name),
            AgentIntent::SwitchApp { app_name } => format!("Switch to: {}", app_name),
            AgentIntent::Unknown { .. } => "Unknown (fallback to search)".into(),
        }
    }

    fn build_summary(
        &self,
        intent: &AgentIntent,
        results: &[StepResult],
        _graph: &TaskGraph,
    ) -> String {
        if results.is_empty() {
            return "No actions to execute.".into();
        }

        let success_count = results.iter().filter(|r| r.success).count();
        let total = results.len();

        if total == 1 {
            let r = &results[0];
            if r.success {
                // For info queries, include the output.
                match intent {
                    AgentIntent::ClipboardRead
                    | AgentIntent::ListRunningApps
                    | AgentIntent::SystemInfo { .. }
                    | AgentIntent::WebSearch { .. }
                    | AgentIntent::FindFiles { .. } => {
                        return r.output.clone();
                    }
                    _ => return format!("✓ {}", r.label),
                }
            } else {
                return format!("✗ {}: {}", r.label, r.error.as_deref().unwrap_or("Unknown error"));
            }
        }

        if success_count == total {
            format!("✓ All {} steps completed successfully", total)
        } else {
            format!("⚠ {}/{} steps completed ({} failed)", success_count, total, total - success_count)
        }
    }

    fn update_context(&self, intent: &AgentIntent) {
        let mut ctx = self.context.lock();
        match intent {
            AgentIntent::LaunchApp { app_name, .. } | AgentIntent::SwitchApp { app_name } => {
                ctx.record_app(app_name);
            }
            AgentIntent::OpenFile { path } => {
                ctx.record_file(path);
            }
            _ => {}
        }
    }
}
