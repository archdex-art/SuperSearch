# PHASE 11 — MARKETPLACE ARCHITECTURE

**Prepared By:** Principal Engineering Team
**Objective:** Architect the central registry, automated publishing pipeline, and distribution strategy for third-party extensions. The marketplace must guarantee security, rapid discovery, and seamless version compatibility.

---

### 1. Architectural Strategy (The Monorepo Model)

To encourage a vibrant open-source ecosystem while maintaining strict security controls, SuperSearch utilizes a **GitHub-driven Monorepo** for extension submission, combined with an **Edge CDN** for client distribution.

*   **Submission:** Developers submit their source code via Pull Requests to `github.com/supersearch/extensions`.
*   **Distribution:** Once merged, CI compiles, signs, and pushes the binary bundles to a global Edge CDN (e.g., Cloudflare R2 or AWS CloudFront), ensuring sub-50ms download times globally.

---

### 2. The Publishing Pipeline (CI/CD)

The security and performance of the platform rely on automated gates rejecting bad code *before* a human reviewer ever sees it.

When a PR is opened, the GitHub Actions pipeline enforces:
1.  **Dependency Auditing:** Runs `npm audit` and cross-references OSV databases to block known vulnerabilities in `node_modules`.
2.  **Capability Diffing:** If an update to an existing extension requests a *new* capability (e.g., adding `net.fetch`), the PR is automatically flagged for manual security review.
3.  **Static Analysis (SAST):** Scans for malicious patterns (e.g., attempting to bypass the `deno_core` sandbox, obfuscated code).
4.  **Performance Linting:** Ensures the uncompressed bundle size is under the 5MB limit and tree-shaking succeeds.
5.  **Compilation & Signing:** If all checks pass and the PR is merged, the CI runner compiles the TS into a single minified JS bundle and signs it with the private Ed25519 Marketplace Key.

---

### 3. Registry & Discovery

The SuperSearch desktop application does not poll a heavy GraphQL backend to browse extensions.

*   **The Index (`extensions.json`):** The CI pipeline continuously generates a highly compressed JSON index of all approved extensions, their metadata, and health scores.
*   **Edge Delivery:** The SuperSearch client downloads this index periodically. Searching for new extensions happens entirely locally against this downloaded index, providing a zero-latency "App Store" experience.

---

### 4. Versioning & Host Compatibility

Because the `@supersearch/api` SDK evolves, extensions must be routed correctly based on the user's Host application version.

*   **SDK Constraints:** Manifests declare their required host version (e.g., `engines: { "supersearch": ">=1.2.0" }`).
*   **CDN Routing:** The client's Extension Manager checks this constraint locally before attempting a download. If the user is on v1.0.0, the marketplace UI greys out the extension and prompts an app update.
*   **SemVer Enforcement:** The CI pipeline enforces strict Semantic Versioning. A PR that breaks backward compatibility in an extension's stored data or MCP tool signature must bump the major version.

---

### 5. Analytics, Health Scores, & Ranking

To surface the best extensions and automatically bury broken ones, the Marketplace relies on anonymized telemetry rather than simple star ratings.

*   **Health Score Calculation:** 
    *   `+` Active daily users.
    *   `+` Fast cold start times.
    *   `-` High crash/panic rates.
    *   `-` High user uninstalls within 1 hour.
*   **Ranking:** When a user searches the Store for "Linear", extensions with higher Health Scores appear first. If an extension's crash rate exceeds a critical threshold, it is automatically de-listed from search results until a fix is merged.

---

### 6. Updates & Automated Rollbacks

*   **Background Updates:** The SuperSearch client checks the Edge CDN for signature-verified updates every 12 hours. Updates are applied silently in the background while the extension is `Unloaded`.
*   **Crash-Loop Rollback:** If a newly updated extension crashes 3 times sequentially upon activation, the Rust Host automatically reverts the local bundle to the previous signed version and sends an anonymized failure report to the telemetry endpoint.
