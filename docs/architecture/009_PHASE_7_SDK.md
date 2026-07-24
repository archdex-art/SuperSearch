# PHASE 7 — DEVELOPER SDK (@supersearch/api)

**Prepared By:** Principal Engineering Team
**Objective:** Architect the public-facing TypeScript SDK. This is the primary interface for extension developers. It must be highly ergonomic, deeply typed, and strictly bound by the capability security model established in Phase 4.

---

### 1. Module Overview

The SDK is divided into distinct, tree-shakeable modules:
1.  **`@supersearch/api/ui`**: React primitives, Toast, Navigation, Actions.
2.  **`@supersearch/api/environment`**: App context, theme, active extension metadata.
3.  **`@supersearch/api/preferences`**: Strongly-typed access to user settings.
4.  **`@supersearch/api/storage`**: LocalStorage, SecretStore, SQLite access.
5.  **`@supersearch/api/system`**: Clipboard, FileSystem, Shell (Capability gated).
6.  **`@supersearch/api/network`**: `fetch` overrides (routed through Rust for CORS/Proxy handling).
7.  **`@supersearch/api/ai`**: Native integration with the `AgentController` (MCP Hooks).

---

### 2. Core API Specifications

#### 2.1 UI & Navigation (Declarative React)
The core of view-based extensions.
```typescript
import { List, ActionPanel, Action, useNavigation } from "@supersearch/api/ui";

export default function Command() {
  const { push } = useNavigation();

  return (
    <List>
      <List.Item
        title="Open Details"
        actions={
          <ActionPanel>
            <Action title="View" onAction={() => push(<DetailView />)} />
          </ActionPanel>
        }
      />
    </List>
  );
}
```

#### 2.2 Storage & Secrets (ACID Compliant)
Storage bypasses V8's ephemeral memory, persisting to the extension's isolated SQLite database.
```typescript
import { LocalStorage, SecretStore } from "@supersearch/api/storage";

// Standard KV caching
await LocalStorage.setItem("last_query", "react");
const query = await LocalStorage.getItem<string>("last_query");

// OS Keychain integration (Requires "secrets" capability)
const apiKey = await SecretStore.get("github_token");
```

#### 2.3 System Interaction (Capability Gated)
Access to the host OS. If the capability is missing in the manifest, these throw a `CapabilityError`.
```typescript
import { Clipboard, open } from "@supersearch/api/system";

// Write to OS Clipboard (Requires "clipboard-write" capability)
await Clipboard.copy("Text to copy");

// Open OS Default Browser (Requires "shell-open" capability)
await open("https://github.com");
```

#### 2.4 AI Integration (MCP Native)
Unlike other platforms where AI is bolted on, SuperSearch extensions are natively compiled into Model Context Protocol (MCP) tools. 

If an extension defines a `mode: "no-view"` command with arguments in its manifest, the Host automatically registers it with the `AgentController`. The SDK merely provides the execution context.

```json
// manifest.json
{
  "commands": [
    {
      "name": "create-issue",
      "title": "Create Linear Issue",
      "mode": "no-view",
      "arguments": [
        { "name": "title", "type": "string", "required": true }
      ]
    }
  ]
}
```

```typescript
// src/create-issue.ts
import { showToast, Toast } from "@supersearch/api/ui";

export default async function Command(props: { arguments: { title: string } }) {
  // This can be triggered by a user typing, OR autonomously by the AI Agent
  await createIssueInLinear(props.arguments.title);
  
  await showToast({ 
    style: Toast.Style.Success, 
    title: "Issue Created" 
  });
}
```

---

### 3. Error Handling & Best Practices

*   **No Blocking:** The SDK strictly forbids blocking the main thread. All `fs`, `network`, and `storage` calls return `Promise<T>`.
*   **Cancellable Requests:** Network requests must support standard `AbortController` to handle extensions suspending mid-fetch.
*   **Strict Typings:** The SDK utilizes TypeScript's `template literal types` and `mapped types` to provide deep autocomplete for icons (`Icon.MagnifyingGlass`), semantic colors, and preference keys based on the manifest.

### 4. SDK Versioning & Backward Compatibility

*   The SDK follows SemVer. 
*   **Host Compatibility:** The Rust Runtime checks the `package.json` SDK version. If an extension requires `@supersearch/api@2.0.0` but the Host provides `1.5.0`, the Sandbox Allocator refuses to boot the extension and prompts the user to update the SuperSearch application.
