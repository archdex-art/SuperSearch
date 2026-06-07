//! # Extensions — user-installable capabilities
//!
//! SuperSearch extensions live on disk as a directory with a `manifest.toml`
//! plus an entrypoint. Two execution models share one registry and manager UI:
//!
//! - **Script** (v1): the entrypoint is a native script run as a subprocess
//!   (argv, no shell, hard timeout). Implemented in [`host`].
//! - **Wasm** (v2): a sandboxed WebAssembly module (declared in manifests,
//!   not yet executed — see the `plugin` module's sandbox scaffolding).
//!
//! Every extension is consent-gated: enabling it grants a revocable capability
//! token scoped to `plugin.<id>`, and its result-actions are checked against
//! that token by the same [`crate::capability::gate::CapabilityGate`] the agent
//! uses. Nothing an extension does escapes the capability model.

pub mod manifest;
pub mod host;
pub mod registry;

pub use host::{ExtensionAction, ExtensionResult};
pub use manifest::{ExtensionKind, ExtensionManifest};
pub use registry::{
    ExtensionError, ExtensionInfo, ExtensionQueryHit, ExtensionRegistry, PermissionInfo,
};
