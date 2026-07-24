# PHASE 6 — NATIVE UI SYSTEM

**Prepared By:** Principal Engineering Team
**Objective:** Architect the production-grade UI component primitives that extensions can use via `@supersearch/api`. These components are abstractly defined in the guest isolate and natively rendered by the host application to guarantee performance, accessibility, and visual consistency.

---

### 1. Component Architecture & Principles

The Native UI System operates on a strict **Declarative Primitive Model**. Extensions cannot inject arbitrary CSS, HTML, or SVG. They must compose their interfaces using the provided generic primitives. 

*   **Host-Driven Rendering:** The Reconciler serializes the component tree into MessagePack. The React 18 Host Engine deserializes it and maps the abstract nodes to actual Tailwind-styled components.
*   **Accessibility by Default:** Every primitive automatically handles ARIA labels, focus trapping, and screen-reader announcements.
*   **Theme Continuity:** Colors are restricted to semantic tokens (e.g., `Color.Primary`, `Color.Danger`), ensuring perfect light/dark mode transitions.

---

### 2. Core UI Primitives

#### 2.1 `<List>` and `<List.Item>`
The workhorse component for displaying searchable, actionable data.

*   **API / Props:**
    ```typescript
    interface ListProps {
      isLoading?: boolean;
      searchText?: string;
      onSearchTextChange?: (text: string) => void;
      onSelectionChange?: (id: string) => void;
      navigationTitle?: string;
      searchBarPlaceholder?: string;
      children: ReactNode; // <List.Item> elements
    }
    
    interface ListItemProps {
      id?: string;
      title: string;
      subtitle?: string;
      icon?: Image.ImageLike;
      accessories?: Accessory[];
      actions?: ReactNode; // <ActionPanel>
    }
    ```
*   **IPC Representation (MessagePack mapping):**
    ```json
    { "type": "List", "props": { "isLoading": false, "onSearchTextChange": "cb_001" }, "children": [ ... ] }
    ```
*   **Performance:** The Host Engine automatically wraps the `children` array in `@tanstack/react-virtual`. Even if an extension serializes 5,000 items, only the visible ~15 are rendered to the DOM.
*   **Accessibility:** Implements `role="listbox"` and `aria-activedescendant` for keyboard navigation.

#### 2.2 `<Form>` and Input Primitives
Used for capturing user configuration or structured data.

*   **API / Props:**
    ```typescript
    interface FormProps {
      actions?: ReactNode; // <ActionPanel>
      children: ReactNode; // Form components
    }
    // Sub-components: <Form.TextField>, <Form.PasswordField>, <Form.Dropdown>, <Form.DatePicker>
    ```
*   **Lifecycle:** Form state is primarily controlled. When a user types in a `<Form.TextField>`, the Host dispatches the `onChange` event via IPC, the Guest updates its state, and pushes the new tree.
*   **Optimization:** To prevent 60FPS typing lag over IPC, TextFields support an *uncontrolled* mode via `defaultValue` and a `Form.useForm()` hook to capture all values on submit.

#### 2.3 `<Detail>`
Renders rich text, Markdown, and split-pane views for deep-dives into data (e.g., viewing a Jira ticket or GitHub issue).

*   **API / Props:**
    ```typescript
    interface DetailProps {
      markdown: string;
      metadata?: ReactNode; // <Detail.Metadata>
      actions?: ReactNode; // <ActionPanel>
    }
    ```
*   **Performance:** Markdown parsing (e.g., `marked` or `react-markdown`) is strictly performed by the **Host Engine**, NOT the extension guest. The extension merely passes the raw markdown string over IPC. This saves massive CPU cycles in the V8 isolate.

#### 2.4 `<ActionPanel>` and `<Action>`
The central hub for executing commands. Accessible via keyboard shortcuts (Cmd+K) or context menus.

*   **API / Props:**
    ```typescript
    interface ActionPanelProps {
      title?: string;
      children: ReactNode; // <Action> elements
    }
    
    // Core Actions:
    // <Action.SubmitForm onSubmit={...} />
    // <Action.OpenInBrowser url="..." />
    // <Action.CopyToClipboard content="..." />
    // <Action.Push target={<NextComponent />} />
    ```
*   **IPC Representation:** Actions that perform OS-level tasks (like `OpenInBrowser`) bypass the V8 Isolate's custom JS logic. The Host executes the OS command directly upon receiving the intent, ensuring zero latency.

---

### 3. View Navigation & Routing

SuperSearch uses a stack-based navigation model.
*   **API:** `useNavigation()` hook provides `push()`, `pop()`, and `replace()`.
*   **Lifecycle:** Pushing a new view unmounts the previous view from the DOM but keeps it alive in the V8 React Reconciler memory tree (Suspended). Popping the view immediately restores the state without re-rendering.

---

### 4. Shared Resources: Icons & Images

*   **API:** All image props accept a semantic `Icon` enum (e.g., `Icon.MagnifyingGlass`), a local file path, or a remote URL.
*   **Security:** Remote image URLs are intercepted by the Host Engine. The Host fetches and caches the image. The V8 Isolate never makes direct HTTP requests for images, enforcing the Capability Gate.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

### APPROVAL GATE

Phase 6 (Native UI System) is complete, establishing the strictly declarative, accessible, and highly optimized component primitives that bridge the Extension SDK with the Host Renderer. 

The preceding phases (0 through 5) have been successfully logged to the file system at `~/Desktop/SuperSearch/docs/architecture/`.

Shall the Principal Engineering Team proceed to **PHASE 7 — SDK**, where we will define the comprehensive `@supersearch/api` modules, including HTTP, Storage, Filesystem, AI Tool calling, and their associated TypeScript types?