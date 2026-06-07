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

    // Prefix routing ($ / terminal / /app) short-circuits the rest.
    if let Some(cmd) = command_prefix_result(q) {
        return vec![cmd];
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

/// Map a `$`/`/terminal`/`/app` prefixed query to its synthetic command result.
/// Pure (no IPC/OS), so it is unit-testable. Returns `None` if not a command.
fn command_prefix_result(q: &str) -> Option<SearchResult> {
    // Terminal: "$ cmd", "$cmd", or "/terminal cmd".
    if q.starts_with('$') || q.starts_with("/terminal ") {
        let cmd = q
            .strip_prefix("$ ")
            .or_else(|| q.strip_prefix('$'))
            .or_else(|| q.strip_prefix("/terminal "))
            .unwrap_or("")
            .trim();
        if cmd.is_empty() {
            return None;
        }
        return Some(SearchResult {
            id: format!("terminal:{}", cmd),
            title: "Execute in Terminal".into(),
            subtitle: cmd.to_string(),
            category: "Command".into(),
            icon: "💻".into(),
            score: 2.0,
        });
    }

    // App command: "/app some task".
    if q.starts_with('/') {
        if let Some(space_idx) = q.find(' ') {
            let raw = &q[1..space_idx];
            let task = q[space_idx + 1..].trim();
            if !raw.is_empty() && !task.is_empty() {
                let app_name = match raw.to_lowercase().as_str() {
                    "chrome" => "Google Chrome",
                    "brave" => "Brave Browser",
                    "edge" => "Microsoft Edge",
                    "safari" => "Safari",
                    "firefox" => "Firefox",
                    _ => raw,
                };
                return Some(SearchResult {
                    id: format!("appcmd:{}|{}", app_name, task),
                    title: format!("Execute in {}", app_name),
                    subtitle: task.to_string(),
                    category: "Command".into(),
                    icon: "🤖".into(),
                    score: 2.0,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_dollar_and_slash_forms() {
        assert_eq!(command_prefix_result("$ ls -la").unwrap().id, "terminal:ls -la");
        assert_eq!(command_prefix_result("$htop").unwrap().id, "terminal:htop");
        assert_eq!(command_prefix_result("/terminal echo hi").unwrap().id, "terminal:echo hi");
    }

    #[test]
    fn appcmd_aliases_and_passthrough() {
        assert_eq!(command_prefix_result("/chrome open github").unwrap().id, "appcmd:Google Chrome|open github");
        assert_eq!(command_prefix_result("/notes write a memo").unwrap().id, "appcmd:notes|write a memo");
    }

    #[test]
    fn non_commands_return_none() {
        assert!(command_prefix_result("hello world").is_none());
        assert!(command_prefix_result("$").is_none());
        assert!(command_prefix_result("$   ").is_none());
        assert!(command_prefix_result("/app").is_none());
    }
}
