# SuperSearch Extension Platform: Architecture Principles

These principles serve as the non-negotiable lens through which all future implementation decisions, PRs, and architectural shifts must be evaluated. Technologies may change, but these principles remain constant.

1. **Security by Default:** Extensions operate in a zero-trust environment. No filesystem, network, or OS access is granted implicitly. Every capability must be statically requested in the manifest and explicitly granted by the user or organizational policy.
2. **Capability-Based Access:** The runtime host acts as a strict capability gate. The guest sandbox cannot bypass these gates; it can only request execution through predefined IPC channels monitored by the host.
3. **Deterministic Lifecycle:** Extension states (`Load`, `Activate`, `Suspend`, `Unload`) must be predictable. Memory must be reliably freed upon `Unload`. An extension crashing must never crash the host application.
4. **Event-Driven Execution:** Extensions do not poll the host. The host pushes state changes (`QueryChanged`, `ThemeChanged`) to the extension, and the extension pushes UI updates back. Execution only occurs in response to an event.
5. **Native-First UX:** The platform rejects webviews and `iframes` for extension UI. All UIs are declared abstractly (via JSON/MessagePack) and rendered using the host's native components to guarantee accessibility, keyboard navigation, and thematic consistency.
6. **AI as a First-Class Consumer:** Every extension is a tool for the AI. Manifests and commands automatically map to the Model Context Protocol (MCP). The LLM is considered a primary user of the extension ecosystem.
7. **Backward-Compatible SDK Evolution:** The developer SDK (`@supersearch/api`) must maintain strict semantic versioning. Host runtime updates must never break older extension bundles. 
8. **Observable by Default:** The platform measures everything. Every extension's CPU time, memory footprint, startup latency, and panic count is tracked. Poorly performing extensions are penalized or suspended automatically.
9. **Extension Isolation:** No two extensions share the same memory heap. Extension A crashing or leaking memory has zero impact on Extension B.
10. **Minimal Idle Resource Usage:** Extensions that are not actively visible or processing background jobs must consume 0% CPU and minimal RAM, relying heavily on `Suspend` and `Unload` states.
