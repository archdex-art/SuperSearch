//! # IPC Protocol Envelopes (Phase 8)
//!
//! Defines the dense, positional MessagePack arrays bridging V8 and Rust.
//! By using tuples instead of structs, we avoid serializing field names (keys),
//! massively compressing the byte payload on the wire.

use serde::{Deserialize, Serialize};

/// The current IPC version negotiated during sandbox boot.
pub const IPC_VERSION: u8 = 1;

bitflags::bitflags! {
    /// Bitmask for envelope routing, compression, and priority logic.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct IpcFlags: u8 {
        /// Indicates the payload is compressed with LZ4.
        const COMPRESSED    = 0x01;
        /// Reserved for future end-to-end encryption.
        const ENCRYPTED     = 0x02;
        /// Indicates this is a chunk of a larger stream.
        const STREAMED      = 0x04;
        /// Indicates more data follows in the stream.
        const PARTIAL       = 0x08;
        /// Bypasses standard queue limits (e.g. cancellation signals).
        const HIGH_PRIORITY = 0x10;
    }
}

use serde_repr::{Deserialize_repr, Serialize_repr};

/// Identifies the core routing semantic of the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum EnvelopeType {
    /// Guest → Host. Used for calling host APIs (e.g., `fs.read`, `fetch`).
    Request = 0,
    /// Host → Guest. Used to resolve/reject a previous Request.
    Response = 1,
    /// Host → Guest. One-way pub/sub events (e.g., Theme changes).
    Event = 2,
    /// Guest → Host. Specialized, highly optimized envelope for React Reconciler updates.
    UiSync = 3,
    /// Guest → Host. Aborts an underlying active task.
    Cancel = 4,
}

/// The universal, zero-overhead envelope array.
/// Serialized format: `[Version, Flags, Type, ID, Method, Payload]`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcEnvelope(
    pub u8,           // 0: Version
    pub IpcFlags,     // 1: Flags (Bitmask)
    pub EnvelopeType, // 2: Type
    pub u32,          // 3: Request ID / Stream ID
    pub String,       // 4: Method or Event name
    pub rmpv::Value,  // 5: Payload (Abstract MessagePack Value)
);

impl IpcEnvelope {
    /// Factory for generating standard Guest->Host requests.
    pub fn new_request(id: u32, method: impl Into<String>, payload: rmpv::Value) -> Self {
        Self(
            IPC_VERSION,
            IpcFlags::empty(),
            EnvelopeType::Request,
            id,
            method.into(),
            payload,
        )
    }

    /// Factory for generating Host->Guest responses.
    pub fn new_response(id: u32, payload: rmpv::Value) -> Self {
        Self(
            IPC_VERSION,
            IpcFlags::empty(),
            EnvelopeType::Response,
            id,
            "".into(), // Method unused in response
            payload,
        )
    }

    /// Factory for Host->Guest pub/sub events.
    pub fn new_event(event_name: impl Into<String>, payload: rmpv::Value) -> Self {
        Self(
            IPC_VERSION,
            IpcFlags::empty(),
            EnvelopeType::Event,
            0, // ID unused in global events
            event_name.into(),
            payload,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_serialization() {
        let env = IpcEnvelope::new_request(42, "fs.read", rmpv::Value::String("test.txt".into()));

        let buf = rmp_serde::to_vec(&env).unwrap();
        // A struct representation would serialize field names like "version", "flags", etc.
        // A tuple representation serializes as a bare array: [1, 0, 0, 42, "fs.read", "test.txt"]

        let expected_array_marker = 0x96; // fixarray of length 6
        assert_eq!(buf[0], expected_array_marker);

        let decoded: IpcEnvelope = rmp_serde::from_slice(&buf).unwrap();
        assert_eq!(decoded.2, EnvelopeType::Request);
        assert_eq!(decoded.3, 42);
        assert_eq!(decoded.4, "fs.read");
    }

    #[test]
    fn test_binary_compatibility_golden() {
        let env =
            IpcEnvelope::new_request(1024, "test.method", rmpv::Value::String("hello".into()));

        let buf = rmp_serde::to_vec(&env).unwrap();

        // Golden bytes for Version=1, Flags=0, Type=Request(0), ID=1024, "test.method", "hello"
        // Ensures future changes do not accidentally break binary representation.
        let golden: &[u8] = &[
            0x96, // fixarray of 6
            0x01, // version: 1
            0x00, // flags: 0
            0x00, // type: 0
            0xcd, 0x04, 0x00, // ID: 1024 (u16 representation in MessagePack)
            0xab, 0x74, 0x65, 0x73, 0x74, 0x2e, 0x6d, 0x65, 0x74, 0x68, 0x6f,
            0x64, // "test.method"
            0xa5, 0x68, 0x65, 0x6c, 0x6c, 0x6f, // "hello"
        ];
        assert_eq!(buf, golden);
    }
}
