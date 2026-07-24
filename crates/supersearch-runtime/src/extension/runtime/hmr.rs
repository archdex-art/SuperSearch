//! # Hot Module Replacement (HMR) Server
//!
//! Exposes a local WebSocket server (port 9999) when the Host runs in Developer Mode.
//! The `@supersearch/cli` connects to this socket and pushes recompiled JavaScript bundles
//! (via React Fast Refresh) without requiring the extension to restart or lose internal React state.

use tokio::net::TcpListener;
use tokio_stream::StreamExt;
use tokio_tungstenite::accept_async;

use tokio::sync::mpsc;
use tracing::{error, info};

/// Starts the HMR WebSocket listener on `ws://127.0.0.1:9999`.
/// Injects incoming bundle updates into the provided channel to trigger a Hot Reload in V8.
pub async fn start_hmr_server(
    reload_tx: mpsc::Sender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = "127.0.0.1:9999";
    let listener = TcpListener::bind(&addr).await?;
    info!("HMR WebSocket server listening on ws://{}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let reload_tx = reload_tx.clone();
        tokio::spawn(async move {
            let mut ws_stream = match accept_async(stream).await {
                Ok(ws) => ws,
                Err(e) => {
                    error!("Error during HMR WebSocket handshake: {}", e);
                    return;
                }
            };

            info!("CLI Connected to HMR Server.");

            while let Some(msg) = ws_stream.next().await {
                if let Ok(msg) = msg {
                    if msg.is_text() {
                        let text = msg.to_text().unwrap_or_default();

                        // Parse the incoming JSON envelope { "type": "hmr_update", "code": "..." }
                        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(text) {
                            if payload.get("type").and_then(|t| t.as_str()) == Some("hmr_update") {
                                if let Some(code) = payload.get("code").and_then(|c| c.as_str()) {
                                    info!(
                                        "Received HMR update from CLI. Pushing to active isolates."
                                    );
                                    let _ = reload_tx.send(code.to_string()).await;
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    Ok(())
}
