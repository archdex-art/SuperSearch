# Changelog

All notable changes to SuperSearch are documented here. Format loosely
follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); versions
correspond to [GitHub Releases](https://github.com/archdex-art/SuperSearch/releases)
and their published installers.

## [Unreleased]

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

[Unreleased]: https://github.com/archdex-art/SuperSearch/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.1
[0.1.0]: https://github.com/archdex-art/SuperSearch/releases/tag/v0.1.0
