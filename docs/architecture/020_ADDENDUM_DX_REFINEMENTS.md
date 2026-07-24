# PHASE 12 ADDENDUM — DX REFINEMENTS & WORKFLOW

*Formalizing the developer lifecycle, diagnostics, and profiling tooling to ensure a frictionless onboarding experience for extension authors.*

### 1. Development Profiles
The CLI supports multiple execution modes to balance iteration speed with diagnostic depth:
*   `--mode fast` (Default): Optimizes for HMR speed. Bypasses deep type-checking on every save.
*   `--mode debug`: Attaches the V8 Inspector, generates full source maps, and enables verbose Rust IPC logging.
*   `--mode profile`: Injects performance tracing hooks to measure CPU, memory, and React render timings.
*   `--mode production`: Runs `esbuild` with minification and tree-shaking to test exact release artifacts.

### 2. HMR Boundaries (Limitations)
While React Fast Refresh handles UI updates seamlessly, certain changes require a full CLI restart and V8 Isolate reboot:
*   Edits to `package.json` (Capabilities, Commands, Preferences).
*   Changes to the `@supersearch/api` SDK version.
*   Modifying the entry-point file path.

### 3. Developer Diagnostics & Dashboard
*   **`supersearch doctor`:** A CLI utility that validates the developer's environment (Node.js version, Host App compatibility, valid Ed25519 signing keys, and linting rules).
*   **The Dev Dashboard:** When running in dev mode, the Host renders a hidden diagnostic panel (accessible via a hotkey). It displays:
    *   Active V8 Isolate memory heap usage.
    *   Real-time IPC latency and React reconciliation times.
    *   A live audit log of Capability requests (e.g., "Network request to `api.github.com` allowed").

### 4. Source Map Strategy
*   **Development:** Source maps are rendered *inline* (Data URIs). This allows zero-configuration debugging out of the box when developers open `chrome://inspect`.
*   **Production:** Source maps are separated into `.map` files. They are uploaded to the Marketplace Telemetry server for crash-reporting stack decoding, but are **not** distributed to client machines, saving bandwidth and protecting intellectual property.

---

### APPENDIX: THE DEVELOPER WORKFLOW
A reference guide for the standard extension authoring lifecycle.

1.  **Create:** `npx @supersearch/cli create my-extension` (Selects React/TS template).
2.  **Develop:** `cd my-extension && npm run dev` (Connects to the desktop app via WebSocket).
3.  **Test:** `npm test` (Runs Vitest against the `@supersearch/testing` mocked reconciler).
4.  **Debug:** Add `debugger;` to code, open `chrome://inspect`, step through logic.
5.  **Profile:** Run `npm run dev --mode profile` to optimize list virtualization or heavy computation.
6.  **Publish:** `npm run publish` (Automates linting, builds the bundle, and opens a GitHub PR).
7.  **Upgrade:** Run `npx @supersearch/cli upgrade` to bump the SDK and auto-migrate deprecated APIs via AST codemods.
