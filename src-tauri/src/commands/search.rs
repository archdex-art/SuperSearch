//! Search command — real search coordinator across all indexes.
//!
//! Queries applications, files (Spotlight), and system commands,
//! then merges and ranks results by fuzzy score.

use serde::Serialize;
use tauri::command;

use super::system_search;
use crate::state::AppState;

/// A single search result item.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub category: String,
    pub icon: String,
    pub score: f64,
}

/// Execute a unified search query across all indexes.
///
/// Search sources (merged and ranked):
/// 1. Application bundles (/Applications, ~/Applications, /System/Applications)
/// 2. Files via Spotlight (mdfind)
/// 3. Built-in system commands
#[command]
pub fn search_query(query: String, state: tauri::State<'_, AppState>) -> Vec<SearchResult> {
    let q = query.trim();

    if q.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    // 1. Terminal commands
    if q.starts_with('$') || q.starts_with("/terminal ") {
        let cmd = if let Some(rest) = q.strip_prefix("$ ") {
            rest
        } else if let Some(rest) = q.strip_prefix('$') {
            rest
        } else {
            &q[10..]
        };

        if !cmd.trim().is_empty() {
            results.push(SearchResult {
                id: format!("terminal:{}", cmd),
                title: "Execute in Terminal".into(),
                subtitle: cmd.to_string(),
                category: "Command".into(),
                icon: "💻".into(),
                score: 2.0,
            });
            return results;
        }
    }

    // 2. App commands (e.g. /chatgpt what is regression)
    if q.starts_with('/') && !q.starts_with("/terminal") {
        if let Some(space_idx) = q.find(' ') {
            let raw_app_name = &q[1..space_idx];
            let task = &q[space_idx + 1..];
            if !raw_app_name.is_empty() && !task.trim().is_empty() {
                let app_name = match raw_app_name.to_lowercase().as_str() {
                    "chrome" => "Google Chrome",
                    "brave" => "Brave Browser",
                    "edge" => "Microsoft Edge",
                    "safari" => "Safari",
                    "firefox" => "Firefox",
                    _ => raw_app_name,
                };
                results.push(SearchResult {
                    id: format!("appcmd:{}|{}", app_name, task),
                    title: format!("Execute in {}", app_name),
                    subtitle: task.to_string(),
                    category: "Command".into(),
                    icon: "🤖".into(),
                    score: 2.0,
                });
                return results;
            }
        }
    }

    let q_lower = q.to_lowercase();

    // 1. Application search.
    let mut system_apps = system_search::search_applications(&q_lower);
    results.append(&mut system_apps);

    // 2. Files via Spotlight (skip 1-char queries to avoid noise / a costly
    //    mdfind that matches nearly everything).
    if q.len() >= 2 {
        let mut files = system_search::search_files(&q_lower);
        results.append(&mut files);
    }

    // 3. System commands (fuzzy filtered).
    let sys_commands = system_search::system_commands();
    for mut cmd in sys_commands {
        let title_lower = cmd.title.to_lowercase();
        let subtitle_lower = cmd.subtitle.to_lowercase();

        if title_lower.contains(q) {
            cmd.score = 0.85;
            results.push(cmd);
        } else if subtitle_lower.contains(q) {
            cmd.score = 0.5;
            results.push(cmd);
        } else if q.chars().all(|c| title_lower.contains(c)) {
            cmd.score = 0.25;
            results.push(cmd);
        }
    }

    // 4. If it looks like a natural-language command, add an "Ask Agent" result.
    if state.agent.is_agent_query(&query) {
        results.insert(0, SearchResult {
            id: format!("agent:{}", query),
            title: format!("⚡ {}", query),
            subtitle: "Execute with AI Agent".into(),
            category: "Agent".into(),
            icon: "🤖".into(),
            score: 1.5, // Always rank first.
        });
    }

    // Sort by score descending.
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(12);
    results
}
