# PHASE 5 — CUSTOM REACT RENDERER

**Prepared By:** Principal Engineering Team

### 1. Reconciler Architecture
React Fiber -> `react-reconciler` (Host Config) -> Mutates In-Memory V8 Object Graph -> Serialization Pipeline -> MessagePack -> Tauri IPC -> Host React Renderer.

### 2. UI Node Model
Functions cannot be serialized. Event handlers are replaced with unique `CallbackID`s. A global `CallbackRegistry` maps IPC events back to the original closure.

### 3. Serialization & Batching
`resetAfterCommit()` pushes a microtask to serialize the root UI Node. V8 utilizes a Fast API MessagePack serializer to write directly to the Rust host.

### 4. Performance & Virtualization
DOM virtualization is handled entirely by the Host React Renderer (e.g., `@tanstack/react-virtual`).

### 5. Error Boundaries & Suspense
Fatal escapes terminate the isolate (crash overlay). Suspense natively supported (renders `<Loading />` nodes until Promises resolve).
