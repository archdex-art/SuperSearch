# PHASE 1 — INDUSTRY RESEARCH

**Prepared By:** Principal Engineering Team

### Synthesis: The SuperSearch Manifesto

#### 1. What SuperSearch Should ADOPT
*   **Raycast’s DX (Custom Reconciler):** Declarative React components decoupling runtime from DOM.
*   **Tauri/Warp’s Execution Philosophy:** Low-memory, fast-boot execution.
*   **Chrome’s Ephemeral Lifecycle:** Extensions suspend/kill when idle.
*   **Anthropic’s MCP:** Manifests compile to MCP Tools for the AI Agent.
*   **VS Code’s IPC Separation:** Separate thread/sandbox for execution.

#### 2. What SuperSearch Should IMPROVE
*   **The Raycast Memory Problem:** Avoid 1 OS process per extension. Use ultra-lightweight runtimes.
*   **Marketplace Pipeline:** Fully automated CI/CD, signed binaries, edge CDNs.

#### 3. What SuperSearch Should AVOID
*   **WebViews / iframes:** Massive RAM overhead, bad UX.
*   **Obsidian-style DOM Access:** Extensions must never access `window` or `document`.
*   **Shared Mutable State:** Execution must be isolated per extension.
*   **Unrestricted OS Access:** All capabilities must be explicitly requested.
