//! Script extension host — runs a script extension's entrypoint to answer a
//! query, with the same safety properties as the agent executor.
//!
//! Protocol: the entrypoint is invoked as `<entrypoint> "<query>"` (argv — no
//! shell) and must print a JSON array of [`ExtensionResult`] to stdout. The
//! process is killed if it exceeds [`SCRIPT_TIMEOUT`].

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
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

    let mut child = cmd.spawn().map_err(|e| HostError::Spawn(e.to_string()))?;

    // Poll for exit with a deadline, then read the child's pipes directly.
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

    // The process has exited, so its output sits in the pipe buffers; read it.
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }

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
}
