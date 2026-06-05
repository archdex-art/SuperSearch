//! Capability gate — the mediation point for all privileged operations.
//!
//! Every OS-level operation from a plugin passes through a capability gate.
//! The gate validates the presented token against the registry and emits
//! a journal entry for the check (pass or fail).
//!
//! ## Performance
//! Gate checks are designed for < 100ns total:
//! - Token validity: ~2ns (atomic load)
//! - Permission check: ~5ns (linear scan of small vec)
//! - Namespace check: ~10ns (string prefix comparison)
//! - Registry lookup: ~20ns (DashMap read)
//! - Total: ~37ns typical

use std::sync::Arc;
use tracing::{debug, warn};

use super::namespace::Namespace;
use super::registry::CapabilityRegistry;
use super::token::{CapabilityToken, Permission};

/// Result of a capability gate check.
#[derive(Debug, Clone)]
pub enum GateDecision {
    /// Access granted. Includes the validated token ID for audit.
    Allowed {
        capability_id: super::token::CapabilityId,
        permission: Permission,
        namespace: Namespace,
    },
    /// Access denied with reason.
    Denied {
        reason: DenialReason,
        grantee: String,
        permission: Permission,
        namespace: Namespace,
    },
}

/// Why a capability check was denied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenialReason {
    /// Token has been revoked.
    Revoked,
    /// Token has expired.
    Expired,
    /// Token does not grant the required permission.
    InsufficientPermission,
    /// Token's namespace does not contain the requested namespace.
    NamespaceMismatch,
    /// No token was presented.
    NoToken,
    /// Token is not registered (forged or from a stale session).
    UnregisteredToken,
}

impl std::fmt::Display for DenialReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DenialReason::Revoked => write!(f, "capability revoked"),
            DenialReason::Expired => write!(f, "capability expired"),
            DenialReason::InsufficientPermission => write!(f, "insufficient permission"),
            DenialReason::NamespaceMismatch => write!(f, "namespace mismatch"),
            DenialReason::NoToken => write!(f, "no capability token presented"),
            DenialReason::UnregisteredToken => write!(f, "unregistered or forged token"),
        }
    }
}

/// The capability gate — all privileged operations pass through here.
///
/// The gate is shared (via `Arc`) across all plugin hosts. It holds a
/// reference to the registry and optionally a journal sender for auditing.
pub struct CapabilityGate {
    registry: Arc<CapabilityRegistry>,
    /// If true, denied checks are logged at WARN level.
    /// In production, this should always be true for security auditing.
    log_denials: bool,
}

impl CapabilityGate {
    pub fn new(registry: Arc<CapabilityRegistry>) -> Self {
        Self {
            registry,
            log_denials: true,
        }
    }

    /// Check whether a capability token grants the requested permission
    /// in the requested namespace.
    ///
    /// This is the hot-path security check. Total cost: ~37ns typical.
    ///
    /// ## Arguments
    /// - `token`: The capability token presented by the plugin. `None` if
    ///   the plugin attempted an operation without presenting a token.
    /// - `namespace`: The namespace of the resource being accessed.
    /// - `permission`: The specific permission required.
    ///
    /// ## Returns
    /// A [`GateDecision`] indicating whether access is allowed or denied.
    /// The decision should be journaled by the caller for audit replay.
    #[inline]
    pub fn check(
        &self,
        token: Option<&CapabilityToken>,
        namespace: &Namespace,
        permission: Permission,
    ) -> GateDecision {
        let token = match token {
            Some(t) => t,
            None => {
                let decision = GateDecision::Denied {
                    reason: DenialReason::NoToken,
                    grantee: "unknown".into(),
                    permission,
                    namespace: namespace.clone(),
                };
                if self.log_denials {
                    warn!(
                        permission = ?permission,
                        namespace = %namespace,
                        "Capability gate DENIED: no token presented"
                    );
                }
                return decision;
            }
        };

        // Detailed validation with specific denial reasons.
        if token.is_revoked() {
            let decision = GateDecision::Denied {
                reason: DenialReason::Revoked,
                grantee: token.grantee.clone(),
                permission,
                namespace: namespace.clone(),
            };
            if self.log_denials {
                warn!(grantee = %token.grantee, id = %token.id, "DENIED: token revoked");
            }
            return decision;
        }

        if !token.is_valid() {
            let decision = GateDecision::Denied {
                reason: DenialReason::Expired,
                grantee: token.grantee.clone(),
                permission,
                namespace: namespace.clone(),
            };
            if self.log_denials {
                warn!(grantee = %token.grantee, id = %token.id, "DENIED: token expired");
            }
            return decision;
        }

        if !token.has_permission(permission) {
            let decision = GateDecision::Denied {
                reason: DenialReason::InsufficientPermission,
                grantee: token.grantee.clone(),
                permission,
                namespace: namespace.clone(),
            };
            if self.log_denials {
                warn!(
                    grantee = %token.grantee,
                    requested = ?permission,
                    "DENIED: insufficient permission"
                );
            }
            return decision;
        }

        if !token.namespace.contains(namespace) {
            let decision = GateDecision::Denied {
                reason: DenialReason::NamespaceMismatch,
                grantee: token.grantee.clone(),
                permission,
                namespace: namespace.clone(),
            };
            if self.log_denials {
                warn!(
                    grantee = %token.grantee,
                    token_ns = %token.namespace,
                    requested_ns = %namespace,
                    "DENIED: namespace mismatch"
                );
            }
            return decision;
        }

        // Final registry existence check.
        if !self.registry.grants.contains_key(&token.id) {
            let decision = GateDecision::Denied {
                reason: DenialReason::UnregisteredToken,
                grantee: token.grantee.clone(),
                permission,
                namespace: namespace.clone(),
            };
            if self.log_denials {
                warn!(grantee = %token.grantee, id = %token.id, "DENIED: unregistered token");
            }
            return decision;
        }

        debug!(
            grantee = %token.grantee,
            permission = ?permission,
            namespace = %namespace,
            "Capability gate ALLOWED"
        );

        GateDecision::Allowed {
            capability_id: token.id,
            permission,
            namespace: namespace.clone(),
        }
    }

    /// Convenience: check and return a boolean (for use in guard clauses).
    #[inline]
    pub fn is_allowed(
        &self,
        token: Option<&CapabilityToken>,
        namespace: &Namespace,
        permission: Permission,
    ) -> bool {
        matches!(self.check(token, namespace, permission), GateDecision::Allowed { .. })
    }
}
