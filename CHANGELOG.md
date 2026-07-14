# Changelog

All notable changes to SuperSearch are documented here. Format loosely
follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); versions
correspond to [GitHub Releases](https://github.com/archdex-art/SuperSearch/releases)
and their published installers.

## [0.1.4] — 2026-07-14

UI responsiveness, full-screen overlay, and branding.

### Fixed
- **Typing jitter / freezing (responsiveness).** `search_query` was a plain
  (non-`async`) Tauri command, so its body — including the `mdfind` Spotlight
  subprocess and extension fan-out — ran synchronously on the thread that
  delivers the IPC message (the WKWebView main thread on macOS). Firing on
  every debounced keystroke, it froze the window (no typing, no animation)
  for the duration of each search. It now runs on a blocking thread via
  `spawn_blocking`, so the UI stays responsive while a search is in flight.
- **Action execution froze the window.** `execute_action` (and
  `execute_extension_action`) had the same flaw: selecting a result spawned
  `open`/`osascript`/an extension subprocess synchronously on the main
  thread, freezing the window for up to the 15s action timeout. Both are now
  offloaded to a blocking thread.
- **Palette didn't float over full-screen apps.** The window carried the
  correct `FullScreenAuxiliary` collection behavior but only
  `NSFloatingWindowLevel` (via `set_always_on_top`), which isn't high enough
  to composite above a full-screen app — so it stayed hidden behind it. It's
  now raised to `NSStatusWindowLevel` (the menu-bar level) and the overlay
  settings are re-asserted on every summon, so it appears over full-screen
  apps like Spotlight.

### Changed
- **App icon** now matches the website brand mark: a dark rounded square with
  the blue→cyan→violet gradient search glass (regenerated for macOS, Windows,
  Linux, iOS, and Android from the site's `favicon.svg`).

## [0.1.3] — 2026-07-14

Engineering audit pass: a scheduler CPU-burn bug, a quadratic disk-I/O bug,
two timeout-bypassing subprocess-I/O bugs, and several correctness bugs in
the natural-language agent and the palette front-end.

### Fixed
- **Scheduler busy-wait (CPU):** the tick loop re-polled the same still-pending
  task up to `max_tasks_per_tick` times per tick and then looped again with no
  sleep, so any in-flight `agent_query` pinned a CPU core for its entire
  duration. Pending tasks are now deferred to the next tick and the executor
  parks between ticks unless something actually completed.
- **Scheduler fairness:** starvation-promoted tasks were served LIFO (most
  recently promoted first); switched the promotion staging queue to FIFO so
  the longest-starved task runs first, as intended.
- **Journal disk I/O:** the writer rewrote the entire (up to 64 MiB) segment
  buffer from byte 0 on every flush — O(n²) total disk I/O to fill one
  segment. Flushes now append only the unwritten delta.
- **Subprocess timeout bypass:** OS actions and script extensions only read
  stdout/stderr after the child exited, so any output larger than the OS pipe
  buffer (~64 KiB) deadlocked the child and always burned the full timeout
  before the result was discarded. Clipboard writes had the same problem via
  a blocking stdin write issued before the timeout loop even started. Pipes
  are now drained (and stdin fed) on background threads concurrently with
  the deadline-bounded poll loop.
- **Multi-step agent commands:** a compound query like `"open /Users/Bob/
  Report.PDF and switch to Chrome"` lowercased each sub-intent's original-case
  text before extraction, corrupting case-sensitive file paths.
- **Browser-search folding:** queries folded into a browser launch
  (`"open chrome and search for …"`) were URL-encoded with a bare
  space→`+` replace, so `&`, `%`, `#` reached the query unescaped and could
  truncate or corrupt the search.
- **Windows `open_url`:** routed through `cmd /c start`, which re-parses its
  whole operand and honors `&`/`|` even in an "already quoted" argument — an
  ordinary query string like `?a=1&b=2` could execute a second command.
  Switched to `explorer <url>` (no shell involved).
- **Front-end search race:** a slower, stale debounced search response could
  land after a faster newer one and clobber the visible results.
- **Front-end listener leak:** unmounting before the `supersearch://reset`
  listener finished registering skipped `unlisten()`, leaking the Tauri event
  listener.
- Capability tokens were derived from a hardcoded compile-time key instead of
  a per-boot random one; switched to an OS-CSPRNG-seeded key.
- Multi-step task-graph flattening silently dropped a sub-plan's own internal
  dependency edges (currently latent — no planner branch emits multi-node
  sub-plans yet, but preserved for the next one that does).

### Also merged from the prior unreleased branch
- Unified the React command palette onto a single search source, merging
  extension results into the same ranked list instead of a separate panel
  (`query_extensions`).
- Extension query hardening: concurrent fan-out across enabled extensions,
  plus a script trust gate before an extension's output can produce actions.
- Scheduler fix: in-flight tasks are no longer cancelled as overdue
  (retention fix in `scheduler/`).
- CI: added a front-end typecheck + build job (`react-command-palette`) so
  TypeScript/Vite regressions are caught in the same pipeline as the Rust
  build.

## [0.1.1] — 2026-06-09

Patch release fixing macOS actions that didn't work in the installed app.

### Fixed
- **Lock screen** now works without any permission (uses `pmset displaysleepnow`
  instead of a mechanism that needed Automation access).
- **App commands** (`/chatgpt …`, `/notes …`, etc.) — the app now requests
  **Accessibility** permission (previously requested Automation, the wrong
  permission), so synthesized keystrokes actually reach the target app
  instead of being silently dropped in the installed build.
- **Screenshot** now saves to the Desktop correctly — `~` is resolved instead
  of used literally.
- Widened the app-command focus delay so slow-launching apps finish focusing
  before SuperSearch starts typing.

### Known issues
- Installers are still unsigned — macOS Gatekeeper and Windows SmartScreen
  warn on first launch.

## [0.1.0] — 2026-06-07

First cross-platform release — macOS, Linux, and Windows.

### Added
- Cross-platform OS automation behind a single Platform Abstraction Layer
  (macOS / Linux / Windows backends).
- Capability-gated execution: every OS action authorized against a revocable
  token and journaled.
- Command injection eliminated on the live execution path — argv-only
  process spawns, no `sh -c` for user-derived data.
- Installers: `SuperSearch_0.1.0_universal.dmg` (macOS), `.deb` (Linux),
  NSIS `.exe` + `.msi` (Windows).

### Known issues
- Unsigned builds — Gatekeeper / SmartScreen warnings on first launch.
- Auto-update not enabled in this build.

---

[Unreleased]: https://github.com/archdex-art/SuperSearch/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.4
[0.1.3]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.3
[0.1.1]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.1
[0.1.0]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.0
