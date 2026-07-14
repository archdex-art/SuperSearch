# Usage Guide

## Summon the palette

Default hotkey is **⌥ Space** (`Alt+Space`, Tauri accelerator syntax). It's
user-configurable — settings persist to `settings.json` under the app data
directory and survive restarts:

| Setting | Default | Meaning |
|---|---|---|
| `toggle_shortcut` | `Alt+Space` | Global summon/dismiss hotkey (e.g. `CommandOrControl+Space`) |
| `hide_on_blur` | `true` | Dismiss automatically when the palette loses focus (Spotlight-style) |
| `theme` | `dark` | UI theme identifier |

Press the hotkey again, hit **Esc**, or click away to dismiss. SuperSearch
runs as a menu-bar/accessory app (no Dock icon) and floats over full-screen
apps.

## 1. Unified search

Just start typing. A single fuzzy-ranked list merges:
- Installed applications
- Files (Spotlight `mdfind` on macOS; the platform-appropriate indexer elsewhere)
- Built-in system toggle commands (sleep, lock, dark mode, …)
- Results from any enabled extensions (see [extensions.md](extensions.md)) —
  merged into the same ranked list, not a separate panel

## 2. Natural-language agent

Full conversational commands route through the intent classifier
(`agent::patterns`, zero-latency, fully local — no LLM, no network call) and
are compiled into a `TaskGraph` before anything touches the OS:

- `"open chrome in incognito"`
- `"launch spotify and search imagine dragons"`
- `"search the web for how to center a div"`

Multi-step phrasing (`"open X and do Y"`) compiles to a `MultiStep` intent —
each step is authorized and journaled independently.

## 3. Direct app injection (`/appname`)

Prefix with `/` and an app name to inject keystrokes directly into that app,
waking it if it's closed:

- `/chatgpt draft an email to my boss`
- `/brave open github.com`
- `/notes …`

This requires **Accessibility** permission (macOS) — SuperSearch prompts on
first use of an app command. Without it, the app opens but nothing types.

## 4. Terminal execution (`$` / `/terminal`)

- `$ htop`
- `/terminal python3 server.py`

Pipes the command directly into a live terminal session via argv — never
through a shell string, so shell metacharacters in the rest of your query
can't leak into the command.

## Every query goes through `agent_check` first

The frontend calls `agent_check(query)` to decide whether a query is a plain
search or should route to `agent_query(query)` (the classification →
planning → execution pipeline). Queries are capped at 2048 bytes
(`MAX_QUERY_LEN`) and validated before entering the pipeline — empty or
oversized input is rejected at the IPC boundary, not deep in the runtime.

## Permissions you'll be asked for (macOS)

| Permission | Why | Required for |
|---|---|---|
| Accessibility | Synthesize keystrokes into other apps | `/app …` commands, multi-step agent intents |
| Input Monitoring | Register the global hotkey | Summoning the palette at all |

If Option+Space doesn't summon the window, or app commands silently do
nothing, check **System Settings → Privacy & Security** for both.
