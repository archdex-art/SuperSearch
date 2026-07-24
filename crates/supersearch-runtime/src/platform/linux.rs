//! Linux implementation of the [`PlatformBackend`] contract.
//!
//! Each method maps a runtime automation primitive onto the standard Linux tool
//! that performs it, spawned through the shared [`exec`](super::exec) engine
//! with an argument vector:
//!
//! | Primitive            | Tool                                            |
//! |----------------------|-------------------------------------------------|
//! | open path / URL      | `xdg-open`                                       |
//! | launch app           | `gtk-launch` (XDG desktop entry)                |
//! | file search          | `locate -i`                                      |
//! | clipboard            | `wl-copy`/`wl-paste` (Wayland) or `xclip` (X11) |
//! | list / quit / switch | `wmctrl`                                          |
//! | trusted script       | `sh -c` (planner-generated constants only)       |
//!
//! As on macOS, user-derived values (paths, URLs, app names, clipboard content,
//! search queries) are always passed as argv items — never interpolated into a
//! shell string — so shell metacharacters are inert.
//!
//! Compiled on every target but only *selected* on Linux; the command builders
//! below are pure and unit-tested on any host. Clipboard ownership semantics
//! (X11/Wayland keep a process alive to serve the selection) and the presence
//! of each tool can only be exercised on a real Linux session — that is what the
//! Linux CI job is for.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::time::Duration;

use super::exec;
use super::{PlatformBackend, StepResult};

/// The clipboard tooling family for the current session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Clipboard {
    /// Wayland session — `wl-copy` / `wl-paste`.
    Wayland,
    /// X11 session (or unknown) — `xclip`.
    X11,
}

/// Pick the clipboard family from a `WAYLAND_DISPLAY`-style value. A non-empty
/// value means a Wayland session; anything else falls back to X11.
pub(crate) fn clipboard_kind(wayland_display: Option<&str>) -> Clipboard {
    match wayland_display {
        Some(v) if !v.is_empty() => Clipboard::Wayland,
        _ => Clipboard::X11,
    }
}

/// `(program, args)` to read the clipboard for a given family.
pub(crate) fn clipboard_read_cmd(kind: Clipboard) -> (&'static str, Vec<&'static str>) {
    match kind {
        Clipboard::Wayland => ("wl-paste", vec!["--no-newline"]),
        Clipboard::X11 => ("xclip", vec!["-selection", "clipboard", "-o"]),
    }
}

/// `(program, args)` to write the clipboard (content is fed via stdin).
pub(crate) fn clipboard_write_cmd(kind: Clipboard) -> (&'static str, Vec<&'static str>) {
    match kind {
        Clipboard::Wayland => ("wl-copy", vec![]),
        Clipboard::X11 => ("xclip", vec!["-selection", "clipboard"]),
    }
}

/// argv for launching an application via its XDG desktop entry. Extra `args`
/// (e.g. files/URIs to hand the app) are forwarded after the entry name.
pub(crate) fn launch_argv(app_name: &str, args: &[String]) -> Vec<String> {
    let mut v = vec![app_name.to_string()];
    v.extend(args.iter().cloned());
    v
}

/// The clipboard family for the live session (reads the environment).
fn session_clipboard() -> Clipboard {
    clipboard_kind(std::env::var("WAYLAND_DISPLAY").ok().as_deref())
}

/// The Linux automation backend.
pub(crate) struct LinuxBackend;

impl LinuxBackend {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl PlatformBackend for LinuxBackend {
    fn launch_app(
        &self,
        app_name: &str,
        args: &[String],
        label: &str,
        timeout: Duration,
    ) -> StepResult {
        let argv = launch_argv(app_name, args);
        let arg_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        exec::run_argv("gtk-launch", &arg_refs, label, timeout)
    }

    fn open_path(&self, path: &str, label: &str, timeout: Duration) -> StepResult {
        exec::run_argv("xdg-open", &[path], label, timeout)
    }

    fn open_url(&self, url: &str, label: &str, timeout: Duration) -> StepResult {
        exec::run_argv("xdg-open", &[url], label, timeout)
    }

    fn find_files(&self, query: &str, label: &str, timeout: Duration) -> StepResult {
        // Indexed filename search (case-insensitive substring), closest to
        // macOS `mdfind -name`. The executor caps result volume.
        exec::run_argv_output("locate", &["-i", query], label, timeout)
    }

    fn clipboard_read(&self, label: &str, timeout: Duration) -> StepResult {
        let (program, args) = clipboard_read_cmd(session_clipboard());
        exec::run_argv_output(program, &args, label, timeout)
    }

    fn clipboard_write(&self, content: &str, label: &str, timeout: Duration) -> StepResult {
        let (program, args) = clipboard_write_cmd(session_clipboard());
        exec::run_stdin(program, &args, content, label, timeout)
    }

    fn list_running_apps(&self, label: &str, timeout: Duration) -> StepResult {
        // `wmctrl -l` lists managed (user-visible) windows.
        exec::run_argv_output("wmctrl", &["-l"], label, timeout)
    }

    fn quit_app(&self, app_name: &str, label: &str, timeout: Duration) -> StepResult {
        // Gracefully close the window whose title matches (argv, not shell).
        exec::run_argv("wmctrl", &["-c", app_name], label, timeout)
    }

    fn switch_app(&self, app_name: &str, label: &str, timeout: Duration) -> StepResult {
        // Activate/raise the window whose title matches.
        exec::run_argv("wmctrl", &["-a", app_name], label, timeout)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_kind_detects_wayland_vs_x11() {
        assert_eq!(clipboard_kind(Some("wayland-0")), Clipboard::Wayland);
        // Empty value (set-but-blank) is not a live Wayland session.
        assert_eq!(clipboard_kind(Some("")), Clipboard::X11);
        assert_eq!(clipboard_kind(None), Clipboard::X11);
    }

    #[test]
    fn clipboard_commands_match_family() {
        assert_eq!(clipboard_read_cmd(Clipboard::Wayland).0, "wl-paste");
        assert_eq!(clipboard_write_cmd(Clipboard::Wayland).0, "wl-copy");

        let (read_prog, read_args) = clipboard_read_cmd(Clipboard::X11);
        assert_eq!(read_prog, "xclip");
        assert!(
            read_args.contains(&"-o"),
            "X11 read must request output mode"
        );

        let (write_prog, write_args) = clipboard_write_cmd(Clipboard::X11);
        assert_eq!(write_prog, "xclip");
        assert!(
            !write_args.contains(&"-o"),
            "X11 write must not be output mode"
        );
    }

    #[test]
    fn launch_argv_forwards_app_then_args() {
        assert_eq!(launch_argv("firefox", &[]), vec!["firefox"]);
        assert_eq!(
            launch_argv("firefox", &["https://example.com".into()]),
            vec!["firefox", "https://example.com"]
        );
    }

    #[test]
    fn launch_argv_keeps_untrusted_app_name_as_a_single_argv_item() {
        // A malicious "app name" stays one argv element — it can never become a
        // second command, because nothing here touches a shell.
        let argv = launch_argv("evil; rm -rf ~", &[]);
        assert_eq!(argv, vec!["evil; rm -rf ~"]);
        assert_eq!(argv.len(), 1);
    }
}
