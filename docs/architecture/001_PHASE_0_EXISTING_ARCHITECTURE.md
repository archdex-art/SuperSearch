# PHASE 0 & 0.5 — EXISTING ARCHITECTURE ANALYSIS

**Prepared By:** Principal Engineering Team
**Target:** SuperSearch (Tauri 2.0 + Rust + React 18)

SuperSearch is a highly optimized native desktop application prioritizing low memory footprint and thread-safe execution.

### 1. Existing Architecture Report
*   **Frontend (UI Layer):** React 18 application bundled with Vite. State is managed via native React Hooks (no Redux/Zustand).
*   **Backend (Host & Runtime):** Split into `src-tauri` (window lifecycle, OS integrations) and `supersearch-runtime` (custom priority-queue task scheduler, AI agent controller, capability-based security model).
*   **Targeted Sandbox Model:** WebAssembly (WASM) scaffolding exists, moving away from heavy Node.js/V8 sidecars.

### 2. Integration Points
1.  **Frontend Reconciler Target:** Extend `CommandPalette.tsx`.
2.  **IPC Routing:** Upgrade `bridge.ts` and `src-tauri/src/commands/` for streaming/events.
3.  **Command Pipeline:** Hook into `src/commands/search.rs`.
4.  **Security Gates:** Integrate SDK directly into `capability/` framework.

### 3. Command Lifecycle
Keyboard Shortcut -> Command Palette -> Search Pipeline -> Extension Registry -> Runtime Loader -> Extension -> JSON UI Tree -> React Renderer

### 4. Extension Lifecycle
[Install] -> [Discover] -> [Index] -> [Load] -> [Activate] -> [Suspend] -> [Resume] -> [Unload] -> [Remove]
*(Plus: Update, Disable, Enable, Crash, Restart, Health Check)*

### 5. System Constraints
1.  **Must remain Tauri-native.**
2.  **Performance:** Startup overhead <50ms. Search pipeline <10ms latency.
3.  **UI Consistency:** Global CSS/layout protected.
4.  **AI-First:** Native tool-calling by AgentController.
