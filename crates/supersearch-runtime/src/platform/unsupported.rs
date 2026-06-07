//! Fallback [`PlatformBackend`] for operating systems without a native backend.
//!
//! The cross-platform contract requires that the runtime *compile and behave
//! deterministically* on every target — not that every target is feature
//! complete. On an OS we have not yet ported automation to, every primitive
//! must fail loudly and identically rather than silently doing nothing or
//! spawning a tool that does not exist. This backend provides exactly that: a
//! uniform, auditable "not supported on this platform" failure that the
//! capability gate and journal record like any other outcome.
//!
//! macOS and Linux now have real backends; this covers every *other* target
//! (e.g. Windows) until one is written. Adding support means a sibling module
//! that implements [`PlatformBackend`] and one arm in
//! [`default_backend`](super::default_backend) — nothing in the executor, the
//! capability gate, or the journal changes. Compiled everywhere but only
//! selected off macOS/Linux, hence the module-level `allow`.
#![cfg_attr(any(target_os = "macos", target_os = "linux"), allow(dead_code))]

use std::time::Duration;

use super::{PlatformBackend, StepResult};

/// A backend that refuses every operation with a clear platform error.
pub(crate) struct UnsupportedBackend;

impl UnsupportedBackend {
    pub(crate) fn new() -> Self {
        Self
    }

    fn unsupported(label: &str) -> StepResult {
        let os = std::env::consts::OS;
        StepResult {
            node_id: 0,
            label: label.to_string(),
            success: false,
            output: String::new(),
            error: Some(format!(
                "OS automation is not supported on this platform ({os}) yet"
            )),
        }
    }
}

impl PlatformBackend for UnsupportedBackend {
    fn launch_app(&self, _app: &str, _args: &[String], label: &str, _t: Duration) -> StepResult {
        Self::unsupported(label)
    }
    fn open_path(&self, _path: &str, label: &str, _t: Duration) -> StepResult {
        Self::unsupported(label)
    }
    fn open_url(&self, _url: &str, label: &str, _t: Duration) -> StepResult {
        Self::unsupported(label)
    }
    fn find_files(&self, _query: &str, label: &str, _t: Duration) -> StepResult {
        Self::unsupported(label)
    }
    fn clipboard_read(&self, label: &str, _t: Duration) -> StepResult {
        Self::unsupported(label)
    }
    fn clipboard_write(&self, _content: &str, label: &str, _t: Duration) -> StepResult {
        Self::unsupported(label)
    }
    fn list_running_apps(&self, label: &str, _t: Duration) -> StepResult {
        Self::unsupported(label)
    }
    fn quit_app(&self, _app: &str, label: &str, _t: Duration) -> StepResult {
        Self::unsupported(label)
    }
    fn switch_app(&self, _app: &str, label: &str, _t: Duration) -> StepResult {
        Self::unsupported(label)
    }
    fn run_trusted_script(&self, _s: &str, label: &str, _c: bool, _t: Duration) -> StepResult {
        Self::unsupported(label)
    }
}
