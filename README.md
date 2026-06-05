# SuperSearch 🤖⚡

**The AI-Native Productivity Operating Layer for macOS**

SuperSearch is not just another launcher or command palette. It is a fundamental evolution of how you interact with your operating system. Traditional launchers (like Raycast or Alfred) act as search bars with extensions. SuperSearch acts as an **Intent-Driven Runtime Kernel** that understands natural language, synthesizes multi-step execution graphs, and orchestrates actions autonomously across your local system environment.

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

* **`ui/` (Frontend):** A blazingly fast vanilla HTML/CSS/JS interface designed with rich, glassmorphic aesthetics. It communicates with the Rust backend entirely via Tauri IPC channels.
* **`src-tauri/` (App Host):** The Tauri daemon. Handles global hotkeys, window management, fuzzy searching (`commands/search.rs`), and dispatches OS-level AppleScript executions (`commands/actions.rs`).
* **`crates/supersearch-runtime/` (AI Kernel):** The autonomous brain of SuperSearch.
  * `patterns.rs` — Natural Language Classifier mapped to deterministic Intents.
  * `planner.rs` — Compiles Intents into DAG-based `TaskGraph` structures.
  * `executor.rs` — Executes the task graphs, manipulating macOS via shell and `osascript`.
  * `memory.rs` & `context.rs` — Short-term execution journals and spatial awareness.

---

## 🚀 Setup & Installation

SuperSearch is currently built exclusively for **macOS** (due to heavy reliance on `osascript` and Apple Events for zero-friction OS automation).

### Prerequisites
1. **Rust & Cargo:** (Install via [rustup](https://rustup.rs/))
2. **Tauri CLI:** Installed via Cargo or your package manager.
3. **macOS:** Tested on macOS Ventura & Sonoma.

### Running the App

1. Clone the repository and navigate into the project root.
2. Initialize the development server:
   ```bash
   cd src-tauri
   cargo tauri dev
   ```
3. **CRITICAL — Accessibility Permissions:** SuperSearch drives your Mac via simulated keystrokes and Apple Events. When you execute your first action (e.g. `/chatgpt hello`), macOS will prompt you for Accessibility access.
   * Open **System Settings > Privacy & Security > Accessibility**.
   * Toggle the switch to **ON** for your Terminal (or IDE) and for `SuperSearch`.
   * *If you do not grant this permission, app commands and multi-step agent intents will silently fail.*

### Building for Production
To package a release `.app` bundle:
```bash
cd src-tauri
cargo tauri build
```
The compiled binary will be located at `src-tauri/target/release/bundle/macos/SuperSearch.app`.

---

## ⌨️ Usage Guide

### 1. Unified Search
Trigger the palette and just start typing. It fuzzy-matches system settings (`Sleep`, `Empty Trash`), Applications (`Spotify`, `Xcode`), and local files.

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

## 🛡️ Security & Sandboxing

Because SuperSearch can autonomously drive your OS:
- **No Blind `eval()`:** We use a strict Capability-Gated Action model. The agent maps intents to predefined enums (`TaskNodeKind`) rather than generating raw, unsafe bash scripts on the fly.
- **Local First:** The intent classification engine is fully local, meaning your app launches and file lookups never leave your machine. 

## 🛠️ Contributing

When contributing to the runtime kernel, ensure that new `AgentIntent` additions are mapped properly through `TaskPlanner` to guarantee safe execution limits. Run `cargo build --workspace` to ensure both the Tauri app and the Runtime compile cleanly.
