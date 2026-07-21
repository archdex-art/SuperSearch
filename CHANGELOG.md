# Changelog

All notable changes to SuperSearch are documented here. Format loosely
follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); versions
correspond to [GitHub Releases](https://github.com/archdex-art/SuperSearch/releases)
and their published installers.

## [0.1.19] — 2026-07-22

Fixes two "Execute in Terminal" / "Empty Trash" system commands that failed
every time they were invoked, with no indication of why.

Also fixes the global hotkey not reliably focusing the palette on Windows —
found and fixed while standing up and testing a Windows build for the first
time (Parallels/Windows 11 on Apple Silicon).

### Fixed
- **"Empty Trash" always reported failure when the Trash was already
  empty.** Live evidence: `Empty Trash: 29:40: execution error: Finder got
  an error: The operation can't be completed. (-128)`. This isn't a
  permissions or code bug — Finder's own `empty trash` AppleScript command
  raises error -128 whenever there's nothing to empty (verified: emptying a
  populated Trash exits 0; emptying an already-empty one always returns
  -128, regardless of automation permissions). Both the palette's direct
  `sys:empty_trash` action and the natural-language agent's `EmptyTrash`
  system command now check the item count first (`count items in trash`,
  and `if (count items in trash) > 0 then empty trash` respectively) so an
  empty Trash reads as "✓ Trash is already empty" instead of a scary error.
- **"Execute in Terminal" always failed to compile.** Live evidence:
  `Terminal Command: 43:49: syntax error: Expected end of line but found
  "script". (-2741)`. `open_terminal` built the AppleScript as one `-e` per
  line — `tell application "Terminal"` / `do script (item 1 of argv)` /
  `activate` / `end tell` — the textbook idiom found in most AppleScript
  tutorials. Root cause: `osascript` reliably fails to compile a `do
  script` statement that's split across `-e` boundaries *inside* a
  `tell ... end tell` block — even though the identical text compiles fine
  from a real `.applescript` file, and even though `tell application
  "Terminal" to get version` (any other command) works fine split the same
  way. Rewritten to two single-line `tell "Terminal" to do script (item 1
  of argv)` / `tell "Terminal" to activate` statements, which sidesteps the
  compiler quirk entirely. Verified: the exact new `osascript` invocation
  now exits 0 and runs the command in a new Terminal tab; the old one
  reliably reproduced the syntax error on every run.
- **The global hotkey summoned the palette invisibly-focused on Windows.**
  Reproduced by building and running a real Windows install (Parallels VM,
  Windows 11 on ARM64): summoning while another app (Notepad) held focus
  left the palette window shown but not actually keyed, mirroring a
  previously mac-only failure mode. Root cause: `show_palette`'s
  `activate_app()` step — which macOS needs because `set_focus()` alone
  doesn't activate the *process*, only the window — was a no-op on Windows.
  Windows has the same structural gap for a different reason:
  `SetForegroundWindow` (what `set_focus()` calls internally) is refused by
  the OS for any process that isn't already the foreground process, which a
  background global-hotkey summon always is. `activate_app` now does the
  standard `AttachThreadInput` + `SetForegroundWindow` dance on Windows.
  Verified live: before the fix, summoning while Notepad had focus left
  Notepad as the real Win32 foreground window; after the fix, the same
  summon makes SuperSearch the foreground window immediately and it stays
  there.

## [0.1.17] — 2026-07-17

Fixes the actual root cause behind the "hotkey doesn't work" reports: the
capture UI itself could persist a corrupted shortcut.

### Fixed
- **Rebinding the hotkey to an Option/Alt combo could silently corrupt it.**
  Live log evidence: `Global shortcut registration failed ... error=Found
  empty token while parsing hotkey: Alt+\u{a0}`. The General pane's capture
  UI built the accelerator from `KeyboardEvent.key` — the *composed*
  character — but macOS recomposes many keys under Option into a different
  Unicode character than what's printed on the keycap: Option+Space's `key`
  is a non-breaking space (U+00A0), not a plain `" "`, and Option+letters
  compose accented characters (Option+C → "ç"). The capture UI showed
  "Alt+Space" (the *display* used the physical key label) but silently
  persisted `"Alt+<NBSP>"` (built from the composed character) to
  `settings.json` — invisible until the next boot's registration attempt
  rejected it outright. The 0.1.15/0.1.16 self-heal recovered the hotkey
  back to the default afterward, but the underlying capture bug meant
  rebinding to *any* Option combo reproduced it every time.
  `toAccelerator` now resolves letters/digits/Space from
  `KeyboardEvent.code` (the physical key, unaffected by modifier
  composition) instead of `key`. Verified live: simulated the exact
  Option+Space event macOS actually sends (`code: "Space", key: "\u00A0"`)
  and confirmed it now captures as `Alt+Space`; same for Option+C
  (`key: "ç"`) capturing as `Alt+C`.

## [0.1.16] — 2026-07-17

Fixes the hotkey going completely silent instead of just "unreliable."

### Fixed
- **The hotkey could register zero times instead of falling back.** The
  0.1.15 self-heal only ever retried with the *default* shortcut, and only
  when the configured shortcut already differed from it — so a transient
  registration failure while the configured shortcut *was already* the
  default (the common case) left the app with no working hotkey at all
  until the next restart, no retry, no fallback. This is exactly what
  happens when an old build is still squatting on the same combo: confirmed
  live via `ps aux` finding a pre-0.1.15 `/Applications/SuperSearch.app`
  process still running (started well before the fix was pulled) still
  holding `Alt+Space`, which the newly-started process's own registration
  attempt then silently lost to. Registration now retries the *same*
  shortcut up to 3 times with a 200ms backoff before giving up on it —
  covers the OS taking a moment to release a hotkey a just-exited duplicate
  process (an old build, or a `single_instance`/dev-restart handoff) was
  still holding.

## [0.1.15] — 2026-07-17

Three fixes for "the hotkey doesn't reliably summon the palette," found while
chasing a settings-persistence report back to its actual root cause.

### Fixed
- **Two competing processes could run at once, each with its own cached
  settings.** A second launch (a stray `/Applications` install left running
  alongside a `cargo tauri dev` session, or an overlapping dev restart)
  started a fully independent process — its own `SettingsStore` loaded once
  from disk at its own boot time, its own attempt at registering the same
  global hotkey. Two processes racing for one hotkey meant summoning "the
  palette" could nondeterministically show either one's window, and each had
  its own stale view of settings (the `rev` guard added in 0.1.14 only
  covers races *within* one process — it can't help across two with no
  shared memory). Added `tauri-plugin-single-instance`, registered first per
  Tauri's requirement: a second launch now just re-summons the one running
  instance's palette instead of starting a competing process. Verified live
  — launched the binary twice and confirmed via `ps` that the second
  process exits (defunct, status 0) instead of staying alive.
- **The hotkey could show the window without it ever actually taking
  keyboard focus.** The palette runs under `ActivationPolicy::Accessory` (no
  Dock icon) — the one case where `WebviewWindow::set_focus()` alone is
  unreliable on macOS. `set_focus()` calls `makeKeyAndOrderFront:` on the
  *window*; it doesn't activate the *process*. Summoned from a background
  global hotkey while a different app holds focus, an accessory app's window
  can end up visually shown but not actually key, so it silently never
  receives keyboard input — indistinguishable from "the hotkey did nothing."
  `show_palette` now also calls `activateIgnoringOtherApps:` on
  `NSApplication`, which `set_focus()` never did.
- **A hotkey press during the close animation could get silently dropped.**
  `closingRef` (read by the toggle-hotkey listener to tell "genuinely idle"
  apart from "still animating closed") was mirrored from React state via a
  `useEffect`, which only runs *after* React commits the next render. A
  hotkey re-summon landing in that gap — e.g. right after `hide_on_blur`
  starts a close — read the *previous* value of `closingRef.current` and hit
  the wrong branch: it called `hide()` again (a no-op, already closing)
  instead of cancelling the close and reopening. `closingRef` is now updated
  synchronously at the same call site as `setClosing`, closing that window
  entirely rather than narrowing it.

## [0.1.14] — 2026-07-17

Fixes an accent-color persistence race: a color chosen in Settings could
silently revert to the default on the next launch.

### Fixed
- **A picked accent color could get clobbered back to the default.**
  `update_settings` fires on every step of the Appearance accent picker
  (each `HexColorPicker` drag frame, and rapid preset clicks), so several
  `update_settings` IPC calls can be in flight at once. Tauri dispatches
  each command invocation to its own task and does not guarantee they
  *complete* in the order they were *issued* — an earlier, slower write
  (e.g. an early drag frame, or an accidental "Amber" click quickly followed
  by the intended color) could finish *after* the final color and silently
  overwrite `settings.json` back to a stale value. The running session kept
  showing the correct color (already applied locally), so nothing looked
  wrong until the next full quit-and-relaunch read the stale value back off
  disk. `SettingsStore::set` now takes a strictly-increasing `rev` the
  frontend bumps once per issued patch, and discards any write whose `rev`
  isn't newer than the last one actually applied — the newest *issued*
  change always wins on disk, regardless of which IPC call happens to land
  first. Covered by `settings::tests::stale_out_of_order_write_is_discarded`.

## [0.1.13] — 2026-07-17

Extends the base theme to the main palette window (it only ever reached the
Settings window before), surfaces failed open/launch actions instead of
silently closing, and removes a rendering artifact around the palette.

### Fixed
- **Theme choice didn't apply to the palette.** Each Tauri window is its own
  webview with an independent `document`; picking Light in Settings only
  ever set `data-theme` on the Settings window's own document; the main
  palette never called `applyTheme()` at all, so it always rendered the dark
  default no matter what was chosen — looking like the setting kept
  resetting on every summon. The palette now applies the persisted theme on
  boot and live on every `settings-changed` broadcast, the same way it
  already did for accent color. Converted the palette's own hardcoded
  `white/*` and `hsla(32,14%,6%,…)` tokens (`App.tsx`, `CommandItem.tsx`,
  `DetailPane.tsx`, `categories.ts`) to the `ink`/`canvas` semantic colors
  introduced in 0.1.12 so Light actually repaints it, not just Settings.
- **Failed file/app opens closed the palette with zero feedback.**
  `execute_action` always reported `acknowledged: true` even when the
  underlying `open`/`xdg-open` call failed (bad or stale path, no default
  handler, a permission gate, …), and the frontend never inspected the
  response before closing — a failed open and a successful one looked
  identical to the user. Added an explicit `success` field to
  `ExecuteActionResponse`; the palette now only closes on success and
  otherwise keeps the panel open with an inline error banner naming the
  actual OS-level failure.
- **Translucent rectangle around the palette.** The panel's ambient
  box-shadow (`blur-90px`/`spread--20px`) reached far past the 12px padding
  around it, but the window itself is transparent with no native shadow —
  so the shadow's soft gradient got hard-clipped at the window's true
  rectangular bounds instead of fading out, showing as a faint rectangular
  edge floating around the rounded card. Shrunk the shadow to fit inside
  the existing padding so it now fades to nothing before it ever reaches
  the window edge.

## [0.1.12] — 2026-07-17

Fixes two settings-window bugs found during a UI pass, and adds a real
light/dark base theme alongside the existing accent picker.

### Fixed
- **Behavior toggle switch overflowed its track.** The switch thumb had no
  base horizontal anchor (`left`), so the browser fell back to a
  static-position heuristic that rendered it flush against — and partly
  outside — the track's right edge in the checked state. Anchored with
  `left-0` so `translate-x-*` offsets are relative to a known origin.
- **Global hotkey capture looked stuck on "Listening…".** The bound toggle
  shortcut stays registered as an OS-level global hotkey the entire time the
  settings window is open, so pressing that combo (or anything already
  intercepted) while recording a new one never reached the capture
  `keydown` listener — the OS grabbed it first. The settings window now
  suspends the active global hotkey for the duration of a capture session
  (`suspend_toggle_shortcut`) and re-arms it on cancel, failure, or unmount
  (`resume_toggle_shortcut`); a successful capture re-registers the new
  combo instead, via the existing `update_settings` → `rebind_toggle` path.

### Added
- **Base theme selector.** Appearance now has a Dark/Light theme picker
  alongside Accent Color, independent of it. Every settings-window surface
  reads its ink/canvas colors from CSS variables (`--ink-rgb`,
  `--canvas-rgb`) flipped by `theme.ts:applyTheme()`, so switching themes
  repaints the whole window instantly — no reload.

## [0.1.11] — 2026-07-16

Completes the 0.1.10 settings window: the accent picker now actually
repaints everything, and the settings UI is more accessible.

### Fixed
- **Accent color didn't fully propagate.** Several surfaces still used the
  hardcoded amber `rgba(245,166,35,…)` directly — the empty-state glow, the
  panel's ambient wash and drop shadow, the scrollbar thumb, and the
  schematic background grid — instead of the `--accent-rgb` CSS variable, so
  they stayed amber regardless of the chosen accent. All now read the
  variable via `rgb(var(--accent-rgb) / α)`.
- **Settings window's own chrome didn't re-theme live.** Picking a color
  updated the preview card (driven by React state) but not the sidebar's
  active-nav highlight, wordmark, or "Saving…" dot (driven by the CSS
  variable), because `applyAccent` only ran on the `settings-changed`
  broadcast round-trip. It's now also called synchronously in the same
  window on every local accent change.
- Extended the sweep to `settings/` (ui.tsx, SettingsApp.tsx, and the
  General/About panes), which had their own separate hardcoded amber
  classes outside the palette's `amber-*` → `accent` rename.

### Added
- `aria-current` on the settings sidebar's active section, `aria-live`
  regions for the hotkey-validation message and the "Saving…/Saved"
  status, and `role="alert"` on the extensions error banner.

## [0.1.10] — 2026-07-16

Adds a proper Settings window instead of relying on defaults baked into
`settings.json` by hand.

### Added
- **Settings window.** `open_settings_window` (wired to the palette's gear
  icon and ⌘,) lazily builds a decorated, resizable preferences window
  separate from the frameless palette overlay. Opening it switches the app's
  macOS activation policy to `Regular` (Dock icon + ⌘-Tab visibility); it
  drops back to `Accessory` when the window is hidden, so the palette stays
  Dock-less the rest of the time. The window hides (not destroys) on close,
  preserving scroll/section state across reopens.
- **Accent color customization.** `Settings.accent_color` (optional
  `#RRGGBB` string) overrides the built-in amber "Instrument" identity.
  `theme.ts:applyAccent()` writes it to the shared `--accent-rgb` CSS
  variable both windows read from; the palette repaints live via the new
  `supersearch://settings-changed` event, no reopen needed. Settings files
  written before this field existed still load, defaulting to `None`.
- **Install extension from folder.** A native folder-picker command
  (`tauri-plugin-dialog`, run on a blocking thread so it can't freeze the
  settings WebView) for installing an extension by directory.

## [0.1.9] — 2026-07-16

Guards against a second, unfixable-in-app-code source of the "hotkey
sometimes doesn't fire" report: a `toggle_shortcut` that collides with a
shortcut macOS itself reserves.

### Fixed
- **Reject shortcuts macOS reserves for input-source switching.**
  `Control+Space` and `Control+Option+Space` are macOS's own defaults for
  "Select the previous/next input source" (System Settings → Keyboard →
  Keyboard Shortcuts → Input Sources) — a system-level Symbolic Hotkey that
  can intercept the keypress before a third-party global-shortcut
  registration ever sees it, so the app's handler fires inconsistently no
  matter what Carbon reports back. `register_toggle` now refuses these
  combos outright instead of silently accepting a binding that will flake.
  If the persisted `toggle_shortcut` is one of them at boot, the app falls
  back to the built-in default (`Alt+Space`) and persists the correction —
  self-healing, since there's no settings UI yet to fix it by hand.

## [0.1.8] — 2026-07-16

Fixes a race between the new exit animation (0.1.7) and the global summon
hotkey that made it seem like the hotkey "sometimes" did nothing.

### Fixed
- **Hotkey swallowed during the close animation.** `window.is_visible()`
  stays `true` for the whole ~150ms exit transition (the native
  `window.hide()` only fires once it finishes), so a hotkey press that
  landed in that window was read as "still open, close it again" instead of
  "reopen" — the press appeared to do nothing. The global-shortcut path now
  emits a distinct `supersearch://toggle-request` event; the frontend (the
  only side that knows whether it's genuinely idle-open or mid-close)
  decides whether that means finish closing or cancel back to visible.
  Escape, selecting a result, and blur-hide are unaffected — those always
  close.

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

[0.1.19]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.19
[0.1.17]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.17
[0.1.16]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.16
[0.1.15]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.15
[0.1.14]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.14
[0.1.13]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.13
[0.1.12]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.12
[0.1.11]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.11
[0.1.10]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.10
[0.1.9]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.9
[0.1.8]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.8
[0.1.7]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.7
[0.1.6]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.6
[0.1.5]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.5
[0.1.4]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.4
[0.1.3]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.3
[0.1.1]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.1
[0.1.0]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.0
