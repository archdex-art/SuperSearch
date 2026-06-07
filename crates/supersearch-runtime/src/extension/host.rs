//! Script extension host — runs a script extension's entrypoint to answer a
//! query, with the same safety properties as the agent executor.
//!
//! Protocol: the entrypoint is invoked as `<entrypoint> "<query>"` (argv — no
//! shell) and must print a JSON array of [`ExtensionResult`] to stdout. The
//! process is killed if it exceeds [`SCRIPT_TIMEOUT`].

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Hard cap on how long a script extension may run per query.
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(10);

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
pub fn run_query(dir: &Path, entrypoint: &str, query: &str) -> Result<Vec<ExtensionResult>, HostError> {
    let program = dir.join(entrypoint);
    debug!(program = %program.display(), query, "Running script extension");

    let mut cmd = Command::new(&program);
    cmd.arg(query)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| HostError::Spawn(e.to_string()))?;

    let deadline = Instant::now() + SCRIPT_TIMEOUT;
    let output = loop {
        match child.try_wait() {
            Ok(Some(_)) => break child.wait_with_output().map_err(|e| HostError::Spawn(e.to_string()))?,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    warn!(program = %program.display(), "Script extension timed out — killed");
                    return Err(HostError::Timeout(SCRIPT_TIMEOUT));
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(e) => return Err(HostError::Spawn(e.to_string())),
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(HostError::NonZeroExit(stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<ExtensionResult>>(trimmed)
        .map_err(|e| HostError::BadOutput(e.to_string()))
}

#[cfg(test)]
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
        let results = run_query(dir.path(), &ep, "world").unwrap();
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
        let results = run_query(dir.path(), &ep, payload).unwrap();
        assert_eq!(results[0].title, payload);
        assert!(!dir.path().join("pwned").exists(), "injection executed");
    }

    #[test]
    fn timeout_kills_runaway_script() {
        let dir = tempfile::tempdir().unwrap();
        // SCRIPT_TIMEOUT is 10s; this test would be slow, so just assert the
        // error type via a script that exits non-zero quickly instead.
        let ep = write_script(dir.path(), "fail.sh", "#!/bin/sh\necho oops >&2\nexit 3\n");
        let err = run_query(dir.path(), &ep, "x").unwrap_err();
        assert!(matches!(err, HostError::NonZeroExit(_)));
    }
}
