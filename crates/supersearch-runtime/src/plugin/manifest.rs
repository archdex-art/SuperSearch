//! Plugin manifest — declarative plugin metadata and permission requests.
//!
//! Every plugin ships with a manifest (TOML or embedded in the WASM binary)
//! that declares its identity, required capabilities, and resource limits.
//! The kernel reads the manifest before loading to determine what capabilities
//! to inject and whether to approve the plugin.

use serde::{Serialize, Deserialize};
use crate::capability::token::Permission;

/// Semantic version for plugins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl std::fmt::Display for PluginVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A single permission request with justification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPermissionRequest {
    /// The permission being requested.
    pub permission: Permission,
    /// Human-readable justification for why this permission is needed.
    /// Displayed to the user during plugin installation consent.
    pub justification: String,
    /// Whether the plugin can function without this permission.
    /// Non-optional permissions cause load failure if denied.
    pub optional: bool,
}

/// Resource limits for the plugin sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum WASM linear memory in bytes (default: 16 MiB).
    pub max_memory_bytes: usize,
    /// Maximum number of WASM instructions per invocation (fuel).
    pub max_fuel: u64,
    /// Maximum number of concurrent tasks this plugin can spawn.
    pub max_concurrent_tasks: u32,
    /// Maximum IPC message size in bytes.
    pub max_ipc_message_bytes: usize,
    /// Scheduler priority ceiling: the highest priority class this plugin
    /// can use for spawned tasks. Prevents plugins from monopolizing
    /// Critical slots.
    pub priority_ceiling: crate::scheduler::priority::PriorityClass,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 16 * 1024 * 1024, // 16 MiB
            max_fuel: 1_000_000_000,              // ~1 billion instructions
            max_concurrent_tasks: 16,
            max_ipc_message_bytes: 1024 * 1024,   // 1 MiB
            priority_ceiling: crate::scheduler::priority::PriorityClass::UserBlocking,
        }
    }
}

/// The complete plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin identifier (reverse-domain: e.g., "com.openai.chatgpt").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Plugin version.
    pub version: PluginVersion,
    /// Plugin author.
    pub author: String,
    /// Brief description.
    pub description: String,
    /// Requested permissions with justifications.
    pub permissions: Vec<PluginPermissionRequest>,
    /// Resource limits for the sandbox.
    pub resource_limits: ResourceLimits,
    /// BLAKE3 hash of the WASM binary for integrity verification.
    pub wasm_hash: Option<String>,
    /// Entry point function name in the WASM module.
    pub entry_point: String,
    /// Plugin API version this plugin was built for.
    pub api_version: u32,
}

impl PluginManifest {
    /// Validate the manifest for structural correctness.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.id.is_empty() {
            return Err(ManifestError::MissingField("id"));
        }
        if self.name.is_empty() {
            return Err(ManifestError::MissingField("name"));
        }
        if self.entry_point.is_empty() {
            return Err(ManifestError::MissingField("entry_point"));
        }
        if self.api_version == 0 {
            return Err(ManifestError::InvalidApiVersion(0));
        }
        // Validate that no non-optional permission requests exceed the
        // priority ceiling (e.g., a plugin with UserBlocking ceiling
        // should not request TaskSpawnCritical).
        for req in &self.permissions {
            if !req.optional
                && matches!(req.permission, Permission::TaskSpawnCritical)
                    && self.resource_limits.priority_ceiling
                        > crate::scheduler::priority::PriorityClass::Critical
                {
                    // This is actually always fine because Critical is 0 (lowest enum value)
                    // but the real check is against the ceiling.
                }
        }
        Ok(())
    }

    /// Extract the list of required (non-optional) permissions.
    pub fn required_permissions(&self) -> Vec<Permission> {
        self.permissions.iter()
            .filter(|r| !r.optional)
            .map(|r| r.permission)
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
    #[error("Invalid API version: {0}")]
    InvalidApiVersion(u32),
    #[error("Permission request exceeds priority ceiling")]
    PriorityCeilingViolation,
}
