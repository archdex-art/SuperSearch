//! System search — cross-platform file, app, and command indexing.
//!
//! Each platform has its own backend for app discovery and file search:
//!
//! | Platform | App source                          | File search       |
//! |----------|-------------------------------------|-------------------|
//! | macOS    | `/Applications` + `~/Applications`  | `mdfind -name`    |
//! | Linux    | XDG `.desktop` files                | `locate -i`       |
//! | Windows  | `%ProgramFiles%` + Start Menu       | `where /r`        |

use std::process::Command;
use std::sync::OnceLock;
use tracing::debug;

use super::search::SearchResult;

/// Cached application list (loaded once on first search).
static APP_CACHE: OnceLock<Vec<AppEntry>> = OnceLock::new();

#[derive(Debug, Clone)]
struct AppEntry {
    name: String,
    path: String,
}

// ─── Public API ───────────────────────────────────────────────────────────────

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
                ..Default::default()
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(8);
    results
}

/// Search files with the fastest available indexed search tool.
pub fn search_files(query: &str) -> Vec<SearchResult> {
    if query.len() < 2 {
        return Vec::new();
    }
    search_files_impl(query)
}

/// Built-in system commands (OS-appropriate subset shown on each platform).
pub fn system_commands() -> Vec<SearchResult> {
    let mut cmds = vec![
        make_cmd("sys:lock", "Lock Screen", "Lock the screen immediately", "🔒"),
        make_cmd("sys:screenshot", "Screenshot", "Capture a screenshot", "📸"),
        make_cmd("sys:sleep", "Sleep", "Put the computer to sleep", "😴"),
        make_cmd("sys:show_desktop", "Show Desktop", "Minimize all windows", "🖥️"),
        make_cmd("sys:clipboard", "Show Clipboard", "View current clipboard contents", "📋"),
        make_cmd("sys:running_apps", "Running Apps", "List all currently running applications", "📊"),
        make_cmd("sys:system_info", "System Info", "View system information", "ℹ️"),
        make_cmd("sys:dark_mode", "Toggle Dark Mode", "Switch between light and dark appearance", "🌗"),
    ];

    #[cfg(target_os = "macos")]
    {
        cmds.push(make_cmd("sys:dnd", "Do Not Disturb", "Toggle Do Not Disturb mode", "🔕"));
        cmds.push(make_cmd("sys:empty_trash", "Empty Trash", "Permanently delete items in Trash", "🗑️"));
    }
    #[cfg(target_os = "linux")]
    {
        cmds.push(make_cmd("sys:dnd", "Do Not Disturb", "Toggle notification banners (GNOME)", "🔕"));
        cmds.push(make_cmd("sys:empty_trash", "Empty Trash", "Empty the Trash via gio", "🗑️"));
    }
    #[cfg(target_os = "windows")]
    {
        cmds.push(make_cmd("sys:dnd", "Do Not Disturb", "Toggle notification banners", "🔕"));
        cmds.push(make_cmd("sys:empty_trash", "Empty Recycle Bin", "Permanently delete items in Recycle Bin", "🗑️"));
    }

    cmds
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_cmd(id: &str, title: &str, subtitle: &str, icon: &str) -> SearchResult {
    SearchResult {
        id: id.into(),
        title: title.into(),
        subtitle: subtitle.into(),
        category: "System".into(),
        icon: icon.into(),
        score: 0.0,
        ..Default::default()
    }
}

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
        "app" | "exe" | "deb" | "rpm" | "appimage" => "📱",
        "dmg" | "iso" => "💿",
        _ => "📄",
    }
    .into()
}

fn paths_to_results(paths: &[&str]) -> Vec<SearchResult> {
    paths
        .iter()
        .filter(|p| !p.is_empty())
        .take(12)
        .enumerate()
        .map(|(i, path)| {
            let sep = if cfg!(target_os = "windows") { '\\' } else { '/' };
            let filename = path.rsplit(sep).next().unwrap_or(path);
            let is_dir = std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false);
            SearchResult {
                id: format!("file:{}", path),
                title: filename.to_string(),
                subtitle: path.to_string(),
                category: if is_dir { "Folder".into() } else { "File".into() },
                icon: if is_dir { "📁".into() } else { file_icon(filename) },
                score: 0.8 - (i as f64 * 0.05),
                ..Default::default()
            }
        })
        .collect()
}

// ─── macOS backend ────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn load_applications() -> Vec<AppEntry> {
    let mut apps = Vec::new();
    let dirs = ["/Applications", "/System/Applications"];
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
                    apps.push(AppEntry { name, path: path.to_string_lossy().into() });
                }
            }
        }
    }
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
                    apps.push(AppEntry { name, path: path.to_string_lossy().into() });
                }
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    debug!(count = apps.len(), "macOS application index loaded");
    apps
}

#[cfg(target_os = "macos")]
fn search_files_impl(query: &str) -> Vec<SearchResult> {
    let output = match Command::new("mdfind").arg("-name").arg(query).output() {
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
    let paths: Vec<&str> = stdout.lines().collect();
    paths_to_results(&paths)
}

// ─── Linux backend ────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn load_applications() -> Vec<AppEntry> {
    let mut apps = Vec::new();
    let mut dirs = vec![
        std::path::PathBuf::from("/usr/share/applications"),
        std::path::PathBuf::from("/usr/local/share/applications"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(std::path::PathBuf::from(home).join(".local/share/applications"));
    }
    if let Ok(data_dirs) = std::env::var("XDG_DATA_DIRS") {
        for d in data_dirs.split(':') {
            dirs.push(std::path::PathBuf::from(d).join("applications"));
        }
    }
    for dir in &dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("desktop") {
                    if let Some(name) = parse_desktop_name(&path) {
                        apps.push(AppEntry { name, path: path.to_string_lossy().into() });
                    }
                }
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by_key(|a| a.name.to_lowercase());
    debug!(count = apps.len(), "Linux application index loaded");
    apps
}

/// Extract the `Name=` field from a `.desktop` file, skipping hidden entries.
#[cfg(target_os = "linux")]
fn parse_desktop_name(path: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    if content.lines().any(|l| l.trim() == "NoDisplay=true") {
        return None;
    }
    content
        .lines()
        .find(|l| l.starts_with("Name="))
        .and_then(|l| l.strip_prefix("Name="))
        .filter(|n| !n.is_empty())
        .map(|n| n.trim().to_string())
}

#[cfg(target_os = "linux")]
fn search_files_impl(query: &str) -> Vec<SearchResult> {
    // Prefer indexed `locate`; fall back to `find` in the home directory.
    if let Ok(out) = Command::new("locate").args(["-i", "-l", "20", query]).output() {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let paths: Vec<&str> = stdout.lines().collect();
            if !paths.is_empty() {
                return paths_to_results(&paths);
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home_str = home.to_string_lossy().into_owned();
        let pattern = format!("*{}*", query);
        if let Ok(o) = Command::new("find")
            .args([&home_str, "-maxdepth", "5", "-iname", &pattern])
            .output()
        {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let paths: Vec<&str> = stdout.lines().collect();
            return paths_to_results(&paths);
        }
    }
    Vec::new()
}

// ─── Windows backend ─────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn load_applications() -> Vec<AppEntry> {
    let mut apps = Vec::new();
    let program_dirs = [
        std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into()),
        std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into()),
    ];
    for dir in &program_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for app_dir in entries.flatten() {
                let app_path = app_dir.path();
                if app_path.is_dir() {
                    let name = app_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    if let Ok(exes) = std::fs::read_dir(&app_path) {
                        for exe in exes.flatten() {
                            let ep = exe.path();
                            if ep.extension().and_then(|e| e.to_str()) == Some("exe") {
                                apps.push(AppEntry { name: name.clone(), path: ep.to_string_lossy().into() });
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    // Start Menu shortcuts.
    let start_menus = [
        std::env::var("APPDATA")
            .map(|d| format!(r"{}\Microsoft\Windows\Start Menu\Programs", d))
            .unwrap_or_default(),
        r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs".into(),
    ];
    for sm in &start_menus {
        scan_lnk_dir(std::path::Path::new(sm), &mut apps);
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by_key(|a| a.name.to_lowercase());
    debug!(count = apps.len(), "Windows application index loaded");
    apps
}

#[cfg(target_os = "windows")]
fn scan_lnk_dir(dir: &std::path::Path, apps: &mut Vec<AppEntry>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_lnk_dir(&path, apps);
            } else if path.extension().and_then(|e| e.to_str()) == Some("lnk") {
                if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                    apps.push(AppEntry { name: name.to_string(), path: path.to_string_lossy().into() });
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn search_files_impl(query: &str) -> Vec<SearchResult> {
    let user_profile = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users".into());
    let pattern = format!("*{}*", query);
    if let Ok(o) = Command::new("where").args(["/r", &user_profile, &pattern]).output() {
        let stdout = String::from_utf8_lossy(&o.stdout);
        let paths: Vec<&str> = stdout.lines().collect();
        if !paths.is_empty() {
            return paths_to_results(&paths);
        }
    }
    Vec::new()
}

// ─── Unsupported fallback ─────────────────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn load_applications() -> Vec<AppEntry> {
    Vec::new()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn search_files_impl(_query: &str) -> Vec<SearchResult> {
    Vec::new()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_commands_are_well_formed() {
        let cmds = system_commands();
        assert!(!cmds.is_empty(), "expected a non-empty system command catalog");
        assert!(cmds.iter().all(|c| c.id.starts_with("sys:")), "all ids must be sys:");
        let mut ids: Vec<&str> = cmds.iter().map(|c| c.id.as_str()).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "system command ids must be unique");
        assert!(cmds.iter().all(|c| !c.title.is_empty()));
    }

    #[test]
    fn fuzzy_match_works() {
        assert!(fuzzy_match("ch", "chrome"));
        assert!(fuzzy_match("sf", "safari"));
        assert!(!fuzzy_match("zz", "safari"));
    }
}
