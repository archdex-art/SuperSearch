//! Action execution — dispatches palette selections through real OS automation.

use std::process::Command;
use serde::{Deserialize, Serialize};
use tauri::command;
use tracing::info;

use crate::state::AppState;
use supersearch_runtime::journal::{EntryKind, JournalEntry};

/// Request payload from the frontend when a palette item is executed.
#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteActionRequest {
    pub action_id: String,
    pub with_meta: bool,
}

/// Structured result returned to the frontend after execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteActionResponse {
    pub action_id: String,
    pub acknowledged: bool,
    pub title: String,
    pub category: String,
    pub detail: String,
    pub backend: String,
}

/// Execute a palette action through real OS automation.
#[command]
pub fn execute_action(
    request: ExecuteActionRequest,
    state: tauri::State<'_, AppState>,
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

    let response = if action_id.starts_with("app:") {
        // Launch application.
        let app_path = &action_id[4..];
        execute_argv(
            "open",
            &["--", app_path],
            action_id,
            "Launch App",
            "Application",
        )?
    } else if action_id.starts_with("file:") {
        // Open file.
        let file_path = &action_id[5..];
        execute_argv(
            "open",
            &["--", file_path],
            action_id,
            "Open File",
            "File",
        )?
    } else if action_id.starts_with("terminal:") {
        // Open terminal and execute command. The command is passed as an
        // AppleScript argv item, so it cannot break out of the script string.
        let cmd = &action_id[9..];
        execute_argv(
            "osascript",
            &[
                "-e", "on run argv",
                "-e", "tell application \"Terminal\"",
                "-e", "do script (item 1 of argv)",
                "-e", "activate",
                "-e", "end tell",
                "-e", "end run",
                "--", cmd,
            ],
            action_id,
            "Terminal Command",
            "System",
        )?
    } else if action_id.starts_with("appcmd:") {
        let parts: Vec<&str> = action_id[7..].splitn(2, '|').collect();
        if parts.len() == 2 {
            let app_name = parts[0];
            let task = parts[1];
            // Both the app name and the keystroke text are passed as AppleScript
            // argv items (item 1 / item 2), never interpolated into the script
            // source — closing the shell/AppleScript injection hole.
            execute_argv(
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
                    "-e", "delay 0.5",
                    "-e", "end if",
                    "-e", "tell application \"System Events\" to keystroke taskText",
                    "-e", "tell application \"System Events\" to key code 36",
                    "-e", "end run",
                    "--", app_name, task,
                ],
                action_id,
                &format!("Command in {}", app_name),
                "System",
            )?
        } else {
            return Err("Invalid appcmd format".into());
        }
    } else if action_id.starts_with("agent:") {
        // Execute via agent.
        let query = &action_id[6..];
        let agent_response = state.agent.process_query(query);
        ExecuteActionResponse {
            action_id: action_id.to_string(),
            acknowledged: true,
            title: agent_response.intent.clone(),
            category: "Agent".into(),
            detail: agent_response.summary.clone(),
            backend: "agent-controller".into(),
        }
    } else if action_id.starts_with("sys:") {
        // System commands.
        match action_id.as_str() {
            "sys:lock" => execute_shell(
                r#"osascript -e 'tell application "System Events" to keystroke "q" using {command down, control down}'"#,
                action_id, "Lock Screen", "System",
            )?,
            "sys:screenshot" => execute_shell(
                "screencapture -i ~/Desktop/screenshot.png",
                action_id, "Screenshot", "System",
            )?,
            "sys:dnd" => execute_shell(
                r#"osascript -e 'tell application "System Events" to keystroke "d" using {command down, shift down, option down}'"#,
                action_id, "Do Not Disturb", "System",
            )?,
            "sys:empty_trash" => execute_shell(
                r#"osascript -e 'tell application "Finder" to empty trash'"#,
                action_id, "Empty Trash", "System",
            )?,
            "sys:dark_mode" => execute_shell(
                r#"osascript -e 'tell app "System Events" to tell appearance preferences to set dark mode to not dark mode'"#,
                action_id, "Toggle Dark Mode", "System",
            )?,
            "sys:sleep" => execute_shell(
                "pmset sleepnow",
                action_id, "Sleep", "System",
            )?,
            "sys:show_desktop" => execute_shell(
                r#"osascript -e 'tell application "System Events" to key code 103'"#,
                action_id, "Show Desktop", "System",
            )?,
            "sys:clipboard" => {
                let output = Command::new("pbpaste")
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|_| "(empty)".into());
                ExecuteActionResponse {
                    action_id: action_id.to_string(),
                    acknowledged: true,
                    title: "Clipboard Contents".into(),
                    category: "System".into(),
                    detail: if output.is_empty() { "(clipboard is empty)".into() } else { output },
                    backend: "os-automation".into(),
                }
            }
            "sys:running_apps" => {
                let script = r#"osascript -e 'tell application "System Events" to get name of every process whose background only is false'"#;
                let output = Command::new("sh")
                    .arg("-c")
                    .arg(script)
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|_| "Unable to list apps".into());
                ExecuteActionResponse {
                    action_id: action_id.to_string(),
                    acknowledged: true,
                    title: "Running Applications".into(),
                    category: "System".into(),
                    detail: output,
                    backend: "os-automation".into(),
                }
            }
            "sys:system_info" => {
                let output = Command::new("sh")
                    .arg("-c")
                    .arg("sw_vers 2>/dev/null && echo '---' && uptime 2>/dev/null")
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|_| "Unable to get system info".into());
                ExecuteActionResponse {
                    action_id: action_id.to_string(),
                    acknowledged: true,
                    title: "System Information".into(),
                    category: "System".into(),
                    detail: output,
                    backend: "os-automation".into(),
                }
            }
            _ => {
                return Err(format!("Unknown system command: {}", action_id));
            }
        }
    } else {
        return Err(format!("Unknown action type: {}", action_id));
    };

    // Journal the action execution.
    if let Ok(payload) = serde_json::to_vec(&response) {
        let journal_entry = JournalEntry::new(
            EntryKind::ToolCallResult,
            state.boot_instant.elapsed().as_nanos() as u64,
            "ui".into(),
            payload,
        );
        let _ = state.journal_sender.send(journal_entry);
    }

    Ok(response)
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

/// Helper: run a trusted, constant shell command and return an
/// `ExecuteActionResponse`. Only used for the fixed `sys:` commands — never
/// with interpolated user input.
fn execute_shell(
    cmd: &str,
    action_id: &str,
    title: &str,
    category: &str,
) -> Result<ExecuteActionResponse, String> {
    finalize(Command::new("sh").arg("-c").arg(cmd).output(), action_id, title, category)
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
            
            // Check for macOS accessibility permission errors
            if stderr.contains("not allowed to send keystrokes") || stderr.contains("1002") {
                stderr = "macOS Accessibility Permission Required! Please open System Settings > Privacy & Security > Accessibility and enable access for your Terminal/IDE to allow SuperSearch to send keystrokes.".to_string();
            }

            Ok(ExecuteActionResponse {
                action_id: action_id.to_string(),
                acknowledged: true,
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