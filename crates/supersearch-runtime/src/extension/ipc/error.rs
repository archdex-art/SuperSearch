//! # IPC Error Mapping (Phase 8)
//!
//! Standardized binary error format mapping Rust host failures back into the Guest SDK's
//! Error Taxonomy (e.g., `CapabilityError`, `NetworkError`).

use serde::{Deserialize, Serialize};

/// Known error codes that the TypeScript SDK reconstructs into native JavaScript Error classes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcErrorCode {
    /// Fired when an extension attempts to use an ungranted OS API.
    CapabilityError,
    /// Fired during `fetch` overrides or TLS issues.
    NetworkError,
    /// Fired on SQLite IO failures.
    StorageError,
    /// Fired if async tasks resolve while the extension is suspended.
    ExtensionSuspendedError,
    /// Fired if the Host version is incompatible with the SDK version.
    VersionMismatchError,
    /// Fired if the operation times out.
    TimeoutError,
    /// Fired on `AbortSignal` trigger.
    AbortError,
    /// Serialization or boundary failure.
    IpcError,
    /// Catch-all for unexpected host panics.
    InternalError,
}

impl std::fmt::Display for IpcErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// The standardized error payload sent in an IPC Response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcErrorPayload {
    /// The string identifier mapping to the SDK class.
    pub code: IpcErrorCode,
    /// Human-readable message detailing the failure.
    pub message: String,
}

impl IpcErrorPayload {
    /// Constructs a new error payload.
    pub fn new(code: IpcErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl From<crate::extension::storage::StorageError> for IpcErrorPayload {
    fn from(err: crate::extension::storage::StorageError) -> Self {
        Self::new(IpcErrorCode::StorageError, err.to_string())
    }
}
