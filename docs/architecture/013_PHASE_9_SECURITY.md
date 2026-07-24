# PHASE 9 — SECURITY ARCHITECTURE

**Prepared By:** Principal Engineering Team
**Objective:** Define the enterprise-grade security model for the Extension Platform. This encompasses the threat model, capability enforcement, code signing, runtime hardening, and secure secret management.

---

### 1. Threat Model

The platform is designed to defend against the following primary threat vectors:
*   **Malicious Extensions:** Code intentionally designed to exfiltrate user data, mine cryptocurrency, or execute arbitrary OS commands.
*   **Supply Chain Attacks:** A legitimate extension whose npm dependencies are compromised (e.g., a malicious update to a popular utility library).
*   **Privilege Escalation:** An extension attempting to break out of the V8 isolate to access the Rust host memory or ungranted OS APIs.

*(Note: The Host application itself is considered the Trusted Computing Base).*

---

### 2. Capability Enforcement (Zero Trust)

Extensions operate in a strict default-deny environment. 

#### 2.1 Manifest Declarations
Capabilities must be explicitly declared in the extension's `package.json`. Broad capabilities are rejected by the marketplace linter.
*   *Bad:* `{"capabilities": ["fs"]}`
*   *Good:* `{"capabilities": ["fs.read:/tmp/supersearch", "net.fetch:api.github.com"]}`

#### 2.2 Host-Side Enforcement (Rust)
The SDK (`@supersearch/api`) does *not* enforce security; it is easily bypassed by malicious JS. Security is enforced exclusively in the Rust Host.
1.  When an extension is installed, the user is presented with a **Permission Prompt** detailing the requested capabilities.
2.  If granted, these are stored in the Host's SQLite database.
3.  During **Sandbox Allocation**, the Rust runtime injects *only* the authorized FFI bindings (ops) into the V8 isolate. If `net.fetch` is denied, `op_fetch` simply does not exist in that isolate's memory space.

---

### 3. Runtime Hardening (V8 Isolate)

To prevent dynamic code execution and sandbox escapes:
*   **No `eval()` or `new Function()`:** Content Security Policy (CSP) equivalents are enforced within the isolate. Dynamic JavaScript compilation is disabled. All code must be AOT (Ahead-of-Time) parsed during isolate boot.
*   **Frozen Prototypes:** Global objects (`Array.prototype`, `Object.prototype`) are deeply frozen to prevent prototype pollution attacks between the SDK and the extension's business logic.
*   **No `std` Library:** The isolate does not have access to Node.js `fs`, `path`, or `child_process`. It only has access to the proprietary `@supersearch/api` bindings.

---

### 4. Secure Secret Management

Extensions routinely require highly privileged tokens (e.g., GitHub PATs, AWS Keys).
*   **OS Keychain Integration:** The Rust Host utilizes native credential managers (macOS Keychain, Windows Credential Guard, Linux Secret Service).
*   **Extension Isolation:** Secrets are namespaced by Extension ID. Extension A cannot read Extension B's secrets.
*   **Opaque Network Injection (Optional Architecture):** For ultra-high security extensions, the SDK can pass an opaque handle (e.g., `secret:github_pat`) to the `fetch` API. The Rust Host intercepts the network request, looks up the real token in the keychain, injects the `Authorization: Bearer <TOKEN>` header, and executes the request. The raw token never enters the V8 isolate memory.

---

### 5. Code Signing & Marketplace Security

To prevent execution of tampered code, a strict Chain of Trust is established.

#### 5.1 Ed25519 Cryptographic Signatures
1.  **Marketplace CI/CD:** When an extension is published, the Marketplace runs automated security scanning (SAST) and dependency auditing.
2.  **Signing:** If approved, the Marketplace compiles the extension into a single minified bundle and signs it using the Marketplace's private Ed25519 key.
3.  **Host Verification:** When SuperSearch downloads or launches an extension, the Rust Host verifies the bundle's signature against the embedded Marketplace public key.

#### 5.2 Local Development Bypass
To allow developers to build extensions locally, SuperSearch supports a `Developer Mode`. In this mode, unsigned extensions can run, but they are flagged with a prominent visual "Unverified Developer" warning and cannot be distributed.

---

### 6. Resource Exhaustion (DoS Prevention)

A malicious or poorly written extension can attempt a Denial of Service (DoS) attack against the Host application.
*   **CPU Quotas:** Monitored via a background Tokio watchdog. If the V8 event loop blocks for >200ms, the isolate is terminated.
*   **Memory Quotas:** V8 heap is strictly capped (e.g., 50MB for Quick Actions). Exceeding this triggers an immediate `OOM` termination by the Rust runtime.
*   **IPC Throttling:** If an extension attempts to spam the Host with thousands of IPC requests per second, the bounded channel exerts backpressure, and the watchdog will terminate the isolate for abusive behavior.
