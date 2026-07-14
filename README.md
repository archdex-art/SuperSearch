# SuperSearch 🤖⚡

**The AI-Native Productivity Operating Layer for macOS**

SuperSearch is not just another launcher or command palette. It is a fundamental evolution of how you interact with your operating system. Traditional launchers (like Raycast or Alfred) act as search bars with extensions. SuperSearch acts as an **Intent-Driven Runtime Kernel** that understands natural language, synthesizes multi-step execution graphs, and orchestrates actions autonomously across your local system environment.

[Documentation](docs/README.md) · [Roadmap](ROADMAP.md) · [Contributing](CONTRIBUTING.md) · [Changelog](CHANGELOG.md) · [Releases](https://github.com/archdex-art/SuperSearch/releases)

---

## 🌟 Core Philosophy & Features

1. **Deterministic Execution:** Every query is parsed into a replayable, inspectable task graph. 
2. **Intent-Driven Natural Language:** Type `"open brave in incognito and search what is regression"` — SuperSearch maps the intent, builds the execution sequence, and triggers OS-level AppleScripts to perform the actions autonomously.
3. **Application Control (`/appname`):** Type `/chatgpt summarize this file` or `/brave what is rust` to instantly inject keystrokes into specific applications, waking them up dynamically if they are closed.
4. **Instant Terminal Hooks:** Type `$ cargo build` or `/terminal pip3 install requests` to instantly pipe a command directly into a live Terminal session.
5. **Fuzzy Search Indexer:** Incredibly fast, unified fuzzy-search over your installed macOS Applications, Spotlight (`mdfind`) files, and built-in system toggle commands.
6. **Reactive Context Engine:** SuperSearch maintains short-term memory and context of the apps and files you interact with, allowing subsequent AI instructions to infer targets intelligently.

---

## 🏗️ Architecture

SuperSearch follows a modular architecture built for performance, security, and native integrations:

* **`react-command-palette/` (Frontend):** A React + TypeScript + Tailwind + Framer Motion command palette (Spotlight/Raycast-grade motion). Built with Vite; the bundled `dist/` is what Tauri loads as `frontendDist`. It communicates with the Rust backend entirely via Tauri IPC channels. See [`react-command-palette/README.md`](react-command-palette/README.md) for the motion architecture.
* **`src-tauri/` (App Host):** The Tauri daemon. Handles global hotkeys, window management, fuzzy searching (`commands/search.rs`), and dispatches OS-level AppleScript executions (`commands/actions.rs`).
* **`crates/supersearch-runtime/` (AI Kernel):** The autonomous brain of SuperSearch.
  * `patterns.rs` — Natural Language Classifier mapped to deterministic Intents.
  * `planner.rs` — Compiles Intents into DAG-based `TaskGraph` structures.
  * `executor.rs` — Executes the task graphs, driving macOS via argument-vector process spawns (`open`, `mdfind`, `osascript`) with a hard per-action timeout.
  * `memory.rs` & `context.rs` — Short-term execution journals and spatial awareness.

---

## 🚀 Setup & Installation

SuperSearch is currently built exclusively for **macOS** (due to heavy reliance on `osascript` and Apple Events for zero-friction OS automation).

### Install (end users)

SuperSearch ships as a native macOS app — **no Rust toolchain or terminal required**.

1. Download the latest `SuperSearch_x.y.z_aarch64.dmg` from the
   [**Releases** page](https://github.com/archdex-art/SuperSearch/releases).
2. Open the `.dmg` and drag **SuperSearch** into your **Applications** folder.
3. Launch it from Applications or Spotlight.

> **Unsigned builds (current state):** until code signing + notarization are
> configured (see [RELEASING.md](RELEASING.md)), macOS Gatekeeper will block the
> first launch. Right-click the app → **Open** → **Open** to allow it once.
>
> **No release yet?** If the Releases page is empty, there isn't a pre-built
> binary available — build from source using the steps below.

### Build from source (contributors)

#### Prerequisites
1. **Rust & Cargo:** (Install via [rustup](https://rustup.rs/))
2. **Tauri CLI:** Installed via Cargo or your package manager.
3. **macOS:** Tested on macOS Ventura & Sonoma.

#### Run the dev server
```bash
cd src-tauri
cargo tauri dev
```

#### Package a release bundle
```bash
cd src-tauri
cargo tauri build
```
The `.app` and `.dmg` land in `src-tauri/target/release/bundle/`. To cut a
published, (optionally) signed release, see [RELEASING.md](RELEASING.md).

### Accessibility permissions (all install methods)

**CRITICAL:** SuperSearch drives your Mac via simulated keystrokes and Apple
Events. When you execute your first action (e.g. `/chatgpt hello`), macOS will
prompt you for Accessibility access.
* Open **System Settings > Privacy & Security > Accessibility**.
* Toggle the switch to **ON** for `SuperSearch` (when running from source, also
  enable it for your Terminal/IDE).
* *If you do not grant this permission, app commands and multi-step agent intents will silently fail.*

---

## ⌨️ Usage Guide

### 1. Unified Search
Press **⌥ Space (Option+Space)** anywhere to summon the palette, then just start typing — it fuzzy-matches system settings (`Sleep`, `Empty Trash`), Applications (`Spotify`, `Xcode`), and local files. Press the hotkey again, hit **Esc**, or click away to dismiss it (Spotlight-style). SuperSearch runs as a menu-bar/accessory app with no Dock icon and floats over full-screen apps.

> First launch will register the global shortcut and request Accessibility/Input-Monitoring permission. If Option+Space doesn't summon the window, grant SuperSearch access under **System Settings → Privacy & Security**.

### 2. Natural Language Agent
Type full conversational commands to trigger the Runtime Kernel:
* `"open chrome in incognito"`
* `"launch spotify and search imagine dragons"`
* `"search the web for how to center a div"`

### 3. Direct App Injection
Use the `/` prefix followed by the app name to instantly command an application:
* `/chatgpt draft an email to my boss`
* `/brave open github.com`

### 4. Terminal Execution
Use the `$` or `/terminal` prefix to instantly open Terminal and run a bash process:
* `$ htop`
* `/terminal python3 server.py`

---

## 🧩 Extensions

SuperSearch is extensible. An extension is a directory with a `manifest.toml`
and an entrypoint, installed under
`~/Library/Application Support/com.supersearch.app/extensions/`.

Two execution models share one registry and (forthcoming) manager UI:
- **Script extensions (available now):** the entrypoint is a native script run
  as a subprocess — argv, no shell, hard 10s timeout.
- **WASM extensions:** sandboxed `.wasm` (or `.wat`) modules run via wasmtime
  with fuel + memory limits. ABI: export `memory`, `alloc(i32)->i32`, and
  `query(i32,i32)->i64` returning a packed pointer to a JSON result array. See
  [`examples/extensions/wasm-hello/`](examples/extensions/wasm-hello/).

**Capability-gated & consent-based.** Enabling an extension grants it a
*revocable* capability token scoped to `plugin.<id>`, covering exactly the
permissions its manifest requests (each with a justification shown at enable
time). Result-actions (`open_url`, `open_path`, `copy`) are checked against that
token by the same gate the agent uses — an action the extension didn't request
is denied before it touches the OS.

**Manifest** (`manifest.toml`):
```toml
id = "ddg"
name = "DuckDuckGo Search"
version = "1.0.0"
kind = "script"
entrypoint = "run.sh"
keywords = ["ddg", "search"]   # empty = consulted for every query

[[permissions]]
permission = "NetworkConnect"
justification = "Open search results in your default browser"
```

**Script contract:** invoked as `run.sh "<query>"`; print a JSON array of
`{ title, subtitle?, action? }` to stdout. A runnable example lives in
[`examples/extensions/ddg/`](examples/extensions/ddg/).

IPC surface (backing the manager UI): `list_extensions`, `install_extension`,
`uninstall_extension`, `set_extension_enabled`, `query_extensions`,
`execute_extension_action`.

## 🛡️ Security Model

Because SuperSearch can autonomously drive your OS, the threat model matters. Here is what is **actually enforced today**, not what is aspirational:

- **Capability-mediated execution (enforced).** Before any OS action runs, the executor maps it to a required `(Namespace, Permission)` and asks the `CapabilityGate` whether the agent's token authorizes it. A denied action never reaches the OS — no process is spawned. The agent holds a single, revocable token granted at boot in the `agent` namespace ([runtime.rs](crates/supersearch-runtime/src/kernel/runtime.rs)); revoking it (or narrowing its permission set) immediately disables the corresponding actions. This is the object-capability model described below, now on the live execution path — covered by `action_without_capability_is_blocked_before_touching_the_os`.
- **Auditable.** Every gate decision (`CapabilityCheck`) and OS result (`OsAutomationResult`) is appended to the append-only journal, giving a replayable audit trail of what the agent was asked to do and what it was allowed to do.
- **No shell string interpolation of user input.** Every action that carries user-derived data (app names, file paths, URLs, search queries, clipboard content, terminal commands) is executed by spawning the target binary directly with an *argument vector* — `open`, `mdfind`, `pbcopy`, `osascript` — never by building a string and handing it to `sh -c`. Shell metacharacters (`;`, `|`, `$()`, backticks, quotes) are therefore inert. Dynamic values passed to AppleScript are bound as `on run argv` items, not interpolated into the script source. See `crates/supersearch-runtime/src/agent/executor.rs` and `src-tauri/src/commands/actions.rs`. Covered by `clipboard_write_roundtrips_untrusted_content`.
- **Bounded execution.** Every OS action is killed if it exceeds a hard timeout (`ACTION_TIMEOUT`, 15s), so a hung helper process cannot wedge the app. IPC entry points reject empty and oversized input.
- **Fixed intent taxonomy.** The agent maps natural language to a closed set of `TaskNodeKind` variants; it never synthesizes arbitrary scripts from user text. The only `sh -c` calls remaining are *constant* scripts authored in `planner.rs` (e.g. `pmset sleepnow`) that never contain user input.
- **Local first.** Intent classification is fully local (rule-based, no LLM); your app launches and file lookups never leave your machine.

### ⚠️ Implementation status

The capability system (`capability/`) and the append-only journal (`journal/`) are now on the agent's execution path as described above. The cooperative scheduler (`scheduler/`), reactive graph (`reactive/`), and WASM plugin sandbox (`plugin/`) boot but are **not yet** load-bearing for first-party agent actions — they exist to host future third-party plugins, which will receive their own narrowly-scoped capability tokens through the same gate. Grant Accessibility permission only if you trust the build you are running.

## 🛠️ Contributing

When contributing to the runtime kernel, ensure that new `AgentIntent` additions are mapped properly through `TaskPlanner` to guarantee safe execution limits. Run `cargo build --workspace` to ensure both the Tauri app and the Runtime compile cleanly.

See [CONTRIBUTING.md](CONTRIBUTING.md) for dev setup, PR expectations, and
code conventions, and [docs/](docs/README.md) for the detailed architecture,
usage, extension, and security reference. Planned work lives in
[ROADMAP.md](ROADMAP.md); released versions in [CHANGELOG.md](CHANGELOG.md).
