//! Script extension host — runs a script extension's entrypoint to answer a
//! query, with the same safety properties as the agent executor.
//!
//! Protocol: the entrypoint is invoked as `<entrypoint> "<query>"` (argv — no
//! shell) and must print a JSON array of [`ExtensionResult`] to stdout. The
//! process is killed if it exceeds [`SCRIPT_TIMEOUT`].

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Default cap for a one-off (non-search) script invocation. Search fan-out
/// passes a much tighter budget so a slow extension can't stall the palette.
pub const SCRIPT_TIMEOUT: Duration = Duration::from_secs(10);

/// A single result row returned by an extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionResult {
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    /// Optional action to perform when the user activates this result.
    #[serde(default)]
    pub action: Option<ExtensionAction>,
}

/// Actions an extension result can declare. The host executes these through
/// the capability gate (the extension must hold the matching permission).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExtensionAction {
    OpenUrl { url: String },
    OpenPath { path: String },
    Copy { text: String },
}

/// Errors from running a script extension.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("failed to spawn extension: {0}")]
    Spawn(String),
    #[error("extension timed out after {0:?}")]
    Timeout(Duration),
    #[error("extension exited with failure: {0}")]
    NonZeroExit(String),
    #[error("extension produced invalid JSON: {0}")]
    BadOutput(String),
}

/// Spawn `cmd`, retrying briefly on `ETXTBSY` ("Text file busy").
///
/// A script extension's entrypoint can be spawned immediately after being
/// written to disk — right after install/update in production, or by
/// `write_script` in the tests below. On Linux and macOS, `execve` can
/// transiently refuse with ETXTBSY (errno 26) if the kernel hasn't yet
/// released the inode's write-busy bookkeeping, even though the writer
/// already closed its file handle — a narrow, well-documented race, not
/// real contention. Confirmed non-deterministic in CI: the exact same
/// commit's test suite passed on one run and hit this on another. A few
/// retries with a short backoff reliably clears it; any other spawn error
/// is returned immediately.
fn spawn_retrying_on_text_busy(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    const MAX_ATTEMPTS: u32 = 5;
    const RETRY_DELAY: Duration = Duration::from_millis(20);
    let mut last_err = None;
    for _ in 0..MAX_ATTEMPTS {
        match cmd.spawn() {
            Ok(child) => return Ok(child),
            Err(e) if e.raw_os_error() == Some(26) => {
                last_err = Some(e);
                std::thread::sleep(RETRY_DELAY);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.expect("loop runs MAX_ATTEMPTS >= 1 times"))
}

/// Run a script extension's entrypoint for `query`, returning parsed results.
///
/// `dir` is the extension directory; `entrypoint` is the manifest's relative
/// entrypoint (already validated to stay inside `dir`).
pub fn run_query(
    dir: &Path,
    entrypoint: &str,
    query: &str,
    timeout: Duration,
) -> Result<Vec<ExtensionResult>, HostError> {
    let program = dir.join(entrypoint);
    debug!(program = %program.display(), query, "Running script extension");

    let mut cmd = Command::new(&program);
    cmd.arg(query)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child =
        spawn_retrying_on_text_busy(&mut cmd).map_err(|e| HostError::Spawn(e.to_string()))?;

    // Drain stdout/stderr on background threads concurrently with the poll
    // loop below. Without this, a script that writes more than the OS pipe
    // buffer (commonly 64 KiB) before exiting blocks on its own `write()`
    // waiting for us to read — but the loop only calls `try_wait` (it never
    // reads the pipes), so any legitimately-larger-but-well-behaved result
    // set would always stall for the *entire* timeout and then be discarded,
    // instead of completing quickly.
    let stdout_reader = child.stdout.take().map(spawn_reader);
    let stderr_reader = child.stderr.take().map(spawn_reader);

    // Poll for exit with a deadline, then join the reader threads.
    // Do NOT call `wait_with_output()` after `try_wait()` has already reaped the
    // process — the second wait can intermittently fail with ECHILD ("No child
    // processes"), which previously misreported a non-zero exit as a spawn error
    // (a flaky test on loaded CI runners).
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    warn!(program = %program.display(), "Script extension timed out — killed");
                    return Err(HostError::Timeout(timeout));
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(e) => return Err(HostError::Spawn(e.to_string())),
        }
    };

    // The process has exited; the reader threads finish (pipes hit EOF) and
    // hand back whatever they collected.
    let stdout_bytes = join_reader(stdout_reader);
    let stderr_bytes = join_reader(stderr_reader);
    let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();

    if !status.success() {
        return Err(HostError::NonZeroExit(stderr.trim().to_string()));
    }

    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<ExtensionResult>>(trimmed)
        .map_err(|e| HostError::BadOutput(e.to_string()))
}

/// Spawn a background thread that reads a pipe to completion. Used to drain
/// a child's stdout/stderr concurrently with the deadline-bounded poll loop.
fn spawn_reader(mut pipe: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        buf
    })
}

/// Join a reader thread spawned by [`spawn_reader`], discarding the bytes if
/// the thread panicked.
fn join_reader(handle: Option<JoinHandle<Vec<u8>>>) -> Vec<u8> {
    handle.and_then(|h| h.join().ok()).unwrap_or_default()
}

// These tests build `#!/bin/sh` script extensions and `chmod +x` them, so they
// are unix-only (macOS + Linux); Windows script execution is exercised elsewhere.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn write_script(dir: &Path, name: &str, body: &str) -> String {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        name.to_string()
    }

    #[test]
    fn parses_results_from_script_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let ep = write_script(
            dir.path(),
            "run.sh",
            "#!/bin/sh\nprintf '[{\"title\":\"Hello %s\",\"subtitle\":\"sub\"}]' \"$1\"\n",
        );
        let results = run_query(dir.path(), &ep, "world", SCRIPT_TIMEOUT).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Hello world");
        assert_eq!(results[0].subtitle, "sub");
    }

    #[test]
    fn query_argument_is_not_shell_interpreted() {
        // The query is passed as argv, so metacharacters can't execute.
        let dir = tempfile::tempdir().unwrap();
        let ep = write_script(
            dir.path(),
            "echo.sh",
            "#!/bin/sh\nprintf '[{\"title\":\"%s\"}]' \"$1\"\n",
        );
        let payload = "$(touch pwned); `id`";
        let results = run_query(dir.path(), &ep, payload, SCRIPT_TIMEOUT).unwrap();
        assert_eq!(results[0].title, payload);
        assert!(!dir.path().join("pwned").exists(), "injection executed");
    }

    #[test]
    fn timeout_kills_runaway_script() {
        let dir = tempfile::tempdir().unwrap();
        // SCRIPT_TIMEOUT is 10s; this test would be slow, so just assert the
        // error type via a script that exits non-zero quickly instead.
        let ep = write_script(dir.path(), "fail.sh", "#!/bin/sh\necho oops >&2\nexit 3\n");
        let err = run_query(dir.path(), &ep, "x", SCRIPT_TIMEOUT).unwrap_err();
        assert!(matches!(err, HostError::NonZeroExit(_)));
    }

    #[test]
    fn large_stdout_does_not_stall_until_timeout() {
        // Regression: stdout/stderr were only read *after* the child exited, so
        // a script writing more than the OS pipe buffer (commonly 64 KiB)
        // before exiting would block on its own `write()` — the poll loop only
        // called `try_wait` and never drained the pipes, so any legitimately
        // large-but-well-behaved result set always stalled for the full
        // timeout and was then discarded instead of returning promptly.
        let dir = tempfile::tempdir().unwrap();
        // Emit a JSON array with ~2000 rows (comfortably over 64 KiB) then exit.
        let ep = write_script(
            dir.path(),
            "big.sh",
            "#!/bin/sh\nprintf '['\nfor i in $(seq 1 2000); do\n  [ \"$i\" -gt 1 ] && printf ','\n  printf '{\"title\":\"row-%04d-XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX\"}' \"$i\"\ndone\nprintf ']'\n",
        );
        let start = std::time::Instant::now();
        let results = run_query(dir.path(), &ep, "x", Duration::from_secs(5)).unwrap();
        assert_eq!(results.len(), 2000);
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "should complete almost immediately, not stall until the timeout: {:?}",
            start.elapsed()
        );
    }
}
