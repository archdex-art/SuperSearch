# Changelog

All notable changes to SuperSearch are documented here. Format loosely
follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); versions
correspond to [GitHub Releases](https://github.com/archdex-art/SuperSearch/releases)
and their published installers.

## [0.1.7] — 2026-07-16

Visual identity overhaul, and a genuinely animated summon/dismiss instead of
an instant snap.

### Changed
- **New "Instrument" identity, replacing the violet/amber aurora glass.**
  Self-hosted Instrument Sans (titles/body) + Martian Mono (wordmark,
  section labels, kbd hints, paths) via `public/fonts/` — no runtime fetch
  from Google Fonts, consistent with the "local first" security posture.
  The rotating conic-gradient rim is gone; the panel is now a flat amber
  hairline frame with four viewfinder-style corner brackets, a faint
  schematic grid, and film-grain texture instead of a soft color wash.
- **Category colors remapped** to a restrained functional palette (Agent =
  amber, Command = cyan, Application = blue, Extension = rose, System =
  emerald, File = slate) in `categories.ts`, shared by the result list, the
  detail pane, and the type filter.
- **Smooth, Spotlight-style summon/dismiss.** Every dismissal path — Escape,
  selecting a result, the hotkey toggle while open, and blur-hide — now
  plays the panel's exit animation (~130ms fade + scale-down) *before* the
  native window actually hides, instead of vanishing instantly. Rust no
  longer calls `window.hide()` directly except from the `hide_window` IPC
  command itself, which the frontend invokes only after its exit transition
  completes (`App.tsx`'s `onAnimationComplete` + a new
  `supersearch://request-close` event for the two Rust-initiated paths).

## [0.1.6] — 2026-07-15

Follow-up polish on the 0.1.5 redesign.

### Changed
- **Static rim, not spinning.** The panel's conic-gradient border no longer
  rotates — it's a fixed diagonal sweep (violet top-left → amber
  bottom-right, echoing the ambient wash), plus a matching static violet
  glow under the panel shadow. Calmer for a tool opened dozens of times a
  day.

## [0.1.5] — 2026-07-15

Command palette visual redesign: master-detail layout, category color coding,
and a distinct "aurora" identity in place of the earlier generic dark-glass
look.

### Changed
- **Master-detail results.** The palette now splits into a narrow result list
  and a detail pane for the highlighted row (icon, title, category, action,
  path), instead of a single flat list — mirrors a launcher/detail-view shape
  without copying any specific product's styling.
- **Category color coding.** Each result's icon chip, active-row accent bar,
  and section-header dot now carry a category-specific hue (Agent = violet,
  Command = sky, Application = amber, Extension = fuchsia, System = teal,
  Files = slate) via a new shared `categories.ts` module, so the source of a
  result reads at a glance.
- **Type filter.** A chip in the search bar lets you narrow the current
  results to one category; only shown when more than one category is present.
- **New visual identity.** Replaced the flat dark-glass panel with a slowly
  rotating "aurora" gradient rim (`aurora-frame` in `styles.css`), an ambient
  violet/amber color wash on the glass, and a context-aware footer that shows
  the active result's action ("Open Figma") instead of static branding.
- Result-count badge in the search bar; violet-tinted caret, scrollbar, and
  keyboard-shortcut chips throughout, replacing the previous emerald accent.

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

[Unreleased]: https://github.com/archdex-art/SuperSearch/compare/v0.1.7...HEAD
[0.1.7]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.7
[0.1.6]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.6
[0.1.5]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.5
[0.1.4]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.4
[0.1.3]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.3
[0.1.1]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.1
[0.1.0]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.0
