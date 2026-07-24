//! # Marketplace Client (Phase 11)
//!
//! Handles fetching the global extension registry (`extensions.json`) from the Edge CDN.
//! Enforces Ed25519 signature verification on the registry file itself to prevent
//! Man-in-the-Middle (MitM) attacks where an attacker swaps the registry to point
//! to malicious extension bundles.

use crate::capability::signature::{verify_bundle, SignatureError, MARKETPLACE_PUBLIC_KEY};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Represents a single extension listed in the global Edge CDN index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteExtensionInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub download_url: String,
    pub signature_url: String,
    pub health_score: f64,
}

/// The structure of `extensions.json` hosted on the Edge CDN.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceIndex {
    pub schema_version: String,
    pub updated_at: String,
    pub extensions: Vec<RemoteExtensionInfo>,
}

#[derive(Debug, thiserror::Error)]
pub enum MarketplaceError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Cryptographic verification failed: {0}")]
    Signature(#[from] SignatureError),
    #[error("Invalid JSON Schema: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Fetches the global `extensions.json` index and its detached signature from the CDN.
/// Validates the signature before deserializing the JSON to guarantee authenticity.
pub async fn fetch_and_verify_index(
    client: &Client,
    cdn_base_url: &str,
) -> Result<MarketplaceIndex, MarketplaceError> {
    let index_url = format!("{}/extensions.json", cdn_base_url);
    let sig_url = format!("{}/extensions.json.sig", cdn_base_url);

    // Fetch the detached signature first
    let signature_bytes = client.get(&sig_url).send().await?.bytes().await?;

    // Fetch the raw JSON payload
    let index_bytes = client.get(&index_url).send().await?.bytes().await?;

    // Verify the integrity of the JSON payload against the Marketplace Trust Anchor
    verify_bundle(&index_bytes, &signature_bytes, MARKETPLACE_PUBLIC_KEY)?;

    // Only parse the JSON if the cryptographic signature is flawless
    let index: MarketplaceIndex = serde_json::from_slice(&index_bytes)?;
    Ok(index)
}
