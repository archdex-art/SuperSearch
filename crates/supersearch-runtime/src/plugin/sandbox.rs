//! WASM sandbox runtime using Wasmtime.
//!
//! Each plugin runs in its own Wasmtime instance with:
//! - Isolated linear memory (configurable, default 16 MiB).
//! - Fuel metering (instruction counting for preemption).
//! - No direct WASI access — all I/O goes through capability-gated host functions.

use tracing::{debug, info};

use super::manifest::ResourceLimits;

/// Errors from sandbox operations.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("WASM compilation failed: {0}")]
    CompilationFailed(String),
    #[error("WASM instantiation failed: {0}")]
    InstantiationFailed(String),
    #[error("WASM execution trapped: {0}")]
    Trap(String),
    #[error("Fuel exhausted — plugin exceeded instruction budget")]
    FuelExhausted,
    #[error("Memory limit exceeded: requested {requested} bytes, limit {limit} bytes")]
    MemoryLimitExceeded { requested: usize, limit: usize },
    #[error("Entry point '{0}' not found in WASM module")]
    EntryPointNotFound(String),
}

/// Configuration for a WASM sandbox instance.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub limits: ResourceLimits,
    /// Whether to enable fuel metering (instruction counting).
    /// Should always be true in production.
    pub fuel_metering: bool,
    /// Whether to enable WASM SIMD instructions.
    pub enable_simd: bool,
    /// Whether to enable WASM multi-memory proposal.
    pub enable_multi_memory: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            limits: ResourceLimits::default(),
            fuel_metering: true,
            enable_simd: true,
            enable_multi_memory: false,
        }
    }
}

/// Sandbox execution result.
#[derive(Debug)]
pub struct ExecutionResult {
    /// Fuel consumed during execution.
    pub fuel_consumed: u64,
    /// Peak memory usage in bytes.
    pub peak_memory_bytes: usize,
    /// Whether the execution completed or was interrupted.
    pub completed: bool,
    /// Serialized return value (if any).
    pub return_value: Option<Vec<u8>>,
}

/// A WASM sandbox instance for a single plugin.
///
/// The sandbox owns the Wasmtime engine, store, and module. Host functions
/// are registered as imports that delegate to the capability gate.
///
/// ## Zero-Copy IPC
/// Plugin ↔ Kernel communication uses shared linear memory regions.
/// The plugin writes a Cap'n Proto message into a designated IPC buffer
/// region, then calls a host function to notify the kernel. The kernel
/// reads directly from the plugin's linear memory (zero-copy).
pub struct WasmSandbox {
    /// Plugin identifier (for logging/tracing).
    plugin_id: String,
    /// Sandbox configuration.
    config: SandboxConfig,
    /// Current fuel remaining (tracked externally for observability).
    fuel_remaining: u64,
    /// Current memory usage tracking.
    current_memory_bytes: usize,
    /// Whether the sandbox has been initialized.
    initialized: bool,
    /// Number of invocations (for telemetry).
    invocation_count: u64,
}

impl WasmSandbox {
    /// Create a new sandbox for the given plugin.
    ///
    /// This pre-configures the Wasmtime engine but does NOT compile the
    /// WASM module — call `compile_and_instantiate()` separately.
    pub fn new(plugin_id: String, config: SandboxConfig) -> Self {
        info!(
            plugin = %plugin_id,
            memory_limit = config.limits.max_memory_bytes,
            fuel = config.limits.max_fuel,
            "Creating WASM sandbox"
        );

        Self {
            plugin_id,
            fuel_remaining: config.limits.max_fuel,
            config,
            current_memory_bytes: 0,
            initialized: false,
            invocation_count: 0,
        }
    }

    /// Compile and instantiate a WASM module from bytes.
    ///
    /// In the full implementation, this would:
    /// 1. Create a Wasmtime `Engine` with the configured settings.
    /// 2. Compile the WASM bytes into a `Module`.
    /// 3. Create a `Store` with fuel metering.
    /// 4. Register host function imports (capability-gated).
    /// 5. Instantiate the module.
    ///
    /// The skeleton below establishes the interface; Wasmtime integration
    /// requires the actual WASM bytes and host function bindings.
    pub fn compile_and_instantiate(&mut self, _wasm_bytes: &[u8]) -> Result<(), SandboxError> {
        // Validate WASM binary size against memory limits.
        if _wasm_bytes.len() > self.config.limits.max_memory_bytes {
            return Err(SandboxError::MemoryLimitExceeded {
                requested: _wasm_bytes.len(),
                limit: self.config.limits.max_memory_bytes,
            });
        }

        // In production: Wasmtime compilation pipeline.
        // let engine = wasmtime::Engine::new(&wasmtime_config)?;
        // let module = wasmtime::Module::new(&engine, wasm_bytes)?;
        // let mut store = wasmtime::Store::new(&engine, state);
        // store.set_fuel(self.config.limits.max_fuel)?;
        // let instance = wasmtime::Instance::new(&mut store, &module, &imports)?;

        self.initialized = true;
        info!(plugin = %self.plugin_id, "WASM sandbox initialized");
        Ok(())
    }

    /// Invoke the plugin's entry point function.
    ///
    /// Returns an `ExecutionResult` with fuel consumed and return value.
    /// If fuel is exhausted, returns `SandboxError::FuelExhausted`.
    pub fn invoke(
        &mut self,
        _function_name: &str,
        _args: &[u8],
    ) -> Result<ExecutionResult, SandboxError> {
        if !self.initialized {
            return Err(SandboxError::InstantiationFailed(
                "Sandbox not initialized".into(),
            ));
        }

        self.invocation_count += 1;

        // In production: invoke via Wasmtime Store.
        // let func = instance.get_typed_func::<_, _>(&mut store, function_name)?;
        // let result = func.call(&mut store, args)?;
        // let fuel_consumed = initial_fuel - store.get_fuel()?;

        let result = ExecutionResult {
            fuel_consumed: 0,
            peak_memory_bytes: self.current_memory_bytes,
            completed: true,
            return_value: None,
        };

        debug!(
            plugin = %self.plugin_id,
            invocation = self.invocation_count,
            "Plugin invocation complete"
        );

        Ok(result)
    }

    /// Refuel the sandbox for the next time slice.
    pub fn refuel(&mut self, fuel: u64) {
        self.fuel_remaining = fuel;
    }

    /// Check remaining fuel.
    #[inline]
    pub fn fuel_remaining(&self) -> u64 {
        self.fuel_remaining
    }

    /// Check if the sandbox is initialized.
    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Teardown the sandbox, releasing all resources.
    pub fn teardown(&mut self) {
        info!(
            plugin = %self.plugin_id,
            invocations = self.invocation_count,
            "Tearing down WASM sandbox"
        );
        self.initialized = false;
        self.fuel_remaining = 0;
        self.current_memory_bytes = 0;
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }
}
