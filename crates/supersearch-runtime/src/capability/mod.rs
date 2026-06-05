//! # Module 3: Capability-Based Security System
//!
//! Implements the object-capability security model for plugin isolation.
//! Plugins receive only explicitly granted, revocable, namespaced capabilities.
//! No implicit trust — every OS-level operation requires a valid capability token
//! that is checked at the mediation gate.
//!
//! ## Design Principles
//! - **No ambient authority**: Plugins cannot discover capabilities; they are injected.
//! - **Namespace isolation**: Each plugin operates in a scoped namespace.
//! - **Revocable**: Capabilities can be revoked at any time (atomic flag flip).
//! - **Auditable**: Every grant, revoke, and check is journaled.

pub mod token;
pub mod namespace;
pub mod registry;
pub mod gate;

pub use token::{CapabilityToken, CapabilityId, Permission};
pub use namespace::Namespace;
pub use registry::CapabilityRegistry;
pub use gate::{CapabilityGate, GateDecision};
