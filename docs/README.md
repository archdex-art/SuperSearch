# SuperSearch Documentation

This is the detailed reference for SuperSearch, split out from the top-level
[README](../README.md) (which stays focused on install + quick start). Start
here if you're integrating with the runtime, writing an extension, or trying
to understand how a query becomes an OS action.

| Doc | Covers |
|---|---|
| [Architecture](architecture.md) | Process layout, module map, how a keystroke becomes an OS action |
| [Usage Guide](usage.md) | Every input mode: unified search, natural language, `/app`, `$ shell` |
| [Extensions](extensions.md) | Manifest format, script + WASM execution models, IPC surface |
| [Security Model](security.md) | Capability gating, journaling, what's enforced vs. aspirational |

Platform-specific build requirements live in [`requirements/`](../requirements/)
(`macos.md`, `linux.md`, `windows.md`). Release engineering (signing,
notarization, auto-update) is documented in [RELEASING.md](../RELEASING.md).

See also: [ROADMAP.md](../ROADMAP.md) · [CONTRIBUTING.md](../CONTRIBUTING.md) ·
[CHANGELOG.md](../CHANGELOG.md).
