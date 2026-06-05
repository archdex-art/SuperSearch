//! Telemetry command — provides real-time kernel metrics to the frontend.

use serde::Serialize;
use tauri::command;

use crate::state::AppState;

/// Telemetry snapshot returned to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct TelemetrySnapshot {
    /// Scheduler tick count.
    pub scheduler_ticks: u64,
    /// Whether all queues are empty (idle).
    pub scheduler_idle: bool,
    /// Number of active (non-revoked) capability grants.
    pub capabilities_active: usize,
    /// Total capability grants ever issued.
    pub capabilities_total: usize,
    /// Kernel uptime in seconds.
    pub uptime_seconds: f64,
    /// Boot time in milliseconds.
    pub boot_time_ms: u64,
}

/// Get a snapshot of kernel telemetry for the UI telemetry strip.
#[command]
pub fn get_telemetry(state: tauri::State<'_, AppState>) -> TelemetrySnapshot {
    TelemetrySnapshot {
        scheduler_ticks: state.queue.current_tick(),
        scheduler_idle: state.queue.is_empty(),
        capabilities_active: state.registry.active_grants(),
        capabilities_total: state.registry.total_grants(),
        uptime_seconds: state.boot_instant.elapsed().as_secs_f64(),
        boot_time_ms: state.boot_time_ms,
    }
}
