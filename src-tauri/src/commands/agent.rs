//! Agent command — IPC handler for the agentic AI controller.
//!
//! Exposes the agent pipeline to the frontend via Tauri commands.

use serde::{Deserialize, Serialize};
use tauri::command;

use crate::state::AppState;

/// Agent query request from the frontend.
#[derive(Debug, Deserialize)]
pub struct AgentQueryRequest {
    pub query: String,
}

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
#[command]
pub fn agent_query(
    query: String,
    state: tauri::State<'_, AppState>,
) -> Result<AgentQueryResponse, String> {
    let response = state.agent.process_query(&query);

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
    state.agent.is_agent_query(&query)
}
