//! Journal IPC — read-only audit view over the append-only journal.
//!
//! Uses `JournalReader` (which validates CRC32 and never re-executes anything)
//! to summarize what the runtime recorded: how many entries of each kind, and
//! the most recent ones. This is the safe "replay/inspect" entry point — it
//! reconstructs the record, it does not re-run side effects.

use serde::Serialize;
use tauri::command;

use supersearch_runtime::journal::JournalReader;

use crate::state::AppState;

/// A summary of the on-disk journal for the audit view.
#[derive(Debug, Serialize)]
pub struct JournalSummary {
    pub total: usize,
    /// (kind, count) pairs, descending by count.
    pub by_kind: Vec<(String, usize)>,
    /// The most recent entries (newest last), capped.
    pub recent: Vec<JournalEntryView>,
}

/// A single journal entry rendered for display.
#[derive(Debug, Serialize)]
pub struct JournalEntryView {
    pub sequence: u64,
    pub kind: String,
    pub origin: String,
    pub timestamp_ns: u64,
    pub payload_preview: String,
}

/// Read and summarize the journal. Returns an empty summary if nothing has been
/// written yet (e.g., first launch before any action).
#[command]
pub async fn get_journal_summary(
    limit: Option<usize>,
    state: tauri::State<'_, AppState>,
) -> Result<JournalSummary, String> {
    let reader = JournalReader::new(&state.journal_dir, true);
    let entries = match reader.read_all().await {
        Ok(e) => e,
        // No journal dir/segments yet → empty summary, not an error.
        Err(_) => {
            return Ok(JournalSummary { total: 0, by_kind: Vec::new(), recent: Vec::new() })
        }
    };

    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for e in &entries {
        *counts.entry(format!("{:?}", e.kind)).or_insert(0) += 1;
    }
    let mut by_kind: Vec<(String, usize)> = counts.into_iter().collect();
    by_kind.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let cap = limit.unwrap_or(50).min(500);
    let recent: Vec<JournalEntryView> = entries
        .iter()
        .rev()
        .take(cap)
        .map(|e| JournalEntryView {
            sequence: e.sequence.raw(),
            kind: format!("{:?}", e.kind),
            origin: e.origin.clone(),
            timestamp_ns: e.timestamp_ns,
            payload_preview: String::from_utf8_lossy(&e.payload)
                .chars()
                .take(200)
                .collect(),
        })
        .collect();

    Ok(JournalSummary { total: entries.len(), by_kind, recent })
}
