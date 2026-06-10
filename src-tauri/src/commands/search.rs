//! Search command — real search coordinator across all indexes.
//!
//! Queries applications, files (Spotlight), and system commands,
//! then merges and ranks results by fuzzy score.

use std::sync::Arc;

use serde::Serialize;
use tauri::command;

use super::system_search;
use crate::state::AppState;
use supersearch_runtime::extension::{ExtensionAction, ExtensionRegistry};

/// A single search result item.
///
/// All sources (apps, files, system commands, the agent, and extensions) are
/// produced as `SearchResult`s and merged into one ranked list. `category` is
/// the provenance tag; `action`, when present, carries an extension result's
/// declared action so the front-end can route activation to
/// `execute_extension_action` (the extension id is encoded in `id` as
/// `ext:<id>::<title>`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub category: String,
    pub icon: String,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<ExtensionAction>,
}

/// Execute a unified search query across all indexes.
///
/// Search sources (merged and ranked):
/// 1. Application bundles (/Applications, ~/Applications, /System/Applications)
/// 2. Files via Spotlight (mdfind)
/// 3. Built-in system commands
#[command]
pub fn search_query(
    query: String,
    state: tauri::State<'_, AppState>,
    registry: tauri::State<'_, Arc<ExtensionRegistry>>,
) -> Vec<SearchResult> {
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
            ..Default::default()
        });
    }

    // 5. Enabled (and, for scripts, trusted) extensions — fanned out
    //    concurrently with a tight budget, then merged into the same ranking.
    for hit in registry.query(q) {
        results.push(extension_hit_to_result(&query, hit));
    }

    // Sort by score descending.
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(12);
    results
}

/// Convert an extension query hit into a ranked [`SearchResult`]. Extensions
/// opt in by keyword, so a hit is already relevant; a title that contains the
/// query ranks a little higher. The id encodes the source extension so the
/// front-end can route activation back through `execute_extension_action`.
fn extension_hit_to_result(
    query: &str,
    hit: supersearch_runtime::extension::ExtensionQueryHit,
) -> SearchResult {
    let score = if hit.title.to_lowercase().contains(&query.trim().to_lowercase()) {
        0.95
    } else {
        0.8
    };
    SearchResult {
        id: format!("ext:{}::{}", hit.extension_id, hit.title),
        title: hit.title,
        subtitle: hit.subtitle,
        category: "Extension".into(),
        icon: "🧩".into(),
        score,
        action: hit.action,
    }
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
            ..Default::default()
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
                    ..Default::default()
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use supersearch_runtime::extension::ExtensionQueryHit;

    #[test]
    fn extension_hit_merges_as_ranked_routable_result() {
        let hit = ExtensionQueryHit {
            extension_id: "spotify".into(),
            title: "Play Daft Punk".into(),
            subtitle: "Artist".into(),
            action: Some(ExtensionAction::OpenUrl { url: "https://open.spotify.com/x".into() }),
        };
        let r = extension_hit_to_result("daft", hit);
        // Provenance + routing: category tags it; id encodes the source extension
        // so the front-end can dispatch to `execute_extension_action`.
        assert_eq!(r.category, "Extension");
        assert!(r.id.starts_with("ext:spotify::"), "id must encode the extension id");
        assert!(r.action.is_some(), "the extension action must be carried for routing");
        // Title contains the query → ranked above a non-matching hit.
        assert!(r.score > 0.9);

        let other = ExtensionQueryHit {
            extension_id: "x".into(),
            title: "Unrelated".into(),
            subtitle: String::new(),
            action: None,
        };
        assert!(extension_hit_to_result("zzz", other).score < r.score);
    }

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
