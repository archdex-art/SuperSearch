//! # Bundle Signature Verification (Phase 9)
//!
//! Enforces the cryptographic Chain of Trust for extension execution.
//! Every bundle entering the `deno_core` sandbox MUST have a valid Ed25519 signature
//! matching the embedded Marketplace Public Key, unless the Host is explicitly
//! configured to run in Developer Mode.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// The embedded SuperSearch Marketplace Public Key (Ed25519).
/// In production, this is rotated periodically via auto-updates (Phase 9 §5).
/// For this milestone, we use a constant placeholder representing the public trust anchor.
pub const MARKETPLACE_PUBLIC_KEY: &[u8; 32] = &[
    0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x7a, 0x8b, 0x9c, 0xad, 0xbe, 0xcf, 0xd0, 0xe1, 0xf2, 0x03,
    0x14, 0x25, 0x36, 0x47, 0x58, 0x69, 0x7a, 0x8b, 0x9c, 0xad, 0xbe, 0xcf, 0xd0, 0xe1, 0xf2, 0x03,
];

#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("Invalid public key format provided by the Marketplace Trust Anchor.")]
    InvalidPublicKey,
    #[error("Invalid signature format parsed from the extension bundle.")]
    InvalidSignatureFormat,
    #[error("Signature verification failed. The extension bundle has been tampered with or signed by an untrusted entity.")]
    VerificationFailed,
    #[error("Unsigned bundles are strictly rejected in Production Mode.")]
    UnsignedBundle,
}

/// Verifies a JavaScript/WASM bundle against an Ed25519 cryptographic signature.
pub fn verify_bundle(
    bundle_bytes: &[u8],
    signature_bytes: &[u8],
    pubkey_bytes: &[u8],
) -> Result<(), SignatureError> {
    // 1. Decode Trust Anchor
    let public_key = VerifyingKey::from_bytes(
        pubkey_bytes
            .try_into()
            .map_err(|_| SignatureError::InvalidPublicKey)?,
    )
    .map_err(|_| SignatureError::InvalidPublicKey)?;

    // 2. Decode Bundle Signature
    let signature = Signature::from_slice(signature_bytes)
        .map_err(|_| SignatureError::InvalidSignatureFormat)?;

    // 3. Cryptographic Verification
    public_key
        .verify(bundle_bytes, &signature)
        .map_err(|_| SignatureError::VerificationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn test_valid_signature_succeeds() {
        let secret_bytes: [u8; 32] = [
            157, 097, 177, 157, 239, 253, 090, 096, 186, 132, 074, 244, 146, 236, 044, 196, 068,
            073, 197, 105, 123, 050, 105, 025, 112, 059, 172, 003, 028, 174, 127, 096,
        ];
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let public_key = VerifyingKey::from(&signing_key);

        let bundle = b"console.log('Safe code');";
        let signature = signing_key.sign(bundle);

        // Validation should succeed
        assert!(verify_bundle(bundle, &signature.to_bytes(), public_key.as_bytes()).is_ok());
    }

    #[test]
    fn test_tampered_bundle_fails() {
        let secret_bytes: [u8; 32] = [
            157, 097, 177, 157, 239, 253, 090, 096, 186, 132, 074, 244, 146, 236, 044, 196, 068,
            073, 197, 105, 123, 050, 105, 025, 112, 059, 172, 003, 028, 174, 127, 096,
        ];
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let public_key = VerifyingKey::from(&signing_key);

        let bundle = b"console.log('Safe code');";
        let signature = signing_key.sign(bundle);

        // Attack: Modify the bundle after it was signed
        let tampered_bundle = b"console.log('Malicious code');";

        let result = verify_bundle(
            tampered_bundle,
            &signature.to_bytes(),
            public_key.as_bytes(),
        );
        assert!(matches!(result, Err(SignatureError::VerificationFailed)));
    }
}
