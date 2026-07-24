# PRE-PHASE 6 — UI COMPONENT CONTRACT

*The architectural API boundary between the Developer SDK (Guest), the Reconciler (Bridge), and the Native Renderer (Host).*

Every component added to the SuperSearch platform must satisfy this strict 7-layer contract.

### 1. Component
*   **Definition:** The semantic identity of the primitive (e.g., `List.Item`, `Form.TextField`).
*   **Constraint:** Must map 1:1 to a native widget or a composite Tailwind construct on the Host.

### 2. Props
*   **Definition:** The immutable data passed from the SDK.
*   **Constraint:** Types must be strictly serializable (Strings, Numbers, Booleans, Enums). Complex objects must conform to predefined interfaces (e.g., `ImageLike`).

### 3. Events
*   **Definition:** Interactive triggers (e.g., `onChange`, `onAction`).
*   **Constraint:** Handled exclusively via `CallbackID` strings in the Reconciler layer.

### 4. Validation
*   **Guest-Side:** TypeScript compiler enforces prop shapes in the SDK.
*   **Host-Side:** The Rust deserializer strictly validates incoming MessagePack payloads against a JSON Schema. Malformed props result in a safe fallback (e.g., rendering an error placeholder) rather than a host panic.

### 5. Accessibility (A11y)
*   **Constraint:** The Host is exclusively responsible for ARIA mapping. 
*   **Example:** `<List.Item>` on the Guest has no ARIA props. The Host Renderer automatically maps it to `role="listitem"` and manages `aria-selected` based on the native focus engine.

### 6. Serialization Schema
*   **Constraint:** The minimum necessary byte representation.
*   **Example:** `{ "t": "List.Item", "p": { "title": "Repo" }, "e": { "onSelect": "cb_1" } }`. Keys are minified (`t` for type, `p` for props, `e` for events) to compress MessagePack size.

### 7. Native Rendering Rules
*   **Constraint:** The Host Renderer treats the Guest tree as a "suggestion." The Host retains ultimate authority over layout, spacing, typography scales, and z-index to enforce global UI consistency.
