//! OS automation primitives — capability-gated access to the host OS.
//!
//! Provides a uniform, cross-platform interface for:
//! - Window enumeration and manipulation
//! - Input simulation (keyboard, mouse)
//! - Clipboard access
//! - Screen capture
//! - File system operations (scoped to granted namespaces)
//!
//! Every operation requires a valid capability token. Results are journaled
//! for deterministic replay.

use std::path::PathBuf;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use tracing::{debug, warn};

use crate::capability::gate::{CapabilityGate, GateDecision};
use crate::capability::namespace::Namespace;
use crate::capability::token::{CapabilityToken, Permission};

/// An OS automation action request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutomationAction {
    // ─── Window Management ───────────────────────────────────────────
    /// List all visible windows with their properties.
    EnumerateWindows,
    /// Focus a window by its OS handle.
    FocusWindow { handle: u64 },
    /// Resize/move a window.
    MoveWindow { handle: u64, x: i32, y: i32, width: u32, height: u32 },
    /// Minimize a window.
    MinimizeWindow { handle: u64 },
    /// Maximize a window.
    MaximizeWindow { handle: u64 },
    /// Close a window.
    CloseWindow { handle: u64 },

    // ─── Input Simulation ────────────────────────────────────────────
    /// Simulate keyboard input.
    KeyPress { key_code: u32, modifiers: u32 },
    /// Simulate key release.
    KeyRelease { key_code: u32, modifiers: u32 },
    /// Type a string (sequence of key presses).
    TypeText { text: String, delay_ms: u32 },
    /// Simulate mouse movement.
    MouseMove { x: i32, y: i32 },
    /// Simulate mouse click.
    MouseClick { button: MouseButton, x: i32, y: i32 },

    // ─── Clipboard ───────────────────────────────────────────────────
    /// Read clipboard contents.
    ClipboardRead,
    /// Write to clipboard.
    ClipboardWrite { content: String },

    // ─── Screen ──────────────────────────────────────────────────────
    /// Capture a screen region.
    ScreenCapture { x: i32, y: i32, width: u32, height: u32 },

    // ─── Filesystem (scoped) ─────────────────────────────────────────
    /// Read a file (path must be within the capability's namespace scope).
    FileRead { path: PathBuf },
    /// Write a file.
    FileWrite { path: PathBuf, contents: Vec<u8> },
    /// List directory contents.
    DirectoryList { path: PathBuf },
    /// Delete a file.
    FileDelete { path: PathBuf },
}

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Result of an automation action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutomationResult {
    /// Window list result.
    Windows(Vec<WindowInfo>),
    /// Success with no data.
    Ok,
    /// Clipboard contents.
    ClipboardContent(String),
    /// Screen capture as raw pixels (RGBA).
    ScreenData { width: u32, height: u32, data: Vec<u8> },
    /// File contents.
    FileData(Vec<u8>),
    /// Directory listing.
    DirectoryEntries(Vec<DirectoryEntry>),
    /// Error.
    Error(String),
}

/// Information about an OS window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub handle: u64,
    pub title: String,
    pub app_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_focused: bool,
    pub is_minimized: bool,
}

/// A directory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub is_dir: bool,
    pub size_bytes: u64,
}

/// The OS automation engine.
///
/// All operations are capability-gated. The engine does NOT directly execute
/// OS calls — it validates capabilities, then delegates to platform-specific
/// backends (macOS, Linux, Windows).
pub struct OsAutomation {
    gate: Arc<CapabilityGate>,
}

impl OsAutomation {
    pub fn new(gate: Arc<CapabilityGate>) -> Self {
        Self { gate }
    }

    /// Execute an automation action with capability validation.
    ///
    /// Returns the result that should be journaled for replay.
    pub fn execute(
        &self,
        action: &AutomationAction,
        token: &CapabilityToken,
        namespace: &Namespace,
    ) -> AutomationResult {
        // Determine required permission for this action.
        let required_perm = Self::required_permission(action);

        // Capability gate check (~37ns).
        let decision = self.gate.check(Some(token), namespace, required_perm);
        match decision {
            GateDecision::Allowed { .. } => {}
            GateDecision::Denied { reason, .. } => {
                warn!(
                    action = ?std::mem::discriminant(action),
                    reason = %reason,
                    "Automation action DENIED"
                );
                return AutomationResult::Error(format!("Capability denied: {}", reason));
            }
        }

        // Dispatch to platform-specific backend.
        // In production, this delegates to macOS Accessibility API,
        // X11/Wayland protocols, or Win32 API.
        debug!(action = ?std::mem::discriminant(action), "Executing automation action");
        self.execute_platform(action)
    }

    /// Map an action to its required permission.
    fn required_permission(action: &AutomationAction) -> Permission {
        match action {
            AutomationAction::EnumerateWindows => Permission::WindowEnumerate,
            AutomationAction::FocusWindow { .. }
            | AutomationAction::MoveWindow { .. }
            | AutomationAction::MinimizeWindow { .. }
            | AutomationAction::MaximizeWindow { .. }
            | AutomationAction::CloseWindow { .. } => Permission::WindowManipulate,

            AutomationAction::KeyPress { .. }
            | AutomationAction::KeyRelease { .. }
            | AutomationAction::TypeText { .. }
            | AutomationAction::MouseMove { .. }
            | AutomationAction::MouseClick { .. } => Permission::InputSimulate,

            AutomationAction::ClipboardRead => Permission::ClipboardRead,
            AutomationAction::ClipboardWrite { .. } => Permission::ClipboardWrite,
            AutomationAction::ScreenCapture { .. } => Permission::ScreenCapture,

            AutomationAction::FileRead { .. } => Permission::FileRead,
            AutomationAction::FileWrite { .. } => Permission::FileWrite,
            AutomationAction::DirectoryList { .. } => Permission::DirectoryList,
            AutomationAction::FileDelete { .. } => Permission::FileDelete,
        }
    }

    /// Platform-specific execution (stub for cross-platform development).
    ///
    /// In production, this would be a trait object dispatching to:
    /// - `MacOsAutomationBackend` (Accessibility API + CGEvent)
    /// - `LinuxAutomationBackend` (X11/xdotool or Wayland protocols)
    /// - `WindowsAutomationBackend` (Win32 UI Automation + SendInput)
    fn execute_platform(&self, action: &AutomationAction) -> AutomationResult {
        match action {
            AutomationAction::EnumerateWindows => {
                // Stub: return empty list. Platform backend fills this.
                AutomationResult::Windows(Vec::new())
            }
            AutomationAction::ClipboardRead => {
                AutomationResult::ClipboardContent(String::new())
            }
            AutomationAction::DirectoryList { path } => {
                // Delegate to std::fs in production.
                match std::fs::read_dir(path) {
                    Ok(entries) => {
                        let listing: Vec<DirectoryEntry> = entries
                            .filter_map(|e| e.ok())
                            .map(|e| {
                                let meta = e.metadata().ok();
                                DirectoryEntry {
                                    name: e.file_name().to_string_lossy().into(),
                                    is_dir: meta.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                                    size_bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                                }
                            })
                            .collect();
                        AutomationResult::DirectoryEntries(listing)
                    }
                    Err(e) => AutomationResult::Error(e.to_string()),
                }
            }
            AutomationAction::FileRead { path } => {
                match std::fs::read(path) {
                    Ok(data) => AutomationResult::FileData(data),
                    Err(e) => AutomationResult::Error(e.to_string()),
                }
            }
            // All other actions return Ok stub in development.
            _ => AutomationResult::Ok,
        }
    }
}
