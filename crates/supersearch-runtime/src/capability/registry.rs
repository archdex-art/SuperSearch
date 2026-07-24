//! Capability registry — centralized grant/revoke/query.
//!
//! The registry is the single source of truth for all active capabilities.
//! Only the kernel can create grants; plugins receive tokens through
//! capability injection, never by self-minting.
//!
//! Uses `DashMap` for concurrent read-heavy access (plugins checking
//! capabilities) with infrequent writes (grants/revocations).

use dashmap::DashMap;
use std::time::Instant;
use tracing::info;

use super::namespace::Namespace;
use super::token::{CapabilityId, CapabilityToken, Permission};

/// Errors from capability registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Capability {0} not found")]
    NotFound(CapabilityId),
    #[error("Capability {0} already revoked")]
    AlreadyRevoked(CapabilityId),
    #[error("Duplicate capability grant for {grantee} in namespace {namespace}")]
    DuplicateGrant { grantee: String, namespace: String },
    #[error("Unauthorized: only kernel can grant capabilities")]
    Unauthorized,
}

/// A grant record stored in the registry.
#[derive(Debug, Clone)]
pub struct GrantRecord {
    pub token: CapabilityToken,
    /// Human-readable reason for the grant (for audit trail).
    pub reason: String,
    /// The sequence number of the journal entry that recorded this grant.
    /// Used for deterministic replay.
    pub journal_sequence: Option<u64>,
}

/// The centralized capability registry.
///
/// Thread-safe via `DashMap` (lock-free reads, sharded writes).
/// Expected access pattern: O(1000) reads/sec (gate checks), O(1) writes/sec (grants).
pub struct CapabilityRegistry {
    /// Active grants indexed by CapabilityId.
    pub(crate) grants: DashMap<CapabilityId, GrantRecord>,
    /// Secondary index: grantee → list of their capability IDs.
    /// Enables efficient "revoke all capabilities for plugin X".
    by_grantee: DashMap<String, Vec<CapabilityId>>,
    /// The kernel's secret key for capability ID derivation.
    /// MUST be generated from a CSPRNG at runtime boot and NEVER persisted.
    kernel_key: [u8; 32],
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        // A fresh, unpredictable key per boot. Capability IDs are derived from
        // this key (see `CapabilityToken::new`); a fixed, compiled-in key would
        // let anything that can execute in-process precompute IDs for grants it
        // never received, defeating the "unforgeable" guarantee the token
        // system advertises.
        let mut key = [0u8; 32];
        getrandom::getrandom(&mut key)
            .expect("OS CSPRNG unavailable — cannot generate capability key");

        Self {
            grants: DashMap::with_capacity(256),
            by_grantee: DashMap::with_capacity(64),
            kernel_key: key,
        }
    }

    /// Create a registry with a specific key (for deterministic replay).
    pub fn with_key(key: [u8; 32]) -> Self {
        Self {
            grants: DashMap::with_capacity(256),
            by_grantee: DashMap::with_capacity(64),
            kernel_key: key,
        }
    }

    /// Grant a new capability. Returns the token that should be injected
    /// into the plugin.
    ///
    /// Only the kernel should call this. Plugins NEVER self-grant.
    pub fn grant(
        &self,
        namespace: Namespace,
        permissions: Vec<Permission>,
        grantee: String,
        expires_at: Option<Instant>,
        reason: String,
    ) -> CapabilityToken {
        let token = CapabilityToken::new(
            &self.kernel_key,
            namespace,
            permissions,
            grantee.clone(),
            expires_at,
        );

        let id = token.id;

        let record = GrantRecord {
            token: token.clone(),
            reason,
            journal_sequence: None,
        };

        self.grants.insert(id, record);
        self.by_grantee.entry(grantee.clone()).or_default().push(id);

        info!(
            id = %id,
            grantee = %grantee,
            namespace = %token.namespace,
            permissions = token.permissions.len(),
            "Capability granted"
        );

        token
    }

    /// Revoke a specific capability by ID.
    pub fn revoke(&self, id: &CapabilityId) -> Result<(), RegistryError> {
        let record = self.grants.get(id).ok_or(RegistryError::NotFound(*id))?;

        if record.token.is_revoked() {
            return Err(RegistryError::AlreadyRevoked(*id));
        }

        record.token.revoke();
        info!(id = %id, grantee = %record.token.grantee, "Capability revoked");
        Ok(())
    }

    /// Revoke ALL capabilities for a specific grantee (e.g., when unloading a plugin).
    pub fn revoke_all_for_grantee(&self, grantee: &str) -> usize {
        let ids = self.by_grantee.get(grantee);
        let mut revoked_count = 0;

        if let Some(ids) = ids {
            for id in ids.value() {
                if let Some(record) = self.grants.get(id) {
                    if !record.token.is_revoked() {
                        record.token.revoke();
                        revoked_count += 1;
                    }
                }
            }
        }

        if revoked_count > 0 {
            info!(grantee = grantee, count = revoked_count, "Bulk revocation");
        }
        revoked_count
    }

    /// Validate a capability token: checks existence, revocation, expiration,
    /// namespace containment, and permission membership.
    ///
    /// This is the hot-path check called by [`CapabilityGate`]. Optimized for
    /// speed: DashMap read is lock-free, token validity is an atomic load.
    pub fn validate(
        &self,
        token: &CapabilityToken,
        required_namespace: &Namespace,
        required_permission: Permission,
    ) -> bool {
        // 1. Token self-validation (atomic revocation check + expiry).
        if !token.is_valid() {
            return false;
        }

        // 2. Permission check.
        if !token.has_permission(required_permission) {
            return false;
        }

        // 3. Namespace containment — token's namespace must contain the
        //    requested namespace. E.g., a token for "plugin.chatgpt" can
        //    access "plugin.chatgpt.filesystem" but not "plugin.vscode".
        if !token.namespace.contains(required_namespace) {
            return false;
        }

        // 4. Registry existence check — the token must still be in the registry.
        //    (Prevents use of tokens from a previous runtime session.)
        self.grants.contains_key(&token.id)
    }

    /// List all active (non-revoked) capabilities for a grantee.
    pub fn list_active_for_grantee(&self, grantee: &str) -> Vec<CapabilityToken> {
        let mut active = Vec::new();
        if let Some(ids) = self.by_grantee.get(grantee) {
            for id in ids.value() {
                if let Some(record) = self.grants.get(id) {
                    if record.token.is_valid() {
                        active.push(record.token.clone());
                    }
                }
            }
        }
        active
    }

    /// Total number of grants (including revoked, for audit purposes).
    pub fn total_grants(&self) -> usize {
        self.grants.len()
    }

    /// Number of currently valid (non-revoked, non-expired) grants.
    pub fn active_grants(&self) -> usize {
        self.grants
            .iter()
            .filter(|r| r.value().token.is_valid())
            .count()
    }
}
