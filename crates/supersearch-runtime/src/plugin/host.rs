//! Plugin host — lifecycle management for loaded plugins.
//!
//! The PluginHost owns a set of loaded plugins, their sandboxes, IPC channels,
//! and capability tokens. It integrates with the Supervisor for fault tolerance
//! and the Scheduler for time-sliced execution.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

use super::manifest::PluginManifest;
use super::sandbox::{WasmSandbox, SandboxConfig, SandboxError};
use super::ipc::{IpcChannel, KernelIpcEndpoint};
use crate::capability::gate::CapabilityGate;
use crate::capability::namespace::Namespace;
use crate::capability::registry::CapabilityRegistry;
use crate::capability::token::{CapabilityToken, Permission};

/// Plugin lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    /// Manifest parsed, not yet compiled.
    Registered,
    /// WASM compiled and instantiated.
    Loaded,
    /// Running — actively processing.
    Running,
    /// Suspended — yielded, state preserved.
    Suspended,
    /// Errored — waiting for supervisor decision.
    Errored,
    /// Unloaded — teardown complete.
    Unloaded,
}

/// A loaded plugin with its sandbox, IPC, and capabilities.
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub state: PluginState,
    pub sandbox: WasmSandbox,
    pub ipc_endpoint: KernelIpcEndpoint,
    pub granted_tokens: Vec<CapabilityToken>,
    pub loaded_at: Instant,
    pub error_count: u32,
}

/// Errors from plugin host operations.
#[derive(Debug, thiserror::Error)]
pub enum PluginHostError {
    #[error("Plugin '{0}' not found")]
    NotFound(String),
    #[error("Plugin '{0}' already loaded")]
    AlreadyLoaded(String),
    #[error("Manifest validation failed: {0}")]
    ManifestError(#[from] super::manifest::ManifestError),
    #[error("Sandbox error: {0}")]
    SandboxError(#[from] SandboxError),
    #[error("Plugin '{plugin}' requires permission {permission:?} which was denied")]
    PermissionDenied { plugin: String, permission: Permission },
    #[error("Plugin '{0}' is in state {1:?}, expected {2:?}")]
    InvalidState(String, PluginState, PluginState),
}

/// The plugin host manages the lifecycle of all adapter plugins.
pub struct PluginHost {
    /// Loaded plugins indexed by plugin ID.
    plugins: HashMap<String, LoadedPlugin>,
    /// Shared capability registry.
    registry: Arc<CapabilityRegistry>,
    /// Shared capability gate.
    gate: Arc<CapabilityGate>,
    /// IPC channel buffer size.
    ipc_buffer_size: usize,
}

impl PluginHost {
    pub fn new(
        registry: Arc<CapabilityRegistry>,
        gate: Arc<CapabilityGate>,
    ) -> Self {
        Self {
            plugins: HashMap::new(),
            registry,
            gate,
            ipc_buffer_size: 256,
        }
    }

    /// Load a plugin: validate manifest, compile sandbox, grant capabilities,
    /// create IPC channel.
    pub fn load(
        &mut self,
        manifest: PluginManifest,
        wasm_bytes: &[u8],
    ) -> Result<(), PluginHostError> {
        // 1. Validate manifest.
        manifest.validate()?;

        if self.plugins.contains_key(&manifest.id) {
            return Err(PluginHostError::AlreadyLoaded(manifest.id.clone()));
        }

        let plugin_id = manifest.id.clone();
        let namespace = Namespace::plugin(&plugin_id);

        // 2. Grant requested capabilities.
        let mut granted_tokens = Vec::new();
        for req in &manifest.permissions {
            let token = self.registry.grant(
                namespace.child(req.permission.category()),
                vec![req.permission],
                plugin_id.clone(),
                None, // No expiration — revoked on unload.
                req.justification.clone(),
            );
            granted_tokens.push(token);
        }

        // 3. Create IPC channel.
        let (_plugin_ipc, kernel_ipc) = IpcChannel::create(
            plugin_id.clone(),
            namespace,
            self.ipc_buffer_size,
            manifest.resource_limits.max_ipc_message_bytes,
        );

        // 4. Compile and instantiate sandbox.
        let sandbox_config = SandboxConfig {
            limits: manifest.resource_limits.clone(),
            fuel_metering: true,
            enable_simd: true,
            enable_multi_memory: false,
        };
        let mut sandbox = WasmSandbox::new(plugin_id.clone(), sandbox_config);
        sandbox.compile_and_instantiate(wasm_bytes)?;

        // 5. Register in plugin host.
        let loaded = LoadedPlugin {
            manifest,
            state: PluginState::Loaded,
            sandbox,
            ipc_endpoint: kernel_ipc,
            granted_tokens,
            loaded_at: Instant::now(),
            error_count: 0,
        };

        self.plugins.insert(plugin_id.clone(), loaded);
        info!(plugin = %plugin_id, "Plugin loaded successfully");
        Ok(())
    }

    /// Start a loaded plugin (transition to Running state).
    pub fn start(&mut self, plugin_id: &str) -> Result<(), PluginHostError> {
        let plugin = self.plugins.get_mut(plugin_id)
            .ok_or_else(|| PluginHostError::NotFound(plugin_id.into()))?;

        if plugin.state != PluginState::Loaded && plugin.state != PluginState::Suspended {
            return Err(PluginHostError::InvalidState(
                plugin_id.into(), plugin.state, PluginState::Loaded,
            ));
        }

        plugin.state = PluginState::Running;
        info!(plugin = %plugin_id, "Plugin started");
        Ok(())
    }

    /// Suspend a running plugin (preserve state, yield time slice).
    pub fn suspend(&mut self, plugin_id: &str) -> Result<(), PluginHostError> {
        let plugin = self.plugins.get_mut(plugin_id)
            .ok_or_else(|| PluginHostError::NotFound(plugin_id.into()))?;

        if plugin.state != PluginState::Running {
            return Err(PluginHostError::InvalidState(
                plugin_id.into(), plugin.state, PluginState::Running,
            ));
        }

        plugin.state = PluginState::Suspended;
        debug!(plugin = %plugin_id, "Plugin suspended");
        Ok(())
    }

    /// Unload a plugin: revoke capabilities, teardown sandbox, remove from host.
    pub fn unload(&mut self, plugin_id: &str) -> Result<(), PluginHostError> {
        let mut plugin = self.plugins.remove(plugin_id)
            .ok_or_else(|| PluginHostError::NotFound(plugin_id.into()))?;

        // Revoke all capabilities.
        let revoked = self.registry.revoke_all_for_grantee(plugin_id);

        // Teardown sandbox.
        plugin.sandbox.teardown();
        plugin.state = PluginState::Unloaded;

        info!(
            plugin = %plugin_id,
            revoked_capabilities = revoked,
            "Plugin unloaded"
        );
        Ok(())
    }

    /// Report a plugin error (for supervisor integration).
    pub fn report_error(&mut self, plugin_id: &str) {
        if let Some(plugin) = self.plugins.get_mut(plugin_id) {
            plugin.error_count += 1;
            plugin.state = PluginState::Errored;
            warn!(
                plugin = %plugin_id,
                errors = plugin.error_count,
                "Plugin error reported"
            );
        }
    }

    /// Get the current state of a plugin.
    pub fn get_state(&self, plugin_id: &str) -> Option<PluginState> {
        self.plugins.get(plugin_id).map(|p| p.state)
    }

    /// List all loaded plugin IDs.
    pub fn list_plugins(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// Get the kernel IPC endpoint for a plugin (for the kernel's message loop).
    pub fn get_ipc_endpoint(&mut self, plugin_id: &str) -> Option<&mut KernelIpcEndpoint> {
        self.plugins.get_mut(plugin_id).map(|p| &mut p.ipc_endpoint)
    }

    /// Number of loaded plugins.
    pub fn plugin_count(&self) -> usize { self.plugins.len() }
}
