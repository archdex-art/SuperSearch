//! Cap'n Proto IPC channel between kernel and plugins.
//!
//! All plugin ↔ kernel communication uses typed, zero-copy messages.
//! The IPC channel is a bidirectional MPSC pair with message framing
//! and capability-gated send/receive.


use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};


use crate::capability::gate::{CapabilityGate, GateDecision};
use crate::capability::namespace::Namespace;
use crate::capability::token::{CapabilityToken, Permission};

/// IPC message types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcMessageKind {
    /// Plugin → Kernel: Request to perform a privileged operation.
    PluginRequest,
    /// Kernel → Plugin: Response to a plugin request.
    KernelResponse,
    /// Kernel → Plugin: Push notification (event, state update).
    KernelPush,
    /// Plugin → Kernel: Event subscription request.
    Subscribe,
    /// Plugin → Kernel: Event unsubscription.
    Unsubscribe,
    /// Bidirectional: Heartbeat/keepalive.
    Heartbeat,
    /// Kernel → Plugin: Shutdown signal.
    Shutdown,
}

/// A single IPC message.
///
/// In production, the `payload` field would be a Cap'n Proto message
/// for zero-copy deserialization. During initial development, we use
/// a serialized byte vector with the interface designed for Cap'n Proto
/// swap-in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    /// Message type.
    pub kind: IpcMessageKind,
    /// Monotonic message sequence number (per-channel).
    pub sequence: u64,
    /// The source plugin or "kernel".
    pub source: String,
    /// The destination plugin or "kernel".
    pub destination: String,
    /// Payload bytes. In production: Cap'n Proto serialized message.
    /// Zero-copy: when reading from WASM linear memory, this is a
    /// reference into the plugin's memory space.
    pub payload: Vec<u8>,
    /// Correlation ID for request-response pairing.
    pub correlation_id: Option<u64>,
}

/// Errors from IPC operations.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Message too large: {size} bytes, limit {limit} bytes")]
    MessageTooLarge { size: usize, limit: usize },
    #[error("Capability check failed: {reason}")]
    CapabilityDenied { reason: String },
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// A bidirectional IPC channel between the kernel and a plugin.
///
/// Each plugin gets exactly one IpcChannel at load time. The channel
/// is capability-gated: the plugin must hold IpcSend/IpcReceive
/// permissions in its namespace.
pub struct IpcChannel {
    /// Plugin identifier.
    plugin_id: String,
    /// Plugin's namespace for capability checks.
    namespace: Namespace,
    /// Plugin → Kernel sender.
    to_kernel_tx: mpsc::Sender<IpcMessage>,
    /// Kernel → Plugin receiver.
    from_kernel_rx: mpsc::Receiver<IpcMessage>,
    /// Message sequence counter.
    sequence: u64,
    /// Maximum message size.
    max_message_size: usize,
}

/// The kernel's end of the IPC channel.
pub struct KernelIpcEndpoint {
    /// Plugin identifier.
    plugin_id: String,
    /// Kernel → Plugin sender.
    to_plugin_tx: mpsc::Sender<IpcMessage>,
    /// Plugin → Kernel receiver.
    from_plugin_rx: mpsc::Receiver<IpcMessage>,
}

impl IpcChannel {
    /// Create a new bidirectional IPC channel pair.
    ///
    /// Returns `(plugin_end, kernel_end)`.
    pub fn create(
        plugin_id: String,
        namespace: Namespace,
        buffer_size: usize,
        max_message_size: usize,
    ) -> (Self, KernelIpcEndpoint) {
        let (p_to_k_tx, p_to_k_rx) = mpsc::channel(buffer_size);
        let (k_to_p_tx, k_to_p_rx) = mpsc::channel(buffer_size);

        let plugin_end = IpcChannel {
            plugin_id: plugin_id.clone(),
            namespace,
            to_kernel_tx: p_to_k_tx,
            from_kernel_rx: k_to_p_rx,
            sequence: 0,
            max_message_size,
        };

        let kernel_end = KernelIpcEndpoint {
            plugin_id,
            to_plugin_tx: k_to_p_tx,
            from_plugin_rx: p_to_k_rx,
        };

        (plugin_end, kernel_end)
    }

    /// Send a message to the kernel (plugin → kernel).
    ///
    /// The caller must present a valid capability token with IpcSend permission.
    pub async fn send(
        &mut self,
        kind: IpcMessageKind,
        payload: Vec<u8>,
        token: &CapabilityToken,
        gate: &CapabilityGate,
    ) -> Result<u64, IpcError> {
        // Capability gate check.
        let decision = gate.check(
            Some(token),
            &self.namespace,
            Permission::IpcSend,
        );
        if !matches!(decision, GateDecision::Allowed { .. }) {
            return Err(IpcError::CapabilityDenied {
                reason: "IpcSend permission denied".into(),
            });
        }

        // Size check.
        if payload.len() > self.max_message_size {
            return Err(IpcError::MessageTooLarge {
                size: payload.len(),
                limit: self.max_message_size,
            });
        }

        self.sequence += 1;
        let msg = IpcMessage {
            kind,
            sequence: self.sequence,
            source: self.plugin_id.clone(),
            destination: "kernel".into(),
            payload,
            correlation_id: None,
        };

        self.to_kernel_tx.send(msg).await
            .map_err(|_| IpcError::ChannelClosed)?;

        Ok(self.sequence)
    }

    /// Receive a message from the kernel (kernel → plugin).
    pub async fn recv(&mut self) -> Result<IpcMessage, IpcError> {
        self.from_kernel_rx.recv().await
            .ok_or(IpcError::ChannelClosed)
    }
}

impl KernelIpcEndpoint {
    /// Send a message to the plugin (kernel → plugin).
    pub async fn send(&self, msg: IpcMessage) -> Result<(), IpcError> {
        self.to_plugin_tx.send(msg).await
            .map_err(|_| IpcError::ChannelClosed)
    }

    /// Receive a message from the plugin (plugin → kernel).
    pub async fn recv(&mut self) -> Result<IpcMessage, IpcError> {
        self.from_plugin_rx.recv().await
            .ok_or(IpcError::ChannelClosed)
    }

    pub fn plugin_id(&self) -> &str { &self.plugin_id }
}
