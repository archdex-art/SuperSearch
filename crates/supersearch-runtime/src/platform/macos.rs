//! macOS implementation of the [`PlatformBackend`] contract.
//!
//! Every method maps a runtime automation primitive onto the native macOS tool
//! that performs it — `open`, `mdfind`, `osascript`, `pbcopy`/`pbpaste` — by
//! spawning it through the shared [`exec`](super::exec) engine with an argument
//! vector. User-derived values (paths, URLs, app names, clipboard contents) are
//! always passed as argv items or AppleScript `on run argv` arguments, never
//! interpolated into a shell string or script source, so shell/AppleScript
//! metacharacters are inert.
//!
//! This is the only module in the runtime that knows these macOS tool names
//! exist. It is compiled on every target but only *selected* on macOS; on other
//! hosts it is the inactive alternative, hence the module-level `allow`.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::time::Duration;

use super::exec;
use super::{PlatformBackend, StepResult};

/// The macOS automation backend.
pub(crate) struct MacosBackend;

impl MacosBackend {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl PlatformBackend for MacosBackend {
    fn launch_app(
        &self,
        app_name: &str,
        args: &[String],
        label: &str,
        timeout: Duration,
    ) -> StepResult {
        let argv: Vec<String> = if args.is_empty() {
            vec!["-a".into(), app_name.to_string()]
        } else {
            let mut v = vec!["-n".into(), "-a".into(), app_name.to_string(), "--args".into()];
            v.extend(args.iter().cloned());
            v
        };
        let arg_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        exec::run_argv("open", &arg_refs, label, timeout)
    }

    fn open_path(&self, path: &str, label: &str, timeout: Duration) -> StepResult {
        // `--` stops `open` from treating a leading-dash path as a flag.
        exec::run_argv("open", &["--", path], label, timeout)
    }

    fn open_url(&self, url: &str, label: &str, timeout: Duration) -> StepResult {
        exec::run_argv("open", &["--", url], label, timeout)
    }

    fn find_files(&self, query: &str, label: &str, timeout: Duration) -> StepResult {
        // Spotlight search via argv (no shell).
        exec::run_argv_output("mdfind", &["-name", query], label, timeout)
    }

    fn clipboard_read(&self, label: &str, timeout: Duration) -> StepResult {
        exec::run_argv_output("pbpaste", &[], label, timeout)
    }

    fn clipboard_write(&self, content: &str, label: &str, timeout: Duration) -> StepResult {
        exec::run_stdin("pbcopy", &[], content, label, timeout)
    }

    fn list_running_apps(&self, label: &str, timeout: Duration) -> StepResult {
        exec::run_argv_output(
            "osascript",
            &["-e", "tell application \"System Events\" to get name of every process whose background only is false"],
            label,
            timeout,
        )
    }

    fn quit_app(&self, app_name: &str, label: &str, timeout: Duration) -> StepResult {
        // App name passed as an AppleScript argv item — not interpolated into
        // the script source — so it cannot break out of the string.
        exec::run_argv(
            "osascript",
            &["-e", "on run argv", "-e", "tell application (item 1 of argv) to quit", "-e", "end run", "--", app_name],
            label,
            timeout,
        )
    }

    fn switch_app(&self, app_name: &str, label: &str, timeout: Duration) -> StepResult {
        exec::run_argv(
            "osascript",
            &["-e", "on run argv", "-e", "tell application (item 1 of argv) to activate", "-e", "end run", "--", app_name],
            label,
            timeout,
        )
    }

    fn run_trusted_script(
        &self,
        script: &str,
        label: &str,
        capture: bool,
        timeout: Duration,
    ) -> StepResult {
        if capture {
            exec::run_shell_with_output(script, label, timeout)
        } else {
            exec::run_shell(script, label, timeout)
        }
    }
}
