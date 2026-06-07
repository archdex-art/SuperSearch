//! Agent command — IPC handler for the agentic AI controller.
//!
//! Exposes the agent pipeline to the frontend via Tauri commands.

use std::time::Duration;

use serde::Serialize;
use tauri::command;

use supersearch_runtime::scheduler::PriorityClass;

use crate::state::AppState;

/// Maximum accepted length of a natural-language query, in bytes. Bounds the
/// work any single IPC call can trigger and rejects pathological input early.
const MAX_QUERY_LEN: usize = 2048;

/// Agent query response sent to the frontend.
#[derive(Debug, Serialize)]
pub struct AgentQueryResponse {
    pub query: String,
    pub intent: String,
    pub plan_description: String,
    pub total_steps: usize,
    pub steps: Vec<AgentStepResponse>,
    pub success: bool,
    pub summary: String,
    pub duration_ms: u64,
}

/// A single step in the agent's execution.
#[derive(Debug, Serialize)]
pub struct AgentStepResponse {
    pub label: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Execute a natural-language query through the agentic AI controller.
///
/// Pipeline: Query → Intent Classification → Task Graph → Execution → Response
///
/// The agent spawns blocking OS processes (`open`/`osascript`/…). To keep the
/// async IPC runtime responsive, that work runs on a dedicated blocking thread
/// via `spawn_blocking` rather than on an async worker — so a multi-step command
/// never stalls other IPC calls or the UI.
#[command]
pub async fn agent_query(
    query: String,
    state: tauri::State<'_, AppState>,
) -> Result<AgentQueryResponse, String> {
    let query = validate_query(query)?;
    let agent = state.agent.clone();

    // Run the agent *through the scheduler*: enqueue an Interactive task that
    // the kernel's scheduler loop drives (so agent work is scheduled,
    // prioritized, and observable in telemetry). The blocking OS pipeline runs
    // on a blocking thread; the result returns over a oneshot channel.
    let (tx, rx) = tokio::sync::oneshot::channel();
    let task = state
        .queue
        .builder(PriorityClass::Interactive)
        .origin("ipc")
        .label("agent_query")
        .spawn(async move {
            let result = tokio::task::spawn_blocking(move || agent.process_query(&query)).await;
            let _ = tx.send(result);
        });
    state.queue.enqueue(task);

    // Bound the overall wait; the agent's per-action timeouts cap each step.
    let response = tokio::time::timeout(Duration::from_secs(60), rx)
        .await
        .map_err(|_| "Agent query timed out".to_string())?
        .map_err(|_| "Agent task was dropped".to_string())?
        .map_err(|e| format!("Agent task failed: {e}"))?;

    Ok(AgentQueryResponse {
        query: response.query,
        intent: response.intent,
        plan_description: response.plan.description,
        total_steps: response.plan.total_steps,
        steps: response.steps.into_iter().map(|s| AgentStepResponse {
            label: s.label,
            success: s.success,
            output: s.output,
            error: s.error,
        }).collect(),
        success: response.success,
        summary: response.summary,
        duration_ms: response.duration_ms,
    })
}

/// Check if a query is an agent command (vs a simple search).
#[command]
pub fn agent_check(
    query: String,
    state: tauri::State<'_, AppState>,
) -> bool {
    match validate_query(query) {
        Ok(q) => state.agent.is_agent_query(&q),
        Err(_) => false,
    }
}

/// Validate and normalize an incoming query: reject empty/oversized input.
fn validate_query(query: String) -> Result<String, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("Query is empty".into());
    }
    if trimmed.len() > MAX_QUERY_LEN {
        return Err(format!(
            "Query too long ({} bytes; max {})",
            trimmed.len(),
            MAX_QUERY_LEN
        ));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_query_trims_and_accepts() {
        assert_eq!(validate_query("  open chrome  ".into()).unwrap(), "open chrome");
    }

    #[test]
    fn validate_query_rejects_empty_and_whitespace() {
        assert!(validate_query("".into()).is_err());
        assert!(validate_query("   ".into()).is_err());
    }

    #[test]
    fn validate_query_rejects_oversized() {
        let big = "x".repeat(MAX_QUERY_LEN + 1);
        assert!(validate_query(big).is_err());
        // Exactly at the limit is accepted.
        assert!(validate_query("y".repeat(MAX_QUERY_LEN)).is_ok());
    }
}
