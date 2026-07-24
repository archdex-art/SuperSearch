# PHASE 12 — DEVELOPER EXPERIENCE (DX)

**Prepared By:** Principal Engineering Team
**Objective:** Architect a world-class local toolchain for extension developers. The DX must provide instant feedback, robust debugging, and seamless scaffolding, rivaling the experience of building a modern web app.

---

### 1. The CLI (`@supersearch/cli`)

The CLI is the entry point for all developer interactions. It is distributed via npm/npx.

*   **`supersearch create <name>`:** Scaffolds a new project using standardized React + TypeScript templates. Initializes Git, installs dependencies, and configures ESLint/Prettier.
*   **`supersearch dev`:** Starts the local development server, watcher, and WebSocket bridge (detailed below).
*   **`supersearch build`:** Compiles the extension via `esbuild`, performs tree-shaking, enforces the 5MB size quota, and outputs the production bundle.
*   **`supersearch publish`:** Automates the Marketplace submission pipeline (Lints, builds, and opens a PR against the `supersearch/extensions` GitHub monorepo).

---

### 2. Hot Module Replacement (HMR) & Dev Mode

Developers require instant visual feedback when changing UI code. We leverage **React Fast Refresh** over a custom WebSocket bridge.

**The HMR Pipeline:**
1.  **File Edit:** Developer saves `src/Command.tsx`.
2.  **CLI Watcher:** `esbuild` recompiles the specific module in `< 50ms`.
3.  **WebSocket Broadcast:** The CLI sends the updated module payload to the local SuperSearch Desktop App (which is listening on a local dev port, e.g., `ws://localhost:9999`).
4.  **Host Injection:** The Rust Host receives the payload and passes it into the active V8 Isolate.
5.  **Reconciler Patch:** The custom React Reconciler applies the Fast Refresh boundary, updating the UI Tree without losing the developer's local React state (e.g., text typed into a `<Form.TextField>` remains intact).

---

### 3. Debugger Integration (V8 Inspector)

Because extensions run inside isolated V8 contexts, `console.log` is insufficient for complex state debugging. We must expose standard debugging tools.

*   **V8 Inspector Protocol:** `deno_core` natively supports the Chrome V8 Inspector Protocol.
*   **Developer Workflow:** When running `supersearch dev`, the Rust Host opens an inspector port (e.g., `9229`).
*   **Tooling:** Developers can open `chrome://inspect` or attach VS Code's debugger to this port.
*   **Capabilities:** This allows developers to set line-level breakpoints in their TypeScript source (via source maps), inspect closures, view memory heaps, and profile CPU execution directly inside the extension sandbox.

---

### 4. Local Simulator vs. Host Fidelity

*   **Decision:** We reject building a standalone "Simulator" (e.g., a mock web app). Simulators inevitably drift from true native behavior, leading to bugs that only appear in production.
*   **Dev Mode Integration:** `supersearch dev` connects directly to the developer's actual SuperSearch Desktop Application. The extension runs in the exact same Rust/Tauri host environment, ensuring 100% fidelity for UI rendering, OS system APIs, and capability gates.

---

### 5. Testing Framework (`@supersearch/testing`)

To encourage high-quality marketplace submissions, the SDK includes a first-party testing utility designed to run in standard Node.js/Vitest environments.

*   **Mocked Reconciler:** `render(<MyCommand />)` outputs the exact MessagePack/JSON tree that would be sent to the host, allowing developers to write snapshot tests for their UI.
*   **Mocked Capabilities:** Developers can simulate denied capabilities:
    ```typescript
    import { mockCapability } from "@supersearch/testing";
    
    test("handles missing clipboard permission", async () => {
      mockCapability("clipboard-write", "denied");
      // Assert that the UI renders the specific fallback error state
    });
    ```
*   **Mocked Storage:** Provides in-memory implementations of the SQLite `LocalStorage` and `SecretStore` APIs, completely isolated between test runs.

---

### 6. Error Overlays

When an unhandled exception or rendering error occurs during development:
*   The Reconciler catches the error boundary.
*   The Host renders a highly visible **Redbox Error Overlay**.
*   The overlay includes the decoded stack trace (mapped back to the original TypeScript source via inline sourcemaps) and a clickable link that opens the exact file and line in the developer's default IDE (e.g., `vscode://file/...`).
