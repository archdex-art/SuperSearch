//! Cross-platform process-spawning engine shared by every platform backend.
//!
//! This is the single point where the runtime turns an argument vector — or a
//! trusted, planner-generated constant shell script — into a spawned child
//! process, bounds it with a deadline, and normalizes the outcome into a
//! [`StepResult`]. Keeping it here, rather than inside any one OS backend,
//! guarantees that spawn, timeout, and result-normalization semantics are
//! identical on every operating system, which the cross-platform execution
//! contract requires.
//!
//! ## Security
//! Callers spawn the target binary directly with an argument vector so that
//! shell metacharacters (`;`, `|`, `$()`, backticks, quotes) in user-derived
//! data are inert. The `run_shell*` helpers exist only for the trusted constant
//! scripts the planner emits — they never interpolate user input.
//!
//! On macOS-only CI this whole module is exercised; on other targets the macOS
//! backend is compiled out and these helpers are dead until a backend for that
//! OS is written, hence the targeted `allow(dead_code)`.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tracing::{debug, error};

use super::StepResult;

/// Spawn a program directly with an argument vector (no shell).
pub(crate) fn run_argv(program: &str, args: &[&str], label: &str, timeout: Duration) -> StepResult {
    debug!(program, ?args, label, "Executing argv command");
    let mut cmd = Command::new(program);
    cmd.args(args);
    run_command(cmd, None, label, false, timeout)
}

/// Spawn a program directly and capture its stdout.
pub(crate) fn run_argv_output(
    program: &str,
    args: &[&str],
    label: &str,
    timeout: Duration,
) -> StepResult {
    debug!(program, ?args, label, "Executing argv command (capturing output)");
    let mut cmd = Command::new(program);
    cmd.args(args);
    run_command(cmd, None, label, true, timeout)
}

/// Spawn a program directly, writing `input` to its stdin.
pub(crate) fn run_stdin(
    program: &str,
    args: &[&str],
    input: &str,
    label: &str,
    timeout: Duration,
) -> StepResult {
    debug!(program, label, "Executing argv command with stdin");
    let mut cmd = Command::new(program);
    cmd.args(args);
    run_command(cmd, Some(input), label, false, timeout)
}

/// Run a trusted, constant shell command, returning success/failure.
///
/// Only used for planner-generated constant scripts — never user input.
pub(crate) fn run_shell(cmd: &str, label: &str, timeout: Duration) -> StepResult {
    debug!(cmd, label, "Executing trusted shell command");
    let mut command = Command::new("sh");
    command.arg("-c").arg(cmd);
    run_command(command, None, label, false, timeout)
}

/// Run a trusted, constant shell command and capture its stdout.
pub(crate) fn run_shell_with_output(cmd: &str, label: &str, timeout: Duration) -> StepResult {
    debug!(cmd, label, "Executing trusted shell command (capturing output)");
    let mut command = Command::new("sh");
    command.arg("-c").arg(cmd);
    run_command(command, None, label, true, timeout)
}

/// Spawn `cmd`, optionally feeding `input` to its stdin, and wait for it to
/// exit — but never longer than `timeout`. A process that overruns the deadline
/// is killed so a hung helper (`osascript`/`open`/…) can't wedge the caller
/// (which, in the app, is the synchronous IPC thread).
pub(crate) fn run_command(
    mut cmd: Command,
    input: Option<&str>,
    label: &str,
    capture: bool,
    timeout: Duration,
) -> StepResult {
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return err(label, e),
    };

    // Write stdin (if any) and close it so the child observes EOF.
    if let Some(data) = input {
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(data.as_bytes()) {
                let _ = child.kill();
                let _ = child.wait();
                return err(label, e);
            }
        }
    }

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => return finish(child.wait_with_output(), label, capture),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    error!(label, timeout_ms = timeout.as_millis() as u64, "Action timed out — killed");
                    return StepResult {
                        node_id: 0,
                        label: label.to_string(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("Timed out after {:?}", timeout)),
                    };
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(e) => return err(label, e),
        }
    }
}

/// Normalize a completed `Command::output()` into a [`StepResult`].
fn finish(
    output: std::io::Result<std::process::Output>,
    label: &str,
    capture: bool,
) -> StepResult {
    match output {
        Ok(output) => {
            let success = output.status.success();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let out = if capture {
                if stdout.is_empty() && success {
                    format!("✓ {} (no output)", label)
                } else {
                    stdout
                }
            } else if success {
                format!("✓ {}", label)
            } else {
                format!("✗ {}: {}", label, stderr)
            };
            StepResult {
                node_id: 0,
                label: label.to_string(),
                success,
                output: out,
                error: if success { None } else { Some(stderr) },
            }
        }
        Err(e) => err(label, e),
    }
}

/// Build a failure [`StepResult`] from a spawn/IO error.
pub(crate) fn err(label: &str, e: impl std::fmt::Display) -> StepResult {
    StepResult {
        node_id: 0,
        label: label.to_string(),
        success: false,
        output: String::new(),
        error: Some(format!("Failed to execute: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_running_action_is_killed_at_deadline() {
        // A process that outlives the deadline must be killed and reported as a
        // timeout rather than blocking the caller indefinitely.
        let start = Instant::now();
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let result = run_command(cmd, None, "sleep", false, Duration::from_millis(200));
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("Timed out"));
        assert!(start.elapsed() < Duration::from_secs(5), "did not honor the deadline");
    }
}
