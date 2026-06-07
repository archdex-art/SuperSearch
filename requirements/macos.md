# macOS — Build Requirements

Minimum OS: **macOS 10.15 Catalina** (as set in `tauri.conf.json`).

## Required

| Tool | Version | Install |
|------|---------|---------|
| Xcode Command Line Tools | latest | `xcode-select --install` |
| Rust (stable) | ≥ 1.77 | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | ≥ 20 LTS | https://nodejs.org or `brew install node` |
| npm | bundled with Node | — |

## Optional (signing & notarization)

| Tool | Purpose |
|------|---------|
| Apple Developer account | Code-sign the `.app` and notarize with Apple |
| `tauri signer` | Generate the updater signing keypair (`tauri signer generate`) |

## Install steps

```bash
# 1. Xcode CLI tools
xcode-select --install

# 2. Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 3. Node dependencies
npm --prefix react-command-palette ci

# 4. Dev run
cargo tauri dev

# 5. Release build
cargo tauri build
```

## Runtime permissions (required at first launch)

| Permission | Where to grant |
|------------|---------------|
| Accessibility | System Settings → Privacy & Security → Accessibility |

SuperSearch requests this on first launch via an osascript probe. Without it,
`appcmd:` keystroke injection and some `sys:` commands will fail.
