# PHASE 9 ADDENDUM — OPERATIONAL SECURITY & LIFECYCLE

*Integrating architectural feedback to solidify the Trusted Computing Base, cryptographic lifecycles, and incident response before implementation.*

### 1. Refined Trust & Isolation Statement
If a capability is not registered during Sandbox Allocation, guest JavaScript has no supported mechanism to invoke that operation. The remaining trusted computing base consists of the Rust host, V8, and registered native bindings, which must be implemented safely and audited regularly.

### 2. Trusted Computing Base (TCB) Definition

| Trusted Components | Untrusted Components |
| :--- | :--- |
| • Rust Runtime (`supersearch-runtime`)<br>• `deno_core` & V8 Engine<br>• Capability Resolver & Gate<br>• IPC Router<br>• Signature Verifier | • Extension JS Bundle<br>• Extension npm Dependencies<br>• Manifest Contents (parsed defensively)<br>• External Network Responses |

### 3. Supply-Chain Security Operations
To protect against compromised dependencies before an extension reaches the user:
*   **Lockfile Verification:** CI strictly rejects submissions lacking a `package-lock.json`, `yarn.lock`, or `pnpm-lock.yaml`.
*   **Reproducible Builds:** The Marketplace build server enforces deterministic outputs.
*   **Provenance Attestations:** Generating Sigstore / SLSA Level 2+ attestations for every signed bundle.
*   **Vulnerability Disclosure:** An automated pipeline utilizing `npm audit` and OSV (Open Source Vulnerabilities) to quarantine extensions if a zero-day is discovered in their dependency tree.

### 4. Capability Lifecycle
Capabilities are not static; they evolve with the extension.
*   **Install-Time:** Prompt displays all requested capabilities.
*   **Upgrade-Time:** If an update requests *new* capabilities (e.g., v1.1 adds `fs.read`), the user is prompted to authorize the delta before the update applies. If denied, the extension remains on v1.0.
*   **Runtime Revocation:** Users can toggle individual capabilities off via the Host Settings UI. The SDK gracefully handles the resulting `CapabilityError`.

### 5. Cryptographic Key Management
*   **Key Rotation:** Marketplace Ed25519 signing keys are rotated annually.
*   **Trust Anchor Updates:** Public keys are hardcoded into the Host application binary and updated securely via the Host's auto-updater (Tauri Updater).
*   **Compromised Key Response:** If a signing key is compromised, a revocation list (CRL) is pushed immediately via the telemetry endpoint, invalidating all bundles signed by that key.

### 6. Audit Logging
Security events are aggregated locally and (if telemetry is enabled) transmitted to the Marketplace for threat intelligence:
*   Capability Denials (attempting to use an ungranted API).
*   Signature Verification Failures.
*   Repeated Crash / Panic loops.
*   Excessive IPC Floods (Triggering backpressure drops).

### 7. Security Response Lifecycle (Incident Flow)
When a zero-day or malicious extension bypasses the Marketplace review:
`Threat Detected` → `Telemetry Flags Anomaly` → `Automated Detection` → `Quarantine (Marketplace Listing Removed)` → `Revocation List Updated` → `Host Client Syncs List` → `Extension Disabled Locally` → `User Notification Dispatched` → `Recovery / Safe Uninstall`.

### 8. Security Properties Summary

| Property | Status | Mechanism |
| :--- | :--- | :--- |
| **Capability Isolation** | ✅ | Native V8 Ops Injection |
| **Memory Isolation** | ✅ | Separate V8 Isolate Heaps |
| **Secret Isolation** | ✅ | OS Keychain + Opaque Handles |
| **Bundle Signing** | ✅ | Ed25519 Cryptography |
| **Dependency Scanning** | ✅ | CI/CD Automated Audits |
| **Runtime Quotas** | ✅ | Tokio Watchdog (CPU/RAM) |
| **Audit Logging** | ✅ | Telemetry Event Aggregation |
| **Incident Response** | ✅ | Remote Revocation / Kill Switch |
