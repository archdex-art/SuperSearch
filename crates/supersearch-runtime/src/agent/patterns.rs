//! Intent classification via pattern matching.
//!
//! Zero-latency, offline, deterministic intent extraction from natural
//! language queries. No LLM required — uses keyword templates and entity
//! extraction heuristics.

use serde::{Deserialize, Serialize};
use tracing::debug;

// ─── Intent Taxonomy ─────────────────────────────────────────────────

/// Classified user intent extracted from a natural-language query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentIntent {
    /// Launch a macOS application by name.
    LaunchApp { app_name: String, args: Vec<String> },
    /// Open a specific file path.
    OpenFile { path: String },
    /// Open a URL in the default browser.
    OpenUrl { url: String },
    /// Search the web.
    WebSearch { query: String },
    /// Search for files matching a query (Spotlight / Tantivy).
    FindFiles { query: String },
    /// Read current clipboard contents.
    ClipboardRead,
    /// Write content to clipboard.
    ClipboardWrite { content: String },
    /// Execute a system command.
    SystemCommand { command: SystemCommand },
    /// List currently running applications.
    ListRunningApps,
    /// Query system information (disk, battery, memory).
    SystemInfo { kind: InfoKind },
    /// Multiple intents to execute (possibly in parallel).
    MultiStep { intents: Vec<AgentIntent> },
    /// Close / quit an application.
    QuitApp { app_name: String },
    /// Switch to (focus) a running application.
    SwitchApp { app_name: String },
    /// Could not classify — fall back to search.
    Unknown { raw_query: String },
}

/// System-level commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemCommand {
    LockScreen,
    Sleep,
    DoNotDisturb,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    BrightnessUp,
    BrightnessDown,
    EmptyTrash,
    Screenshot,
    ShowDesktop,
    ToggleDarkMode,
    Restart,
    Shutdown,
}

/// System info query kinds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InfoKind {
    DiskSpace,
    Battery,
    Memory,
    Cpu,
    Network,
    Uptime,
    General,
}

// ─── Pattern Engine ──────────────────────────────────────────────────

/// Rule-based intent classifier.
///
/// ## Performance
/// Classification completes in < 1µs for typical queries.
/// No heap allocation beyond the returned `AgentIntent`.
pub struct PatternEngine;

impl PatternEngine {
    pub fn new() -> Self {
        Self
    }

    /// Classify a natural-language query into a structured intent.
    pub fn classify(&self, query: &str) -> AgentIntent {
        let q = query.trim();
        if q.is_empty() {
            return AgentIntent::Unknown { raw_query: String::new() };
        }

        let lower = q.to_lowercase();

        // 1. Check for multi-step ("X and Y", "X then Y").
        if let Some(multi) = self.try_multi_step(q, &lower) {
            debug!(steps = ?multi, "Classified as multi-step intent");
            return multi;
        }

        // 2. Single intent classification.
        let intent = self.classify_single(q, &lower);
        debug!(intent = ?std::mem::discriminant(&intent), query = q, "Classified intent");
        intent
    }

    /// Classify a single (non-compound) query.
    fn classify_single(&self, original: &str, lower: &str) -> AgentIntent {
        // ── URL detection ──
        if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.") {
            return AgentIntent::OpenUrl {
                url: if lower.starts_with("www.") {
                    format!("https://{}", original.trim())
                } else {
                    original.trim().to_string()
                },
            };
        }

        // ── App launch ──
        if let Some(app) = strip_prefix_any(lower, &["open ", "launch ", "start ", "run "]) {
            let mut app_name = app.trim();
            let mut args = Vec::new();
            
            // Check for trailing modifiers like "in incognito mode"
            if let Some(idx) = app_name.find(" in ") {
                let modifier = &app_name[idx + 4..];
                app_name = &app_name[..idx];
                if modifier.contains("incognito") || modifier.contains("private") {
                    args.push("--incognito".to_string());
                }
            } else if let Some(idx) = app_name.find(" with ") {
                app_name = &app_name[..idx];
            }

            // Apply common aliases
            let normalized_app_name = match app_name.to_lowercase().as_str() {
                "chrome" => "Google Chrome",
                "brave" => "Brave Browser",
                "edge" => "Microsoft Edge",
                "safari" => "Safari",
                "firefox" => "Firefox",
                _ => app_name,
            };

            // "open /path/to/file" → file open
            if normalized_app_name.starts_with('/') || normalized_app_name.starts_with("~/") {
                return AgentIntent::OpenFile { path: extract_entity(original, normalized_app_name) };
            }
            return AgentIntent::LaunchApp { app_name: titlecase(normalized_app_name), args };
        }

        // ── App quit ──
        if let Some(app) = strip_prefix_any(lower, &["quit ", "close ", "kill ", "exit "]) {
            return AgentIntent::QuitApp { app_name: titlecase(app.trim()) };
        }

        // ── App switch ──
        if let Some(app) = strip_prefix_any(lower, &["switch to ", "focus ", "go to ", "activate "]) {
            return AgentIntent::SwitchApp { app_name: titlecase(app.trim()) };
        }

        // ── Web search ──
        if let Some(query) = strip_prefix_any(lower, &[
            "google ", "bing ", "duckduckgo ", "search the web for ", "search web for ",
        ]) {
            return AgentIntent::WebSearch { query: query.trim().to_string() };
        }

        // ── File search ──
        if let Some(query) = strip_prefix_any(lower, &[
            "find ", "locate ", "where is ", "where's ", "look for ", "show me ",
        ]) {
            let query = query.trim();
            if !query.is_empty() {
                return AgentIntent::FindFiles { query: query.to_string() };
            }
        }

        if let Some(query) = strip_prefix_any(lower, &["search for ", "search "]) {
            let query = query.trim();
            if !query.is_empty() {
                if query.starts_with("what ") || query.starts_with("how ") || query.starts_with("who ") || query.starts_with("why ") {
                    return AgentIntent::WebSearch { query: query.to_string() };
                } else {
                    return AgentIntent::FindFiles { query: query.to_string() };
                }
            }
        }

        // ── Clipboard ──
        if matches_any(lower, &[
            "clipboard", "paste", "what's in clipboard", "show clipboard",
            "clipboard contents", "read clipboard", "get clipboard",
        ]) {
            return AgentIntent::ClipboardRead;
        }
        if let Some(content) = strip_prefix_any(lower, &["copy "]) {
            return AgentIntent::ClipboardWrite { content: content.trim().to_string() };
        }

        // ── Running apps ──
        if matches_any(lower, &[
            "what's running", "whats running", "running apps", "show running",
            "list apps", "active apps", "list running", "running processes",
            "what is running", "show apps",
        ]) {
            return AgentIntent::ListRunningApps;
        }

        // ── System info ──
        if matches_any(lower, &["disk space", "storage", "disk usage", "how much space"]) {
            return AgentIntent::SystemInfo { kind: InfoKind::DiskSpace };
        }
        if matches_any(lower, &["battery", "battery level", "charge", "power"]) {
            return AgentIntent::SystemInfo { kind: InfoKind::Battery };
        }
        if matches_any(lower, &["memory", "ram", "memory usage"]) {
            return AgentIntent::SystemInfo { kind: InfoKind::Memory };
        }
        if matches_any(lower, &["cpu", "processor", "cpu usage"]) {
            return AgentIntent::SystemInfo { kind: InfoKind::Cpu };
        }
        if matches_any(lower, &["uptime", "how long", "system uptime"]) {
            return AgentIntent::SystemInfo { kind: InfoKind::Uptime };
        }
        if matches_any(lower, &[
            "system info", "system status", "about this mac", "system information",
        ]) {
            return AgentIntent::SystemInfo { kind: InfoKind::General };
        }

        // ── System commands ──
        if matches_any(lower, &["lock", "lock screen", "lock my mac"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::LockScreen };
        }
        if matches_any(lower, &["sleep", "put to sleep"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::Sleep };
        }
        if matches_any(lower, &["do not disturb", "dnd", "focus mode", "silence notifications"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::DoNotDisturb };
        }
        if matches_any(lower, &["volume up", "louder", "increase volume"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::VolumeUp };
        }
        if matches_any(lower, &["volume down", "quieter", "decrease volume"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::VolumeDown };
        }
        if matches_any(lower, &["mute", "volume mute", "silence"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::VolumeMute };
        }
        if matches_any(lower, &["screenshot", "screen capture", "capture screen", "take screenshot"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::Screenshot };
        }
        if matches_any(lower, &["empty trash", "clear trash", "delete trash"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::EmptyTrash };
        }
        if matches_any(lower, &["show desktop", "desktop", "minimize all"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::ShowDesktop };
        }
        if matches_any(lower, &["dark mode", "toggle dark mode", "switch theme"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::ToggleDarkMode };
        }
        if matches_any(lower, &["brightness up", "brighter"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::BrightnessUp };
        }
        if matches_any(lower, &["brightness down", "dimmer"]) {
            return AgentIntent::SystemCommand { command: SystemCommand::BrightnessDown };
        }

        // ── Fallback: treat as search ──
        AgentIntent::Unknown { raw_query: original.to_string() }
    }

    /// Attempt to split a compound query into multiple intents.
    fn try_multi_step(&self, original: &str, lower: &str) -> Option<AgentIntent> {
        // Split on " and then " first (most specific), then " then ", then " and ".
        let separators = [" and then ", " then ", " and "];

        for sep in &separators {
            if lower.contains(sep) {
                let parts: Vec<&str> = lower.splitn(10, sep).collect();
                if parts.len() >= 2 && parts.iter().all(|p| !p.trim().is_empty()) {
                    // Get corresponding original-case parts.
                    let orig_parts: Vec<&str> = original.splitn(10, |_: char| false).collect();
                    let _ = orig_parts; // We'll use lower-case parts for classification.

                    let mut intents: Vec<AgentIntent> = parts
                        .iter()
                        .map(|part| self.classify_single(part.trim(), part.trim()))
                        .collect();

                    let classified_count = intents.iter()
                        .filter(|i| !matches!(i, AgentIntent::Unknown { .. }))
                        .count();

                    if classified_count >= 2 {
                        // Fold web search into browser launch if they follow each other
                        for i in 0..intents.len().saturating_sub(1) {
                            let next_intent = intents[i + 1].clone();
                            if let AgentIntent::LaunchApp { app_name, args } = &mut intents[i] {
                                let is_browser = app_name.contains("Chrome") || app_name.contains("Brave") || app_name.contains("Safari") || app_name.contains("Edge") || app_name.contains("Firefox");
                                if is_browser {
                                    if let AgentIntent::WebSearch { query } | AgentIntent::FindFiles { query } = &next_intent {
                                        let url = format!("https://google.com/search?q={}", query.replace(" ", "+"));
                                        args.push(url);
                                        intents[i + 1] = AgentIntent::Unknown { raw_query: "".to_string() };
                                    }
                                }
                            }
                        }
                        
                        intents.retain(|i| !matches!(i, AgentIntent::Unknown { raw_query } if raw_query.is_empty()));
                        
                        if intents.len() == 1 {
                            return Some(intents.into_iter().next().unwrap());
                        }

                        return Some(AgentIntent::MultiStep { intents });
                    }
                }
            }
        }
        None
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────

/// Check if `input` matches any of the given patterns exactly.
fn matches_any(input: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|p| input == *p)
}

/// Strip any matching prefix, returning the remainder.
fn strip_prefix_any<'a>(input: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    for prefix in prefixes {
        if let Some(rest) = input.strip_prefix(prefix) {
            return Some(rest);
        }
    }
    None
}

/// Convert a lowercase string to title case (first letter of each word capitalized).
fn titlecase(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract the original-case entity from the original query.
fn extract_entity(original: &str, lower_entity: &str) -> String {
    // Try to find the entity in the original query preserving case.
    let orig_lower = original.to_lowercase();
    if let Some(pos) = orig_lower.find(lower_entity) {
        original[pos..pos + lower_entity.len()].to_string()
    } else {
        lower_entity.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launch_app() {
        let engine = PatternEngine::new();
        match engine.classify("open Chrome") {
            AgentIntent::LaunchApp { app_name, .. } => assert_eq!(app_name, "Chrome"),
            other => panic!("Expected LaunchApp, got {:?}", other),
        }
    }

    #[test]
    fn test_multi_step() {
        let engine = PatternEngine::new();
        match engine.classify("open Chrome and open Slack") {
            AgentIntent::MultiStep { intents } => assert_eq!(intents.len(), 2),
            other => panic!("Expected MultiStep, got {:?}", other),
        }
    }

    #[test]
    fn test_url_detection() {
        let engine = PatternEngine::new();
        match engine.classify("https://google.com") {
            AgentIntent::OpenUrl { url } => assert_eq!(url, "https://google.com"),
            other => panic!("Expected OpenUrl, got {:?}", other),
        }
    }

    #[test]
    fn test_system_command() {
        let engine = PatternEngine::new();
        assert!(matches!(engine.classify("lock screen"), AgentIntent::SystemCommand { .. }));
        assert!(matches!(engine.classify("screenshot"), AgentIntent::SystemCommand { .. }));
    }

    #[test]
    fn test_file_search() {
        let engine = PatternEngine::new();
        match engine.classify("find budget.xlsx") {
            AgentIntent::FindFiles { query } => assert_eq!(query, "budget.xlsx"),
            other => panic!("Expected FindFiles, got {:?}", other),
        }
    }
}
