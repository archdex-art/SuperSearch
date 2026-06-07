# Windows — Build Requirements

Minimum OS: **Windows 10 version 1803** (WebView2 requirement).

## Required tools

| Tool | Version | Install |
|------|---------|---------|
| Rust (stable, MSVC) | ≥ 1.77 | https://rustup.rs — choose *MSVC* toolchain |
| Visual Studio Build Tools | 2019 or 2022 | https://visualstudio.microsoft.com/visual-cpp-build-tools/ — include "Desktop development with C++" workload |
| Node.js | ≥ 20 LTS | https://nodejs.org |
| WebView2 Runtime | any | Pre-installed on Windows 10/11; installer at https://developer.microsoft.com/en-us/microsoft-edge/webview2/ |

## Rust target

SuperSearch builds with the MSVC ABI. After installing Rust, confirm the toolchain:

```powershell
rustup toolchain install stable-x86_64-pc-windows-msvc
rustup default stable-x86_64-pc-windows-msvc
```

## Optional (runtime tools)

These are already present on all modern Windows installs:

| Tool | Purpose | Available since |
|------|---------|----------------|
| `explorer.exe` | Open files / launch apps | Windows XP |
| `tasklist` | List running processes | Windows XP |
| `taskkill` | Quit a process | Windows XP |
| `powershell` | Clipboard, dark mode, DND, show desktop | Windows 7 |
| `rundll32` | Lock screen, sleep | Windows XP |
| `SnippingTool.exe` | Screenshot | Windows 7 |
| `where` | File search | Windows XP |
| `reg` | Registry queries (dark mode toggle) | Windows XP |
| `systeminfo` | System information | Windows XP |
| `cmd` | Open terminal, launch URLs | Windows XP |

No additional runtime installs are required on Windows 10+.

## Install steps

```powershell
# 1. Install Rust (MSVC toolchain) from https://rustup.rs
# 2. Install VS Build Tools (C++ workload)

# 3. Install Node dependencies
npm --prefix react-command-palette ci

# 4. Dev run
cargo tauri dev

# 5. Release build (produces .msi + NSIS .exe in src-tauri\target\release\bundle\)
cargo tauri build
```

## Code signing (optional, for distribution)

To sign the NSIS/MSI installer with a Windows code-signing certificate, set these
environment variables before running `cargo tauri build` or in GitHub Actions secrets:

| Secret | Value |
|--------|-------|
| `WINDOWS_CERTIFICATE` | Base64-encoded `.pfx` file |
| `WINDOWS_CERTIFICATE_PASSWORD` | Password for the `.pfx` |

Then add to the `bundle.windows` section in `tauri.conf.json`:

```json
"certificateThumbprint": "<YOUR_THUMBPRINT>",
"digestAlgorithm": "sha256",
"timestampUrl": "http://timestamp.digicert.com"
```

## Global shortcut

SuperSearch registers a global hotkey (default `CmdOrCtrl+Space`).
Windows may require the app to be run as a regular user (not elevated) for
global shortcut registration to work correctly.
