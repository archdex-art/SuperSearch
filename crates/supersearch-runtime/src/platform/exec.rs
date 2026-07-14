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
//! Every platform backend is compiled on every target, so these helpers are
//! always referenced and never dead regardless of which backend is selected.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;
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

    // Drain stdout/stderr on background threads so a child that fills its
    // pipe buffer (OS default is commonly 64 KiB) before exiting can never
    // block this thread — the poll loop below only calls `try_wait`, which
    // never reads the pipes, so an undrained child would otherwise block on
    // its own `write()` until the timeout kills it, silently discarding all
    // output that overflowed the buffer.
    let stdout_reader = child.stdout.take().map(spawn_reader);
    let stderr_reader = child.stderr.take().map(spawn_reader);

    // Feed stdin (if any) on a background thread for the same reason: a slow
    // child, or a payload bigger than the OS pipe buffer, must not block this
    // thread past the deadline below — that would defeat the entire purpose
    // of this function (bounding a hung helper process).
    if let Some(data) = input {
        if let Some(mut stdin) = child.stdin.take() {
            let data = data.to_owned();
            std::thread::spawn(move || {
                let _ = stdin.write_all(data.as_bytes());
                // `stdin` drops here, closing the pipe so the child sees EOF.
            });
        }
    }

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = join_reader(stdout_reader);
                let stderr = join_reader(stderr_reader);
                return finish(
                    Ok(std::process::Output { status, stdout, stderr }),
                    label,
                    capture,
                );
            }
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

/// Spawn a background thread that reads a pipe to completion, returning a
/// handle to join for the collected bytes. Used to drain a child's stdout /
/// stderr concurrently with the deadline-bounded poll loop in `run_command`.
fn spawn_reader(mut pipe: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        buf
    })
}

/// Join a reader thread spawned by [`spawn_reader`], discarding the bytes if
/// the thread panicked (never surfaced past output capture).
fn join_reader(handle: Option<JoinHandle<Vec<u8>>>) -> Vec<u8> {
    handle.and_then(|h| h.join().ok()).unwrap_or_default()
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
        // A long-running child, spelled per-OS (no `sleep` binary on Windows).
        #[cfg(not(windows))]
        let cmd = {
            let mut c = Command::new("sleep");
            c.arg("30");
            c
        };
        #[cfg(windows)]
        let cmd = {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
            c
        };
        let result = run_command(cmd, None, "sleep", false, Duration::from_millis(200));
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("Timed out"));
        assert!(start.elapsed() < Duration::from_secs(5), "did not honor the deadline");
    }

    #[test]
    fn large_output_does_not_block_on_a_full_pipe() {
        // Regression: stdout/stderr were only read *after* `try_wait` reported
        // exit, so a child writing more than the OS pipe buffer (commonly
        // 64 KiB) before exiting would block on its own `write()` — and since
        // the poll loop never drained the pipes, it would always stall for the
        // entire timeout and then be killed, silently discarding the output.
        #[cfg(not(windows))]
        let cmd = {
            let mut c = Command::new("sh");
            // ~200 KiB of stdout, well past a 64 KiB pipe buffer, then exit.
            c.args(["-c", "yes A | head -c 200000"]);
            c
        };
        #[cfg(windows)]
        let cmd = {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-Command", "'A' * 200000"]);
            c
        };
        let result = run_command(cmd, None, "big-output", true, Duration::from_secs(5));
        assert!(result.success, "process should complete well within the deadline: {:?}", result.error);
        assert!(result.output.len() >= 100_000, "expected large output to be captured, got {} bytes", result.output.len());
    }
}
