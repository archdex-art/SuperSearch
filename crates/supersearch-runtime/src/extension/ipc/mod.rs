//! # Module: IPC Bridge (Milestone 2)
//!
//! Provides the highly optimized, binary MessagePack communication protocol
//! that routes requests and UI trees between the isolated V8 Guest and the Rust Host.
//!
//! Defined in Phase 8 of the architecture.

pub mod envelope;
pub mod error;

pub use envelope::{EnvelopeType, IpcEnvelope, IpcFlags, IPC_VERSION};
pub use error::{IpcErrorCode, IpcErrorPayload};
