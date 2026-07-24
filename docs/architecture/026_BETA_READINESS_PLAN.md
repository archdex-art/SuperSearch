# PUBLIC BETA READINESS PLAN (POST-V1.0)

*Capturing the final operational, transparency, and disaster recovery policies required before opening the platform to the public developer beta.*

### 1. Marketplace Transparency
The `extensions.json` payload is expanded to expose verifiable metadata to the end-user:
*   `publisher`: Verified GitHub organization or user handle.
*   `publication_timestamp`: ISO-8601 timestamp of the CI/CD merge.
*   `checksum`: SHA-256 hash of the bundle to allow local integrity verification prior to Ed25519 signature checks.

### 2. Update & Rollback Policy
*   **Staged Rollouts:** Updates are pushed to the `Canary` channel first. If crash telemetry remains below 1%, the Edge CDN promotes the signature to the `Stable` channel.
*   **Auto-Update:** The Host polls the CDN every 12 hours. Updates apply seamlessly to `Unloaded` extensions.
*   **Auto-Revert:** If an updated extension panics 3 times consecutively during `Activate`, the Host modifies the local SQLite registry to pin the extension to the previous known-good signature and alerts the user.

### 3. Extended Observability
The following critical operational metrics are wired into the Host telemetry dashboard for the Beta launch:
*   `marketplace_sync_latency`: Measures edge routing performance.
*   `signature_verification_failures`: Flags potential MitM attempts or compromised CDNs.
*   `context_eviction_frequency`: Identifies if `MAX_CONTEXT_PROVIDERS` is too restrictive.
*   `mcp_compilation_failures`: Tracks malformed tool generation from edge-case manifests.

### 4. Disaster Recovery (DR)
*   **CDN Outage:** If the Edge CDN is unreachable, the Host falls back to the locally cached `extensions.json` and existing signed bundles. The platform remains 100% operational offline.
*   **Corrupted Index:** If the `extensions.json` signature check fails, the Host immediately drops the payload and triggers a `DisasterRecovery::RegistryCorrupted` telemetry event, retaining the last known-good index.
*   **Trust Anchor Compromise:** If the Marketplace Private Key is compromised, a Host application update via Tauri Updater is deployed containing the new public key and an integrated Certificate Revocation List (CRL) for the old key.
