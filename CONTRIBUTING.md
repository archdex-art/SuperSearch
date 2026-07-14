# Contributing to SuperSearch

Thanks for looking at this. SuperSearch is a small, fast-moving project — the
fastest way in is usually a small PR, not a design doc.

## Before you start

- **Bugs / small fixes:** just open a PR.
- **New `AgentIntent` variants, new capability permissions, or anything
  touching `capability/gate.rs`:** open an issue first. These affect the
  security model in [docs/security.md](docs/security.md) and need agreement
  on the permission shape before code.
- **Roadmap items:** check [ROADMAP.md](ROADMAP.md) — if what you want to
  build is already scoped there (or explicitly listed under "not planned"),
  say so in your issue/PR so we're aligned before you invest time.

## Dev setup

1. Install prerequisites for your platform: [`requirements/macos.md`](requirements/macos.md),
   [`requirements/linux.md`](requirements/linux.md), or
   [`requirements/windows.md`](requirements/windows.md).
2. Run the dev server:
   ```bash
   cd src-tauri
   cargo tauri dev
   ```
3. Package a local bundle:
   ```bash
   cd src-tauri
   cargo tauri build
   ```
   Output lands in `src-tauri/target/release/bundle/`.

The React palette (`react-command-palette/`) has its own toolchain:
```bash
npm --prefix react-command-palette ci
npm --prefix react-command-palette run build
```

## Before opening a PR

- `cargo build --workspace` must succeed — this compiles both the Tauri app
  and the runtime kernel, and is the fastest way to catch cross-crate
  breakage.
- `cargo test --workspace` for anything touching `crates/supersearch-runtime`
  — the capability gate, journal, and executor all have unit tests that
  encode the security invariants in [docs/security.md](docs/security.md)
  (e.g. `action_without_capability_is_blocked_before_touching_the_os`,
  `clipboard_write_roundtrips_untrusted_content`). A PR that weakens one of
  these needs to explain why in the description, not just delete the test.
- If you touched `react-command-palette/`, the frontend CI job
  (`.github/workflows/ci.yml`) runs a TypeScript typecheck + Vite build —
  reproduce locally with the npm commands above before pushing.
- `RUSTFLAGS="-D warnings"` is enforced in CI — warnings fail the build, not
  just lint.

## Code conventions

- **New `AgentIntent`:** add the variant in `agent/patterns.rs`, extend
  `agent/planner.rs` to compile it into `TaskNode`s, add the executor branch,
  and require an explicit `Permission` through the gate — never bypass it.
  See [docs/architecture.md](docs/architecture.md#extending-the-surface).
- **OS actions always go through argv**, never `sh -c` with interpolated
  user data. This is enforced by review, not just tests — see
  [docs/security.md](docs/security.md) for the specific invariant.
- **Platform-specific code** goes behind the `PlatformBackend` trait
  (`platform/exec.rs`) with per-OS implementations in `platform/{macos,linux,windows}.rs`,
  so IPC handlers stay platform-agnostic.
- Keep module-level doc comments (`//! …`) accurate — they're the source for
  [docs/architecture.md](docs/architecture.md)'s module map. If your PR
  changes a module's responsibility, update its `//!` block in the same PR.

## Extensions

Building an extension doesn't require touching the kernel at all — see
[docs/extensions.md](docs/extensions.md) and the runnable examples in
`examples/extensions/`. Extension PRs (new example extensions, manifest
schema fixes) are welcome and reviewed faster than core kernel changes.

## Release process

Cutting a release (tagging, signing, notarization) is documented separately
in [RELEASING.md](RELEASING.md) — contributors don't need it unless you're
maintaining CI/CD.

## Reporting security issues

Don't open a public issue for anything that bypasses the capability gate,
achieves shell injection, or lets an extension escalate beyond its granted
permissions. Use a
[private security advisory](https://github.com/archdex-art/SuperSearch/security/advisories/new)
instead.
