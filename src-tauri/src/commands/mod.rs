//! Command module registry — all Tauri IPC command handlers.

use std::sync::{Arc, OnceLock};
use supersearch_runtime::platform::{default_backend, PlatformBackend};

pub mod actions;
pub mod agent;
pub mod extensions;
pub mod journal;
pub mod search;
pub mod settings;
pub mod system_search;
pub mod updater;
pub mod telemetry;
pub mod window;

/// The process-wide OS-automation backend selected for the current platform.
///
/// IPC handlers route the OS automation the Platform Abstraction Layer covers —
/// file search, opening files, reading the clipboard, listing running apps —
/// through this single seam instead of spawning platform tools directly. That
/// gives the host layer one auditable place for those calls and makes them work
/// on every target the PAL supports (macOS, Linux), not just macOS. Operations
/// the PAL has no portable equivalent for (keystroke injection, sleep, dark
/// mode, …) remain platform-specific in their handlers.
pub fn os_backend() -> &'static Arc<dyn PlatformBackend> {
    static BACKEND: OnceLock<Arc<dyn PlatformBackend>> = OnceLock::new();
    BACKEND.get_or_init(default_backend)
}
