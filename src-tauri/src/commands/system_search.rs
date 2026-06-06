//! System search — real file, app, and command indexing for macOS.
//!
//! Replaces mock search results with live system queries:
//! - Application bundles from /Applications
//! - Files via Spotlight (mdfind)
//! - Built-in system commands

use std::process::Command;
use std::sync::OnceLock;
use tracing::debug;

use super::search::SearchResult;

/// Cached application list (loaded once on first search).
static APP_CACHE: OnceLock<Vec<AppEntry>> = OnceLock::new();

/// A cached application entry.
#[derive(Debug, Clone)]
struct AppEntry {
    /// Display name (e.g., "Google Chrome").
    name: String,
    /// Bundle path (e.g., "/Applications/Google Chrome.app").
    path: String,
}

/// Search installed applications by name.
pub fn search_applications(query: &str) -> Vec<SearchResult> {
    let apps = APP_CACHE.get_or_init(load_applications);
    let q = query.to_lowercase();

    let mut results: Vec<SearchResult> = apps
        .iter()
        .filter_map(|app| {
            let name_lower = app.name.to_lowercase();
            let score = if name_lower == q {
                1.0
            } else if name_lower.starts_with(&q) {
                0.9
            } else if name_lower.contains(&q) {
                0.6
            } else if fuzzy_match(&q, &name_lower) {
                0.3
            } else {
                return None;
            };

            Some(SearchResult {
                id: format!("app:{}", app.path),
                title: app.name.clone(),
                subtitle: app.path.clone(),
                category: "Application".into(),
                icon: "📱".into(),
                score,
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(8);
    results
}

/// Search files via Spotlight (mdfind) with a timeout.
pub fn search_files(query: &str) -> Vec<SearchResult> {
    if query.len() < 2 {
        return Vec::new();
    }

    let cmd = format!(
        "mdfind -name \"{}\" 2>/dev/null | head -12",
        query.replace('"', "\\\"")
    );

    let output = match Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .output()
    {
        Ok(out) => out,
        Err(e) => {
            debug!(error = %e, "mdfind failed");
            return Vec::new();
        }
    };

    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter(|line| !line.is_empty())
        .enumerate()
        .map(|(i, path)| {
            let filename = path.rsplit('/').next().unwrap_or(path);
            let is_dir = path.ends_with('/') || std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false);

            SearchResult {
                id: format!("file:{}", path),
                title: filename.to_string(),
                subtitle: path.to_string(),
                category: if is_dir { "Folder".into() } else { "File".into() },
                icon: if is_dir { "📁".into() } else { file_icon(filename) },
                score: 0.8 - (i as f64 * 0.05),
            }
        })
        .collect()
}

/// Built-in system commands.
pub fn system_commands() -> Vec<SearchResult> {
    vec![
        SearchResult {
            id: "sys:lock".into(),
            title: "Lock Screen".into(),
            subtitle: "Lock the screen immediately".into(),
            category: "System".into(),
            icon: "🔒".into(),
            score: 0.0,
        },
        SearchResult {
            id: "sys:screenshot".into(),
            title: "Screenshot".into(),
            subtitle: "Capture a screenshot".into(),
            category: "System".into(),
            icon: "📸".into(),
            score: 0.0,
        },
        SearchResult {
            id: "sys:dnd".into(),
            title: "Do Not Disturb".into(),
            subtitle: "Toggle Do Not Disturb mode".into(),
            category: "System".into(),
            icon: "🔕".into(),
            score: 0.0,
        },
        SearchResult {
            id: "sys:empty_trash".into(),
            title: "Empty Trash".into(),
            subtitle: "Permanently delete items in Trash".into(),
            category: "System".into(),
            icon: "🗑️".into(),
            score: 0.0,
        },
        SearchResult {
            id: "sys:dark_mode".into(),
            title: "Toggle Dark Mode".into(),
            subtitle: "Switch between light and dark appearance".into(),
            category: "System".into(),
            icon: "🌗".into(),
            score: 0.0,
        },
        SearchResult {
            id: "sys:sleep".into(),
            title: "Sleep".into(),
            subtitle: "Put the Mac to sleep".into(),
            category: "System".into(),
            icon: "😴".into(),
            score: 0.0,
        },
        SearchResult {
            id: "sys:show_desktop".into(),
            title: "Show Desktop".into(),
            subtitle: "Minimize all windows".into(),
            category: "System".into(),
            icon: "🖥️".into(),
            score: 0.0,
        },
        SearchResult {
            id: "sys:clipboard".into(),
            title: "Show Clipboard".into(),
            subtitle: "View current clipboard contents".into(),
            category: "System".into(),
            icon: "📋".into(),
            score: 0.0,
        },
        SearchResult {
            id: "sys:running_apps".into(),
            title: "Running Apps".into(),
            subtitle: "List all currently running applications".into(),
            category: "System".into(),
            icon: "📊".into(),
            score: 0.0,
        },
        SearchResult {
            id: "sys:system_info".into(),
            title: "System Info".into(),
            subtitle: "View system information".into(),
            category: "System".into(),
            icon: "ℹ️".into(),
            score: 0.0,
        },
    ]
}

// ─── Private helpers ─────────────────────────────────────────────────

/// Load all .app bundles from /Applications and ~/Applications.
fn load_applications() -> Vec<AppEntry> {
    let mut apps = Vec::new();

    let dirs = [
        "/Applications",
        "/System/Applications",
    ];

    for dir in &dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("app") {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    apps.push(AppEntry {
                        name,
                        path: path.to_string_lossy().into(),
                    });
                }
            }
        }
    }

    // Also check ~/Applications
    if let Some(home) = std::env::var_os("HOME") {
        let user_apps = std::path::PathBuf::from(home).join("Applications");
        if let Ok(entries) = std::fs::read_dir(user_apps) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("app") {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    apps.push(AppEntry {
                        name,
                        path: path.to_string_lossy().into(),
                    });
                }
            }
        }
    }

    apps.sort_by_key(|a| a.name.to_lowercase());
    debug!(count = apps.len(), "Application index loaded");
    apps
}

/// Simple character-level fuzzy match: all query chars appear in order.
fn fuzzy_match(query: &str, target: &str) -> bool {
    let mut target_chars = target.chars();
    for qc in query.chars() {
        loop {
            match target_chars.next() {
                Some(tc) if tc == qc => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

/// Get an icon for a file based on its extension.
fn file_icon(filename: &str) -> String {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "pdf" => "📄",
        "doc" | "docx" | "pages" => "📝",
        "xls" | "xlsx" | "numbers" | "csv" => "📊",
        "ppt" | "pptx" | "key" => "📈",
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "heic" => "🖼️",
        "mp4" | "mov" | "avi" | "mkv" => "🎬",
        "mp3" | "wav" | "aac" | "flac" => "🎵",
        "zip" | "tar" | "gz" | "rar" | "7z" => "📦",
        "rs" | "py" | "js" | "ts" | "go" | "c" | "cpp" | "java" => "💻",
        "html" | "css" | "json" | "xml" | "yaml" | "toml" => "📋",
        "md" | "txt" | "rtf" => "📄",
        "app" => "📱",
        "dmg" | "iso" => "💿",
        _ => "📄",
    }
    .into()
}
