# PHASE 13 — AI EXTENSIONS (MCP NATIVE)

**Prepared By:** Principal Engineering Team
**Objective:** Architect the integration between the Extension Ecosystem and the SuperSearch AI Agent. Unlike traditional platforms where AI is a bolted-on feature, SuperSearch treats the AI (`AgentController`) as a primary user, natively equipping it with every installed extension via the Model Context Protocol (MCP).

---

### 1. Architectural Philosophy

**Extensions are the hands and eyes of the AI.**
Rather than building hardcoded integrations (e.g., a specific Jira plugin for the AI), the `AgentController` dynamically queries the `ExtensionRegistry`. Any extension installed by the user instantly grants the AI new capabilities, limited strictly by the extension's capability manifest.

---

### 2. The Model Context Protocol (MCP) Pipeline

Anthropic's MCP provides a standardized JSON-RPC interface for AI tool calling. The Rust Host acts as an automatic MCP translation layer between the LLM and the V8 Isolate.

#### 2.1 Manifest to Tool Schema
When an extension declares a `mode: "no-view"` command, the Host parses its arguments and automatically compiles them into an OpenAI/Anthropic compatible Tool Schema.

*Manifest:*
```json
{
  "name": "search-linear",
  "mode": "no-view",
  "arguments": [{ "name": "query", "type": "string", "description": "The bug or issue to find" }]
}
```
*Compiled MCP Tool:*
```json
{
  "type": "function",
  "function": {
    "name": "ext_linear_search-linear",
    "description": "Searches the user's Linear workspace for issues.",
    "parameters": { "type": "object", "properties": { "query": { "type": "string" } } }
  }
}
```

#### 2.2 Execution Flow
1.  **Prompt:** User types "Find the bug about the login screen."
2.  **LLM Decision:** The model invokes the `ext_linear_search-linear` tool with `{"query": "login screen"}`.
3.  **Host Validation:** Rust intercepts the tool call, verifies the payload against the manifest schema, and spawns the specific Extension's V8 Isolate.
4.  **Guest Execution:** The isolate executes the command and yields the JSON result (e.g., `[{ id: "LIN-123", title: "Login crash" }]`).
5.  **Synthesis:** The Rust Host feeds the result back to the LLM, which generates a natural language summary or a Native UI response.

---

### 3. Context Management & Providers

The AI needs to understand what the user is currently looking at. Extensions can act as **Context Providers**.

*   **API:** Extensions can register listeners via `@supersearch/api/ai`.
*   **Usage:** A Chrome extension might broadcast the current active URL to the Host. An IDE extension might broadcast the active file path.
*   **Agent Memory:** The `AgentController` maintains a rolling context window of these broadcasted states, ensuring the LLM is always contextually aware (e.g., the user types "Summarize this," and the agent already knows "this" refers to the active Chrome tab).

---

### 4. Local vs. Cloud Model Routing

The platform supports a hybrid inference architecture to balance privacy, latency, and reasoning capability.

#### 4.1 Local Inference (The Router)
*   **Engine:** Embedded `llama.cpp` or ONNX runtime within the Rust host.
*   **Model:** A small, highly quantized model (e.g., Llama 3 8B or Phi-3).
*   **Purpose:** Instant intent classification, local context summarization, and basic tool routing. Operates offline, completely preserving privacy.

#### 4.2 Cloud Inference (The Reasoner)
*   **Engine:** API connection to advanced models (Claude 3.5 Sonnet, GPT-4o).
*   **Purpose:** Complex orchestration, multi-step agentic workflows, and heavy code generation.
*   **Trigger:** The Local Router decides to escalate to the Cloud Reasoner when the user's prompt exceeds local capabilities.

---

### 5. Security & Idempotency

Granting an autonomous AI access to extensions poses unique security risks.

*   **Idempotency Flags:** By default, all MCP tools are treated as non-idempotent (mutative). If the AI attempts to use a tool (e.g., `delete-file`), the Host pauses execution and renders a Native UI prompt: *"The AI wants to delete X. Allow?"*
*   **Safe Execution:** Extension authors can explicitly mark commands as `"idempotent": true` (e.g., `search-linear`). The Host will allow the AI to execute these tools autonomously without prompting the user.

---

### 6. AI UI Generation (Agentic UI)

When the AI calls an extension, the extension can return raw data (JSON), OR it can return an abstract Native UI Tree (MessagePack).

*   If an extension returns a `<List>`, the AgentController intercepts it and renders the interactive UI component inline within the AI Chat interface, rather than returning raw text. This bridges conversational AI with highly interactive graphical interfaces.
