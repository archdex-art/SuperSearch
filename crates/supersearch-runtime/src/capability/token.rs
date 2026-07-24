//! Capability tokens — unforgeable, revocable, namespace-scoped grants.
//!
//! A `CapabilityToken` is a cryptographic proof-of-grant that a plugin presents
//! to the capability gate when requesting access to a privileged operation.
//! Tokens are:
//! - **Unforgeable**: Derived from a BLAKE3 keyed hash of the grant parameters.
//! - **Revocable**: Contain an atomic `revoked` flag checked on every gate access.
//! - **Scoped**: Bound to a specific namespace, permission set, and time window.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use super::namespace::Namespace;

/// Unique identifier for a capability grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityId(pub [u8; 32]);

impl CapabilityId {
    /// Derive a capability ID from grant parameters using BLAKE3 keyed hash.
    /// This makes tokens unforgeable — a plugin cannot construct a valid ID
    /// without the kernel's secret key.
    pub fn derive(
        key: &[u8; 32],
        namespace: &Namespace,
        permissions: &[Permission],
        grantee: &str,
    ) -> Self {
        let mut hasher = blake3::Hasher::new_keyed(key);
        hasher.update(namespace.as_str().as_bytes());
        for perm in permissions {
            hasher.update(&[*perm as u8]);
        }
        hasher.update(grantee.as_bytes());
        let hash = hasher.finalize();
        CapabilityId(*hash.as_bytes())
    }
}

impl std::fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display first 8 bytes as hex for readability.
        for byte in &self.0[..8] {
            write!(f, "{:02x}", byte)?;
        }
        write!(f, "…")
    }
}

/// Fine-grained permissions within a capability grant.
///
/// Permissions are bit-flags that can be combined. A single capability token
/// may grant multiple permissions (e.g., FileRead + FileWrite for a specific
/// directory namespace).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Permission {
    // ─── Filesystem ──────────────────────────────────────────────────
    FileRead = 0,
    FileWrite = 1,
    FileDelete = 2,
    DirectoryList = 3,

    // ─── Network ─────────────────────────────────────────────────────
    NetworkConnect = 10,
    NetworkListen = 11,
    NetworkDnsResolve = 12,

    // ─── Process ─────────────────────────────────────────────────────
    ProcessSpawn = 20,
    ProcessSignal = 21,
    ProcessInspect = 22,

    // ─── OS Automation ───────────────────────────────────────────────
    WindowEnumerate = 30,
    WindowManipulate = 31,
    InputSimulate = 32,
    ClipboardRead = 33,
    ClipboardWrite = 34,
    ScreenCapture = 35,

    // ─── AI / LLM ────────────────────────────────────────────────────
    LlmInference = 40,
    LlmStreamTokens = 41,
    EmbeddingGenerate = 42,

    // ─── IPC ─────────────────────────────────────────────────────────
    IpcSend = 50,
    IpcReceive = 51,
    IpcBroadcast = 52,

    // ─── Scheduler ───────────────────────────────────────────────────
    TaskSpawnCritical = 60,
    TaskSpawnInteractive = 61,
    TaskSpawnBackground = 62,
}

impl Permission {
    /// Returns the human-readable category for this permission.
    pub const fn category(&self) -> &'static str {
        match self {
            Permission::FileRead
            | Permission::FileWrite
            | Permission::FileDelete
            | Permission::DirectoryList => "filesystem",

            Permission::NetworkConnect
            | Permission::NetworkListen
            | Permission::NetworkDnsResolve => "network",

            Permission::ProcessSpawn | Permission::ProcessSignal | Permission::ProcessInspect => {
                "process"
            }

            Permission::WindowEnumerate
            | Permission::WindowManipulate
            | Permission::InputSimulate
            | Permission::ClipboardRead
            | Permission::ClipboardWrite
            | Permission::ScreenCapture => "os_automation",

            Permission::LlmInference
            | Permission::LlmStreamTokens
            | Permission::EmbeddingGenerate => "ai",

            Permission::IpcSend | Permission::IpcReceive | Permission::IpcBroadcast => "ipc",

            Permission::TaskSpawnCritical
            | Permission::TaskSpawnInteractive
            | Permission::TaskSpawnBackground => "scheduler",
        }
    }
}

/// A capability token — the unforgeable grant presented to capability gates.
///
/// Tokens are reference-counted (`Arc`) because they may be shared across
/// multiple async tasks within the same plugin, and the revocation flag must
/// be visible to all holders.
#[derive(Debug, Clone)]
pub struct CapabilityToken {
    /// Cryptographic identifier derived from grant parameters.
    pub id: CapabilityId,
    /// The namespace this capability is scoped to.
    pub namespace: Namespace,
    /// Granted permissions.
    pub permissions: Vec<Permission>,
    /// The plugin/entity this capability was granted to.
    pub grantee: String,
    /// When this capability was granted.
    pub granted_at: Instant,
    /// Optional expiration. `None` means valid until explicitly revoked.
    pub expires_at: Option<Instant>,
    /// Atomic revocation flag. Checked on every gate access (~1ns).
    /// Shared across all clones of this token via Arc.
    revoked: Arc<AtomicBool>,
}

impl CapabilityToken {
    /// Create a new capability token.
    pub fn new(
        key: &[u8; 32],
        namespace: Namespace,
        permissions: Vec<Permission>,
        grantee: String,
        expires_at: Option<Instant>,
    ) -> Self {
        let id = CapabilityId::derive(key, &namespace, &permissions, &grantee);
        Self {
            id,
            namespace,
            permissions,
            grantee,
            granted_at: Instant::now(),
            expires_at,
            revoked: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if this token is currently valid.
    /// Cost: ~2ns (one atomic load + optional Instant comparison).
    #[inline]
    pub fn is_valid(&self) -> bool {
        if self.revoked.load(Ordering::Acquire) {
            return false;
        }
        if let Some(expires) = self.expires_at {
            if Instant::now() >= expires {
                return false;
            }
        }
        true
    }

    /// Check if this token grants a specific permission.
    #[inline]
    pub fn has_permission(&self, perm: Permission) -> bool {
        self.permissions.contains(&perm)
    }

    /// Revoke this token. All clones are immediately invalidated.
    /// This is the primary revocation mechanism — instant, atomic, lock-free.
    #[inline]
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
    }

    /// Check if this token has been explicitly revoked.
    #[inline]
    pub fn is_revoked(&self) -> bool {
        self.revoked.load(Ordering::Acquire)
    }
}
