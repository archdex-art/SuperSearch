//! Action execution — dispatches palette selections through real OS automation.
//!
//! Platform-specific `sys:` handlers use mutually-exclusive `#[cfg(target_os)]`
//! early returns (one per OS); those can't collapse into a single tail
//! expression, so `needless_return` is allowed module-wide by design.
#![allow(clippy::needless_return)]

use std::process::Command;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tauri::command;
use tracing::info;

use crate::state::AppState;
use supersearch_runtime::agent::AgentController;
use supersearch_runtime::journal::writer::JournalSender;
use supersearch_runtime::journal::{EntryKind, JournalEntry};
use supersearch_runtime::platform::StepResult;

/// Upper bound for a single user-initiated OS action routed through the PAL.
const ACTION_TIMEOUT: Duration = Duration::from_secs(15);

/// Request payload from the frontend when a palette item is executed.
#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteActionRequest {
    pub action_id: String,
    /// Whether the caller held a modifier key (e.g. ⌘) when triggering the
    /// action. Part of the IPC contract; reserved for alternate-execution
    /// behavior (open-in-background, reveal-in-Finder) — not yet branched on.
    #[allow(dead_code)]
    pub with_meta: bool,
}

/// Structured result returned to the frontend after execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteActionResponse {
    pub action_id: String,
    pub acknowledged: bool,
    /// Whether the underlying OS call actually succeeded — distinct from
    /// `acknowledged` (which just means "we got a response object back").
    /// The frontend uses this, not `detail`'s ✓/✗ prefix, to decide whether
    /// to close the palette or surface an error; previously nothing read
    /// this outcome at all, so a failed `open`/`xdg-open` (bad path, no
    /// default handler, a sandboxed/offline volume, …) silently closed the
    /// palette with no feedback — indistinguishable from it doing nothing.
    pub success: bool,
    pub title: String,
    pub category: String,
    pub detail: String,
    pub backend: String,
}

/// Execute a palette action through real OS automation.
///
/// Runs on a blocking thread (`spawn_blocking`) rather than inline on the IPC
/// caller's thread: a plain (non-`async`) Tauri command runs synchronously on
/// the thread that delivers the IPC message — the WKWebView main thread on
/// macOS — so spawning `open`/`osascript`/… inline froze the whole window for
/// up to `ACTION_TIMEOUT` (15s) on every selection.
#[command]
pub async fn execute_action(
    request: ExecuteActionRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ExecuteActionResponse, String> {
    let agent = state.agent.clone();
    let boot_instant = state.boot_instant;
    let journal = state.journal_sender.clone();
    tokio::task::spawn_blocking(move || run_action(request, &agent, boot_instant, &journal))
        .await
        .map_err(|e| format!("action task panicked: {e}"))?
}

/// The actual (blocking, OS-touching) action pipeline. See `execute_action`'s
/// doc comment for why this is offloaded to a blocking thread.
fn run_action(
    request: ExecuteActionRequest,
    agent: &AgentController,
    boot_instant: Instant,
    journal: &JournalSender,
) -> Result<ExecuteActionResponse, String> {
    let action_id = &request.action_id;
    // Bound the payload; a well-formed action id is short (a prefix + a path,
    // app name, or command). Reject pathological input before doing any work.
    const MAX_ACTION_ID_LEN: usize = 4096;
    if action_id.is_empty() {
        return Err("Empty action id".into());
    }
    if action_id.len() > MAX_ACTION_ID_LEN {
        return Err(format!("Action id too long (max {} bytes)", MAX_ACTION_ID_LEN));
    }
    info!(action_id, "Executing action");

    let response = if let Some(app_path) = action_id.strip_prefix("app:") {
        // Launch application — through the PAL (macOS `open`, Linux `xdg-open`).
        pal_response(
            super::os_backend().open_path(app_path, "Launch App", ACTION_TIMEOUT),
            action_id,
            "Launch App",
            "Application",
        )
    } else if let Some(file_path) = action_id.strip_prefix("file:") {
        // Open file — through the PAL.
        pal_response(
            super::os_backend().open_path(file_path, "Open File", ACTION_TIMEOUT),
            action_id,
            "Open File",
            "File",
        )
    } else if let Some(cmd) = action_id.strip_prefix("terminal:") {
        open_terminal(cmd, action_id)?
    } else if let Some(appcmd) = action_id.strip_prefix("appcmd:") {
        let parts: Vec<&str> = appcmd.splitn(2, '|').collect();
        if parts.len() == 2 {
            let app_name = parts[0];
            let task = parts[1];
            send_app_command(app_name, task, action_id)?
        } else {
            return Err("Invalid appcmd format".into());
        }
    } else if let Some(query) = action_id.strip_prefix("agent:") {
        // Execute via agent.
        let agent_response = agent.process_query(query);
        ExecuteActionResponse {
            action_id: action_id.to_string(),
            acknowledged: true,
            success: true,
            title: agent_response.intent.clone(),
            category: "Agent".into(),
            detail: agent_response.summary.clone(),
            backend: "agent-controller".into(),
        }
    } else if action_id.starts_with("sys:") {
        execute_sys_command(action_id)?
    } else {
        return Err(format!("Unknown action type: {}", action_id));
    };

    // Journal the action execution.
    if let Ok(payload) = serde_json::to_vec(&response) {
        let journal_entry = JournalEntry::new(
            EntryKind::ToolCallResult,
            boot_instant.elapsed().as_nanos() as u64,
            "ui".into(),
            payload,
        );
        let _ = journal.send(journal_entry);
    }

    Ok(response)
}

/// Adapt a PAL [`StepResult`] into an `ExecuteActionResponse` for "do it"
/// actions (open/launch) where only success/failure matters, not captured
/// output.
fn pal_response(
    result: StepResult,
    action_id: &str,
    title: &str,
    category: &str,
) -> ExecuteActionResponse {
    ExecuteActionResponse {
        action_id: action_id.to_string(),
        acknowledged: true,
        success: result.success,
        title: title.to_string(),
        category: category.to_string(),
        detail: if result.success {
            format!("✓ {} completed", title)
        } else {
            format!("✗ {}: {}", title, result.error.unwrap_or_default())
        },
        backend: "pal".into(),
    }
}

/// Helper: spawn a program directly with an argument vector (no shell) and
/// return an `ExecuteActionResponse`. Preferred for any path carrying
/// user-derived data, since shell metacharacters are inert in argv form.
fn execute_argv(
    program: &str,
    args: &[&str],
    action_id: &str,
    title: &str,
    category: &str,
) -> Result<ExecuteActionResponse, String> {
    finalize(Command::new(program).args(args).output(), action_id, title, category)
}

/// Normalize a completed command into an `ExecuteActionResponse`.
fn finalize(
    output: std::io::Result<std::process::Output>,
    action_id: &str,
    title: &str,
    category: &str,
) -> Result<ExecuteActionResponse, String> {
    match output {
        Ok(output) => {
            let success = output.status.success();
            let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

            // Surface macOS accessibility permission errors clearly.
            if stderr.contains("not allowed to send keystrokes") || stderr.contains("1002") {
                stderr = "macOS Accessibility Permission Required — open System Settings \
                    > Privacy & Security > Accessibility and enable SuperSearch."
                    .to_string();
            }

            Ok(ExecuteActionResponse {
                action_id: action_id.to_string(),
                acknowledged: true,
                success,
                title: title.to_string(),
                category: category.to_string(),
                detail: if success {
                    format!("✓ {} completed", title)
                } else {
                    format!("✗ {}: {}", title, stderr)
                },
                backend: "os-automation".into(),
            })
        }
        Err(e) => Err(format!("Execution failed: {}", e)),
    }
}

// ─── Cross-platform sys: dispatch ────────────────────────────────────────────

/// Route `sys:*` commands to platform-specific implementations.
fn execute_sys_command(action_id: &str) -> Result<ExecuteActionResponse, String> {
    match action_id {
        "sys:clipboard" => {
            let r = super::os_backend().clipboard_read("Clipboard Contents", ACTION_TIMEOUT);
            let detail = if r.success && !r.output.is_empty() && !r.output.ends_with("(no output)") {
                r.output
            } else {
                "(clipboard is empty)".into()
            };
            Ok(ExecuteActionResponse {
                action_id: action_id.to_string(),
                acknowledged: true,
                success: r.success,
                title: "Clipboard Contents".into(),
                category: "System".into(),
                detail,
                backend: "pal".into(),
            })
        }
        "sys:running_apps" => {
            let r = super::os_backend().list_running_apps("Running Applications", ACTION_TIMEOUT);
            let detail = if r.success {
                r.output
            } else {
                format!("Unable to list apps: {}", r.error.unwrap_or_default())
            };
            Ok(ExecuteActionResponse {
                action_id: action_id.to_string(),
                acknowledged: true,
                success: r.success,
                title: "Running Applications".into(),
                category: "System".into(),
                detail,
                backend: "pal".into(),
            })
        }
        "sys:lock" => sys_lock(action_id),
        "sys:screenshot" => sys_screenshot(action_id),
        "sys:dnd" => sys_dnd(action_id),
        "sys:empty_trash" => sys_empty_trash(action_id),
        "sys:dark_mode" => sys_dark_mode(action_id),
        "sys:sleep" => sys_sleep(action_id),
        "sys:show_desktop" => sys_show_desktop(action_id),
        "sys:system_info" => sys_system_info(action_id),
        _ => Err(format!("Unknown system command: {}", action_id)),
    }
}

// ─── Per-command cross-platform stubs ────────────────────────────────────────

fn sys_lock(action_id: &str) -> Result<ExecuteActionResponse, String> {
    #[cfg(target_os = "macos")]
    // `pmset displaysleepnow` sleeps the display immediately and needs NO
    // Accessibility permission — unlike a synthesized Ctrl+Cmd+Q keystroke,
    // which silently fails until the app is trusted. With the default "require
    // password after sleep" Lock Screen setting (on for most users) this locks
    // the Mac. (The classic `CGSession -suspend` binary was removed in recent
    // macOS, so it is no longer a reliable target.)
    return execute_argv("pmset", &["displaysleepnow"], action_id, "Lock Screen", "System");
    #[cfg(target_os = "linux")]
    return execute_argv("loginctl", &["lock-session"], action_id, "Lock Screen", "System");
    #[cfg(target_os = "windows")]
    return execute_argv(
        "rundll32",
        &["user32.dll,LockWorkStation"],
        action_id, "Lock Screen", "System",
    );
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Lock Screen not supported on this platform".into())
}

fn sys_screenshot(action_id: &str) -> Result<ExecuteActionResponse, String> {
    #[cfg(target_os = "macos")]
    {
        // `~` is NOT expanded in an argv arg (no shell), so resolve $HOME here —
        // otherwise the capture lands in a literal "~/Desktop" folder, not the
        // real Desktop.
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let path = format!("{}/Desktop/screenshot.png", home);
        return execute_argv("screencapture", &["-i", path.as_str()], action_id, "Screenshot", "System");
    }
    #[cfg(target_os = "linux")]
    {
        // Try gnome-screenshot, fall back to scrot.
        if Command::new("gnome-screenshot").arg("--version").output().is_ok() {
            return execute_argv("gnome-screenshot", &["-i"], action_id, "Screenshot", "System");
        }
        return execute_argv(
            "scrot",
            &["--select", "--freeze", "%Y-%m-%d_screenshot.png"],
            action_id, "Screenshot", "System",
        );
    }
    #[cfg(target_os = "windows")]
    return execute_argv("SnippingTool.exe", &[], action_id, "Screenshot", "System");
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Screenshot not supported on this platform".into())
}

fn sys_dnd(action_id: &str) -> Result<ExecuteActionResponse, String> {
    #[cfg(target_os = "macos")]
    return execute_argv(
        "osascript",
        &["-e", r#"tell application "System Events" to keystroke "d" using {command down, shift down, option down}"#],
        action_id, "Do Not Disturb", "System",
    );
    #[cfg(target_os = "linux")]
    return execute_argv(
        "gsettings",
        &["set", "org.gnome.desktop.notifications", "show-banners", "false"],
        action_id, "Do Not Disturb", "System",
    );
    #[cfg(target_os = "windows")]
    return execute_argv(
        "powershell",
        &[
            "-NonInteractive", "-Command",
            r#"$path='HKCU:\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings';
               $val=(Get-ItemProperty -Path $path -Name 'NOC_GLOBAL_SETTING_ALLOW_TOASTS_ABOVE_LOCK' -EA SilentlyContinue);
               Set-ItemProperty -Path $path -Name 'NOC_GLOBAL_SETTING_ALLOW_TOASTS_ABOVE_LOCK' -Value (1 - [int]$val.NOC_GLOBAL_SETTING_ALLOW_TOASTS_ABOVE_LOCK) -Type DWORD"#,
        ],
        action_id, "Do Not Disturb", "System",
    );
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Do Not Disturb not supported on this platform".into())
}

fn sys_empty_trash(action_id: &str) -> Result<ExecuteActionResponse, String> {
    #[cfg(target_os = "macos")]
    {
        // Finder's `empty trash` AppleScript command raises "The operation
        // can't be completed. (-128)" whenever the Trash is already empty —
        // a longstanding, benign Finder quirk, not a real failure (verified:
        // emptying a populated Trash exits 0; emptying an already-empty one
        // always returns -128 regardless of permissions). Check the count
        // first so an empty Trash reads as success instead of a scary error.
        let count = Command::new("osascript")
            .args(["-e", r#"tell application "Finder" to count items in trash"#])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        if count.ok().as_deref() == Some("0") {
            return Ok(ExecuteActionResponse {
                action_id: action_id.to_string(),
                acknowledged: true,
                success: true,
                title: "Empty Trash".to_string(),
                category: "System".to_string(),
                detail: "✓ Trash is already empty".to_string(),
                backend: "os-automation".into(),
            });
        }
        return execute_argv(
            "osascript",
            &["-e", r#"tell application "Finder" to empty trash"#],
            action_id, "Empty Trash", "System",
        );
    }
    #[cfg(target_os = "linux")]
    return execute_argv("gio", &["trash", "--empty"], action_id, "Empty Trash", "System");
    #[cfg(target_os = "windows")]
    return execute_argv(
        "powershell",
        &["-NonInteractive", "-Command", "Clear-RecycleBin -Force -ErrorAction SilentlyContinue"],
        action_id, "Empty Trash", "System",
    );
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Empty Trash not supported on this platform".into())
}

fn sys_dark_mode(action_id: &str) -> Result<ExecuteActionResponse, String> {
    #[cfg(target_os = "macos")]
    return execute_argv(
        "osascript",
        &["-e", r#"tell app "System Events" to tell appearance preferences to set dark mode to not dark mode"#],
        action_id, "Toggle Dark Mode", "System",
    );
    #[cfg(target_os = "linux")]
    {
        // GNOME: toggle between 'default' (light) and 'prefer-dark'.
        let current = Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "color-scheme"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let next = if current.contains("prefer-dark") { "default" } else { "prefer-dark" };
        return execute_argv(
            "gsettings",
            &["set", "org.gnome.desktop.interface", "color-scheme", next],
            action_id, "Toggle Dark Mode", "System",
        );
    }
    #[cfg(target_os = "windows")]
    {
        let current = Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                   "/v", "AppsUseLightTheme"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let next = if current.contains("0x1") { "0" } else { "1" };
        return execute_argv(
            "reg",
            &["add", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
              "/v", "AppsUseLightTheme", "/t", "REG_DWORD", "/d", next, "/f"],
            action_id, "Toggle Dark Mode", "System",
        );
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Dark mode toggle not supported on this platform".into())
}

fn sys_sleep(action_id: &str) -> Result<ExecuteActionResponse, String> {
    #[cfg(target_os = "macos")]
    return execute_argv("pmset", &["sleepnow"], action_id, "Sleep", "System");
    #[cfg(target_os = "linux")]
    return execute_argv("systemctl", &["suspend"], action_id, "Sleep", "System");
    #[cfg(target_os = "windows")]
    return execute_argv(
        "rundll32",
        &["powrprof.dll,SetSuspendState", "0,1,0"],
        action_id, "Sleep", "System",
    );
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Sleep not supported on this platform".into())
}

fn sys_show_desktop(action_id: &str) -> Result<ExecuteActionResponse, String> {
    #[cfg(target_os = "macos")]
    return execute_argv(
        "osascript",
        &["-e", "tell application \"System Events\" to key code 103"],
        action_id, "Show Desktop", "System",
    );
    #[cfg(target_os = "linux")]
    return execute_argv("wmctrl", &["-k", "on"], action_id, "Show Desktop", "System");
    #[cfg(target_os = "windows")]
    return execute_argv(
        "powershell",
        &["-NonInteractive", "-Command",
          r#"(New-Object -ComObject Shell.Application).ToggleDesktop()"#],
        action_id, "Show Desktop", "System",
    );
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Show Desktop not supported on this platform".into())
}

fn sys_system_info(action_id: &str) -> Result<ExecuteActionResponse, String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        #[cfg(target_os = "macos")]
        let cmd = "sw_vers 2>/dev/null && echo '---' && uptime 2>/dev/null";
        #[cfg(target_os = "linux")]
        let cmd = "uname -a 2>/dev/null && echo '---' && (lsb_release -a 2>/dev/null || cat /etc/os-release 2>/dev/null) && echo '---' && uptime 2>/dev/null";
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "Unable to get system info".into());
        return Ok(ExecuteActionResponse {
            action_id: action_id.to_string(),
            acknowledged: true,
            success: true,
            title: "System Information".into(),
            category: "System".into(),
            detail: output,
            backend: "os-automation".into(),
        });
    }
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("systeminfo")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().take(20).collect::<Vec<_>>().join("\n"))
            .unwrap_or_else(|_| "Unable to get system info".into());
        return Ok(ExecuteActionResponse {
            action_id: action_id.to_string(),
            acknowledged: true,
            success: true,
            title: "System Information".into(),
            category: "System".into(),
            detail: output,
            backend: "os-automation".into(),
        });
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("System info not supported on this platform".into())
}

/// Open a terminal and run `cmd` in it.
fn open_terminal(cmd: &str, action_id: &str) -> Result<ExecuteActionResponse, String> {
    #[cfg(target_os = "macos")]
    return execute_argv(
        "osascript",
        &[
            // Each `-e` becomes one line of the compiled script, but a
            // `do script` statement split across `-e` boundaries *inside* a
            // `tell application "Terminal" ... end tell` block reliably
            // fails to compile ("Expected end of line but found "script"."),
            // even though the identical text run from a real .applescript
            // file compiles fine — an osascript `-e`-joining quirk, not an
            // AppleScript language issue. The single-line `tell ... to do
            // script` form sidesteps it entirely (verified against the
            // multi-line block, which fails 100% of the time here).
            "-e", "on run argv",
            "-e", "tell application \"Terminal\" to do script (item 1 of argv)",
            "-e", "tell application \"Terminal\" to activate",
            "-e", "end run",
            "--", cmd,
        ],
        action_id, "Terminal Command", "System",
    );
    #[cfg(target_os = "linux")]
    {
        // Try common terminal emulators in preference order.
        for term in &["x-terminal-emulator", "gnome-terminal", "xterm", "konsole"] {
            if Command::new(term).arg("--version").output().is_ok() {
                let args: &[&str] = if *term == "gnome-terminal" {
                    &["--", "sh", "-c", cmd]
                } else {
                    &["-e", cmd]
                };
                return execute_argv(term, args, action_id, "Terminal Command", "System");
            }
        }
        Err("No terminal emulator found (tried x-terminal-emulator, gnome-terminal, xterm, konsole)".into())
    }
    #[cfg(target_os = "windows")]
    return execute_argv(
        "cmd",
        &["/c", "start", "cmd", "/k", cmd],
        action_id, "Terminal Command", "System",
    );
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Terminal command not supported on this platform".into())
}

/// Send a typed command to a running application.
fn send_app_command(app_name: &str, task: &str, action_id: &str) -> Result<ExecuteActionResponse, String> {
    #[cfg(target_os = "macos")]
    return execute_argv(
        "osascript",
        &[
            "-e", "on run argv",
            "-e", "set appName to item 1 of argv",
            "-e", "set taskText to item 2 of argv",
            "-e", "set wasRunning to application appName is running",
            "-e", "tell application appName to activate",
            "-e", "if not wasRunning then",
            "-e", "delay 2.5",
            "-e", "else",
            "-e", "delay 1.0",
            "-e", "end if",
            "-e", "tell application \"System Events\" to keystroke taskText",
            "-e", "tell application \"System Events\" to key code 36",
            "-e", "end run",
            "--", app_name, task,
        ],
        action_id,
        &format!("Command in {}", app_name),
        "System",
    );
    #[cfg(not(target_os = "macos"))]
    {
        // Keystroke injection into arbitrary apps requires platform-specific
        // accessibility APIs (AppleScript on macOS, AT-SPI on Linux, UIAutomation
        // on Windows). Only macOS is wired up today.
        let _ = (app_name, task, action_id);
        Err(format!(
            "appcmd: keystroke injection into '{}' is only supported on macOS",
            app_name
        ))
    }
}