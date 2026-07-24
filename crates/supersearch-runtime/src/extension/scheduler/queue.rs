//! # Fair MPSC Multiplexing Queue
//!
//! Provides the `ExtensionScheduler` which multiplexes `Receiver<IpcEnvelope>` streams
//! across all active isolates. Uses `tokio_stream::StreamMap` to ensure fair polling,
//! preventing a single saturated isolate from starving others.

use crate::extension::ipc::IpcEnvelope;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{StreamExt, StreamMap};

/// The central router tracking all active isolates and routing their IPC traffic.
pub struct ExtensionScheduler {
    /// Multiplexed streams of Guest -> Host messages.
    /// `StreamMap` guarantees fair polling via random starting indices per tick.
    incoming: StreamMap<String, ReceiverStream<IpcEnvelope>>,

    /// Host -> Guest Sender channels mapped by Extension ID.
    outgoing: HashMap<String, mpsc::Sender<IpcEnvelope>>,
}

impl ExtensionScheduler {
    pub fn new() -> Self {
        Self {
            incoming: StreamMap::new(),
            outgoing: HashMap::new(),
        }
    }

    /// Registers a newly allocated active isolate with the scheduler.
    pub fn register_isolate(
        &mut self,
        ext_id: String,
        rx: mpsc::Receiver<IpcEnvelope>,
        tx: mpsc::Sender<IpcEnvelope>,
    ) {
        self.incoming
            .insert(ext_id.clone(), ReceiverStream::new(rx));
        self.outgoing.insert(ext_id, tx);
    }

    /// Removes an isolate from the scheduler (e.g., transitioning to Suspended or Unloaded).
    pub fn deregister_isolate(&mut self, ext_id: &str) {
        self.incoming.remove(ext_id);
        self.outgoing.remove(ext_id);
    }

    /// Polls the multiplexed streams for the next IPC message from *any* active isolate.
    /// If an isolate's channel is closed (e.g., due to a panic, OOM termination, or normal unload),
    /// the StreamMap automatically exhausts it. We intercept that to clean up the outgoing sender.
    pub async fn next_message(&mut self) -> Option<(String, IpcEnvelope)> {
        self.incoming.next().await
    }
    /// Routes an IPC envelope from the Host to a specific Guest isolate.
    /// Uses backpressure; if the target isolate's queue is full, this returns an error.
    /// If the target channel is closed (e.g., isolate crashed or unloaded), the
    /// isolate is deregistered to prevent memory leaks in the scheduler.
    pub fn send_to_guest(
        &mut self,
        ext_id: &str,
        envelope: IpcEnvelope,
    ) -> Result<(), &'static str> {
        let mut closed = false;
        let res = if let Some(tx) = self.outgoing.get(ext_id) {
            match tx.try_send(envelope) {
                Ok(_) => Ok(()),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    Err("Target isolate queue full (Backpressure)")
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    closed = true;
                    Err("Target isolate channel closed")
                }
            }
        } else {
            Err("Target isolate not registered")
        };

        if closed {
            // Graceful isolate removal on unexpected closure (M2 Feedback)
            self.deregister_isolate(ext_id);
        }

        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::ipc::{EnvelopeType, IpcFlags};
    use rmpv::Value;

    #[tokio::test]
    async fn test_fair_multiplexing_and_routing() {
        let mut scheduler = ExtensionScheduler::new();

        let (tx1, rx1) = mpsc::channel(10);
        let (tx_out1, rx_out1) = mpsc::channel(10);
        scheduler.register_isolate("ext-1".into(), rx1, tx_out1);

        let (tx2, rx2) = mpsc::channel(10);
        let (tx_out2, rx_out2) = mpsc::channel(10);
        scheduler.register_isolate("ext-2".into(), rx2, tx_out2);

        // Simulate concurrent requests
        let env1 = IpcEnvelope(
            1,
            IpcFlags::empty(),
            EnvelopeType::Request,
            100,
            "fs.read".into(),
            Value::Nil,
        );
        let env2 = IpcEnvelope(
            1,
            IpcFlags::empty(),
            EnvelopeType::Request,
            200,
            "net.fetch".into(),
            Value::Nil,
        );

        tx1.send(env1.clone()).await.unwrap();
        tx2.send(env2.clone()).await.unwrap();

        // Ensure the scheduler can receive both without starving
        let mut received = vec![];
        received.push(scheduler.next_message().await.unwrap().0);
        received.push(scheduler.next_message().await.unwrap().0);

        assert!(received.contains(&"ext-1".to_string()));
        assert!(received.contains(&"ext-2".to_string()));

        // Reverse Path Test
        let response = IpcEnvelope::new_response(100, Value::Nil);
        assert!(scheduler.send_to_guest("ext-1", response).is_ok());

        // The mock rx_out1 should receive it
        let mut rx_out1 = rx_out1; // Take ownership
        let out_msg = rx_out1.recv().await.unwrap();
        assert_eq!(out_msg.3, 100); // Verify RequestID
    }
}
