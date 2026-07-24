# PHASE 11 ADDENDUM — MARKETPLACE OPERATIONS & LIFECYCLE

*Defining the operational governance, publisher trust models, and lifecycle state machines that sustain the SuperSearch ecosystem at scale.*

### 1. Extension Lifecycle State Machine
Every extension (and its specific versions) flows through a strict operational lifecycle:
`Draft` (Local) → `Submitted` (PR Opened) → `Validated` (CI Passed) → `Reviewed` (Human/Auto) → `Published` (Live on CDN) → `Installed` (On Client) → `Updated` (New version available) → `Deprecated` (Publisher marks obsolete) → `Archived` (Read-only) → `Removed` (Emergency kill-switch).

### 2. Publisher Trust & Namespace Model
*   **Authentication:** Publishers authenticate via GitHub OAuth to map identity.
*   **Verification:** Organizations (e.g., `linear`, `github`) undergo manual vetting to receive a "Verified Publisher" badge.
*   **Namespace Reservation:** Official publishers can reserve prefixes (e.g., `@linear/*`) to prevent impersonation and typosquatting.
*   **Revocation:** Malicious publishers have their certificates revoked, triggering an automatic `Removed` state for all their extensions.

### 3. Human Review Policy
While CI/CD handles 95% of submissions, manual human review is explicitly triggered when:
1.  A Publisher submits their very first extension.
2.  An update requests a *new* privileged capability (e.g., `fs.read`, `secrets`).
3.  SAST tooling detects high-entropy or obfuscated code payloads.
4.  The extension attempts to utilize experimental/preview SDK APIs.

### 4. Registry Schema Evolution (`extensions.json`)
*   **Integrity:** The registry index itself is cryptographically signed by the marketplace to prevent Man-in-the-Middle (MitM) downgrade attacks.
*   **Compression:** Served via Brotli/LZ4 for sub-millisecond parsing.
*   **Versioning:** The JSON schema is strictly versioned (`v1`, `v2`). Clients only parse schemas they support.
*   **Incremental Sync:** Clients fetch a delta patch (e.g., `extensions-diff-timestamp.json`) rather than the full 10MB index on every background sync.

### 5. Multi-Dimensional Health Scoring
Instead of a single opaque number, ranking is derived from five independent dimensions:
*   **Reliability:** Crash rate, panic counts, UI-freeze events.
*   **Performance:** P95 cold start latency, P95 render latency.
*   **Adoption:** Monthly Active Users (MAU), retention curve (uninstall rate < 24h).
*   **Maintenance:** Time since last update, open issue count on the monorepo.
*   **Security:** Number of requested capabilities (fewer capabilities = higher score).

### 6. Telemetry Transparency
*   **Opt-In:** Telemetry collection is strictly opt-in during client onboarding.
*   **Anonymization:** No PII, IPs, or query strings are ever logged. Only Extension IDs, SDK versions, and performance timings are transmitted.
*   **Retention:** Raw telemetry drops after 90 days; only aggregated health scores persist.

### 7. Release Channels
Publishers can route bundles to specific user cohorts:
*   `Internal`: Available only to the publisher's GitHub team for staging.
*   `Canary`: Available to users opting into bleeding-edge marketplace features.
*   `Beta`: Public, but flagged as unstable.
*   `Stable`: The default production channel.
