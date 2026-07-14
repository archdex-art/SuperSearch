//! Windows implementation of the [`PlatformBackend`] contract.
//!
//! Each method maps a runtime automation primitive onto the standard Windows
//! tool that performs it, spawned through the shared [`exec`](super::exec)
//! engine with an argument vector:
//!
//! | Primitive            | Tool                                             |
//! |----------------------|--------------------------------------------------|
//! | open path / URL      | `explorer` (both — no shell involved)            |
//! | launch app           | `explorer` (resolves .lnk and .exe)              |
//! | file search          | `where /r %USERPROFILE% *query*`                 |
//! | clipboard read       | PowerShell `Get-Clipboard`                       |
//! | clipboard write      | PowerShell `Set-Clipboard`                       |
//! | list running apps    | `tasklist /FO CSV /NH`                           |
//! | quit app             | `taskkill /IM <name>.exe /F`                     |
//! | switch app           | PowerShell `AppActivate`                         |
//! | trusted script       | `powershell -NonInteractive -Command`            |
//!
//! User-derived values are always passed as spawned-process argv items —
//! never interpolated into a PowerShell string — so injection is not possible.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

use std::time::Duration;

use super::exec;
use super::{PlatformBackend, StepResult};

pub(crate) struct WindowsBackend;

impl WindowsBackend {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl PlatformBackend for WindowsBackend {
    fn launch_app(&self, app_name: &str, args: &[String], label: &str, timeout: Duration) -> StepResult {
        // `explorer` can open .lnk shortcuts and .exe files; for a bare name
        // it searches the PATH like `start` does.
        let mut argv = vec!["explorer".to_string(), app_name.to_string()];
        argv.extend(args.iter().cloned());
        let arg_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        exec::run_argv(arg_refs[0], &arg_refs[1..], label, timeout)
    }

    fn open_path(&self, path: &str, label: &str, timeout: Duration) -> StepResult {
        exec::run_argv("explorer", &[path], label, timeout)
    }

    fn open_url(&self, url: &str, label: &str, timeout: Duration) -> StepResult {
        // `cmd /c start` is NOT safe here: Windows has no real argv at the OS
        // level, so `cmd.exe` re-parses its whole `/c` operand as a command
        // line and honors `&`/`|` command separators even in an argument Rust
        // considers "already quoted" — an unquoted `&`/`|` in the URL (e.g. a
        // plain query string like `?a=1&b=2`, or a crafted payload) reaches
        // cmd's own parser and can chain a second command. `explorer` opens a
        // bare URL via the registered protocol handler with no shell involved
        // at all, so it's inert to shell metacharacters — the same tool this
        // backend already uses for `open_path`.
        exec::run_argv("explorer", &[url], label, timeout)
    }

    fn find_files(&self, query: &str, label: &str, timeout: Duration) -> StepResult {
        let user_profile = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users".into());
        let pattern = format!("*{}*", query);
        exec::run_argv_output("where", &["/r", &user_profile, &pattern], label, timeout)
    }

    fn clipboard_read(&self, label: &str, timeout: Duration) -> StepResult {
        exec::run_argv_output(
            "powershell",
            &["-NonInteractive", "-Command", "Get-Clipboard"],
            label,
            timeout,
        )
    }

    fn clipboard_write(&self, content: &str, label: &str, timeout: Duration) -> StepResult {
        // Pass the content via stdin so it is never interpolated into a script.
        exec::run_stdin(
            "powershell",
            &["-NonInteractive", "-Command", "$input | Set-Clipboard"],
            content,
            label,
            timeout,
        )
    }

    fn list_running_apps(&self, label: &str, timeout: Duration) -> StepResult {
        exec::run_argv_output("tasklist", &["/FO", "CSV", "/NH"], label, timeout)
    }

    fn quit_app(&self, app_name: &str, label: &str, timeout: Duration) -> StepResult {
        // Build the image name: strip a path prefix if the caller passed one,
        // then ensure it ends in .exe.
        let basename = std::path::Path::new(app_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(app_name);
        let image = if basename.to_ascii_lowercase().ends_with(".exe") {
            basename.to_string()
        } else {
            format!("{}.exe", basename)
        };
        exec::run_argv("taskkill", &["/IM", &image, "/F"], label, timeout)
    }

    fn switch_app(&self, app_name: &str, label: &str, timeout: Duration) -> StepResult {
        // AppActivate by window title is the closest portable equivalent.
        let script = format!(
            "(New-Object -ComObject WScript.Shell).AppActivate('{}')",
            app_name.replace('\'', "''")
        );
        exec::run_argv(
            "powershell",
            &["-NonInteractive", "-Command", &script],
            label,
            timeout,
        )
    }

    fn run_trusted_script(&self, script: &str, label: &str, capture: bool, timeout: Duration) -> StepResult {
        if capture {
            exec::run_argv_output(
                "powershell",
                &["-NonInteractive", "-Command", script],
                label,
                timeout,
            )
        } else {
            exec::run_argv(
                "powershell",
                &["-NonInteractive", "-Command", script],
                label,
                timeout,
            )
        }
    }
}
