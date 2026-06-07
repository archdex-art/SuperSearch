//! # Platform Abstraction Layer (PAL)
//!
//! The runtime kernel performs OS automation — launching apps, opening files
//! and URLs, searching the filesystem, reading and writing the clipboard,
//! enumerating and controlling apps — but its core logic must never embed raw,
//! OS-specific calls. This module is the single seam through which every such
//! operation flows.
//!
//! [`PlatformBackend`] is the stable, OS-agnostic contract the
//! [`AgentExecutor`](crate::agent::executor::AgentExecutor) depends on. Exactly
//! one backend is selected at startup by [`default_backend`] from the compile
//! target:
//!
//! - **macOS** → [`macos::MacosBackend`].
//! - **Linux** → [`linux::LinuxBackend`].
//! - **everything else** → [`unsupported::UnsupportedBackend`], which fails
//!   every operation with a clear, auditable error rather than silently doing
//!   nothing.
//!
//! A future Windows port is a new module implementing this same trait plus one
//! arm in [`default_backend`] — the executor, the capability gate, and the
//! journal are untouched. All backends spawn processes through one shared engine
//! ([`exec`]) so spawn, timeout, and result-normalization semantics are
//! identical on every OS, as the cross-platform execution contract requires.

use std::sync::Arc;
use std::time::Duration;

pub(crate) mod exec;

// All backends compile on every target (they are portable process-spawn logic),
// which keeps each backend's command construction unit-testable on any host.
// Exactly one is *selected* per target by `default_backend`; on the platforms
// where a backend is the inactive alternative it carries a targeted
// `allow(dead_code)`.
mod macos;
mod linux;
mod unsupported;

/// Result of executing a single OS automation primitive.
///
/// `node_id` is assigned by the executor when a result is attached to a task
/// graph node; backends always emit `0` and let the caller fill it in.
#[derive(Debug, Clone)]
pub struct StepResult {
    pub node_id: usize,
    pub label: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// The OS-agnostic automation contract the runtime kernel depends on.
///
/// Each method performs one OS primitive and returns a normalized
/// [`StepResult`]. The `label` is presentation text supplied by the caller and
/// echoed into the result so UI/journal messages stay consistent across
/// backends. Implementations must treat every `&str`/`&[String]` argument as
/// untrusted and must never interpolate it into a shell string or script
/// source — pass it as a spawned-process argument instead.
pub trait PlatformBackend: Send + Sync {
    /// Launch an application, optionally forwarding `args` to a fresh instance.
    fn launch_app(&self, app_name: &str, args: &[String], label: &str, timeout: Duration) -> StepResult;
    /// Open a filesystem path with its default handler.
    fn open_path(&self, path: &str, label: &str, timeout: Duration) -> StepResult;
    /// Open a URL with the default browser/handler.
    fn open_url(&self, url: &str, label: &str, timeout: Duration) -> StepResult;
    /// Search the filesystem by name; output is newline-delimited paths.
    fn find_files(&self, query: &str, label: &str, timeout: Duration) -> StepResult;
    /// Read the system clipboard's text contents.
    fn clipboard_read(&self, label: &str, timeout: Duration) -> StepResult;
    /// Write text to the system clipboard.
    fn clipboard_write(&self, content: &str, label: &str, timeout: Duration) -> StepResult;
    /// Enumerate user-visible (foreground) running applications.
    fn list_running_apps(&self, label: &str, timeout: Duration) -> StepResult;
    /// Quit a running application by name.
    fn quit_app(&self, app_name: &str, label: &str, timeout: Duration) -> StepResult;
    /// Bring a running application to the foreground by name.
    fn switch_app(&self, app_name: &str, label: &str, timeout: Duration) -> StepResult;
    /// Run a trusted, planner-generated constant script. `capture` returns its
    /// stdout as the result output. **Never** pass user-derived input here.
    fn run_trusted_script(&self, script: &str, label: &str, capture: bool, timeout: Duration) -> StepResult;
}

/// Select the platform backend for the current compile target.
///
/// macOS and Linux are implemented; every other target gets a backend that
/// fails each operation with a clear error, so the runtime compiles and behaves
/// deterministically everywhere even before a native port exists.
pub fn default_backend() -> Arc<dyn PlatformBackend> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(macos::MacosBackend::new())
    }
    #[cfg(target_os = "linux")]
    {
        Arc::new(linux::LinuxBackend::new())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Arc::new(unsupported::UnsupportedBackend::new())
    }
}
