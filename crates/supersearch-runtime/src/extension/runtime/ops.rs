#![allow(unused_imports)]

//! # Runtime Operations (Phase 8 IPC)
//!
//! Provides the `deno_core::op2` fast APIs that bridge the Rust and V8 memory spaces.
//! These ops handle MessagePack deserialization, payload limits, and backpressure
//! via the `mpsc` queue.

use crate::extension::ipc::IpcEnvelope;
use deno_core::{op2, OpState};
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

// Payload size constraint (50MB) defined in M2 review feedback to reject oversized payloads before they enter the scheduler.
const MAX_PAYLOAD_BYTES: usize = 50 * 1024 * 1024;

/// Submits an IPC envelope from the Guest V8 Isolate to the Rust Host Scheduler.
#[op2(fast)]
pub fn op_ipc_post(
    state: &mut OpState,
    #[buffer] payload: &[u8],
) -> Result<(), deno_error::JsErrorBox> {
    // 1. Strict Payload Limits
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(deno_error::JsErrorBox::generic(format!(
            "PayloadTooLarge: IPC payload of {} bytes exceeds the 50MB limit.",
            payload.len()
        )));
    }

    // 2. Binary Compatibility & Deserialization
    let envelope: IpcEnvelope = rmp_serde::from_slice(payload).map_err(|e| {
        deno_error::JsErrorBox::generic(format!(
            "IpcMalformed: Failed to parse MessagePack envelope: {}",
            e
        ))
    })?;

    // 3. Backpressure & Routing
    let tx = state.borrow::<mpsc::Sender<IpcEnvelope>>();
    tx.try_send(envelope).map_err(|_| {
        deno_error::JsErrorBox::generic(
            "IpcBackpressure: Host IPC queue is full. Guest must throttle.",
        )
    })?;

    Ok(())
}

/// Receives an IPC envelope from the Rust Host.
/// This is an async op that the V8 Guest awaits. It suspends the JS task until the
/// Rust host pushes a message (e.g., UI events, Responses) into the queue.
#[op2]
pub async fn op_ipc_recv(state: Rc<RefCell<OpState>>) -> Result<Vec<u8>, deno_error::JsErrorBox> {
    // 1. Extract the thread-safe Receiver from OpState
    let rx_mutex = {
        let state_ref = state.borrow();
        state_ref
            .borrow::<Rc<Mutex<mpsc::Receiver<IpcEnvelope>>>>()
            .clone()
    };

    // 2. Await the next message from the Host
    let mut rx = rx_mutex.lock().await;
    let envelope = rx.recv().await.ok_or_else(|| {
        deno_error::JsErrorBox::generic("IpcChannelClosed: The host terminated the connection.")
    })?;

    // 3. Serialize back to MessagePack for V8
    let buf = rmp_serde::to_vec(&envelope)
        .map_err(|e| deno_error::JsErrorBox::generic(format!("IpcSerializationFailed: {}", e)))?;

    Ok(buf)
}

deno_core::extension!(supersearch_ipc, ops = [op_ipc_post, op_ipc_recv]);
