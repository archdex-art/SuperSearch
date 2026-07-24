# PHASE 2 — GAP ANALYSIS

**Prepared By:** Principal Engineering Team

### 1. Missing Architectural Blueprints (The Delta)

#### 1.1 The Extended Runtime Execution Pipeline
`Manifest` → `Registry` → `Dependency Resolver` → `Capability Resolver` → `Sandbox Allocator` → `Runtime Scheduler` → `Guest Runtime` → `SDK Host Bridge` → `JSON UI Tree` → `React Renderer`

#### 1.2 Extension Categories
*   **View Extensions:** Interactive UI.
*   **Background Extensions:** Long-running headless.
*   **Quick Actions:** Ephemeral scripts.
*   **AI Tools (MCP):** Headless functions for AgentController.
*   **System Integrations:** Menu Bar, File Providers.

#### 1.3 Event Model
*   `Lifecycle Events`: Loaded, Suspended, Crashed.
*   `UI Events`: WindowOpened, QueryChanged, ThemeChanged.
*   `System Events`: ApplicationStarted, NetworkOnline, ClipboardChanged.

#### 1.4 Capability Model
*   **Core:** Filesystem, Network, Clipboard, Notifications, Shell, SQLite.
*   **Advanced:** OAuth, Window, Search, AI, MCP.

#### 1.5 Observability Layer (Telemetry)
Track: Startup Time, Render Time, IPC Latency, CPU/RAM Quotas, Panic Count, Capability Audits.
