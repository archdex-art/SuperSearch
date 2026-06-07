# Linux — Build Requirements

Tested on: **Ubuntu 22.04 LTS** (Jammy). Other distros work if equivalent packages are available.

## Required system packages

```bash
sudo apt-get update
sudo apt-get install -y \
    libwebkit2gtk-4.1-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    patchelf \
    build-essential \
    pkg-config
```

> **Fedora / RHEL equivalent:**
> ```bash
> sudo dnf install webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel \
>     librsvg2-devel patchelf gcc
> ```

## Required tools

| Tool | Version | Install |
|------|---------|---------|
| Rust (stable) | ≥ 1.77 | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | ≥ 20 LTS | `sudo apt-get install nodejs` or https://nodejs.org |
| npm | bundled with Node | — |

## Optional (strongly recommended at runtime)

These are **not** needed to build, but SuperSearch uses them at runtime:

| Package | Purpose |
|---------|---------|
| `locate` / `plocate` | Fast file search (`sys:` file results) |
| `wmctrl` | Show Desktop, list running apps, switch/quit apps |
| `xdg-utils` | Open files and URLs (`xdg-open`) |
| `gtk-launch` | Launch apps by `.desktop` name |
| `wl-clipboard` (Wayland) | Clipboard read/write on Wayland sessions |
| `xclip` (X11) | Clipboard read/write on X11 sessions |
| `gnome-screenshot` or `scrot` | Screenshot command |
| `loginctl` | Lock screen |
| `systemctl` | Sleep / suspend |
| `gio` (glib2) | Empty Trash |
| `gsettings` | Do Not Disturb + Dark Mode toggle (GNOME) |

```bash
# Install all runtime dependencies at once (Debian/Ubuntu):
sudo apt-get install -y \
    plocate wmctrl xdg-utils libgtk-3-bin wl-clipboard xclip \
    gnome-screenshot scrot libglib2.0-bin
```

## Install steps

```bash
# 1. System packages (see above)

# 2. Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 3. Node dependencies
npm --prefix react-command-palette ci

# 4. Dev run
cargo tauri dev

# 5. Release build (produces .deb + .AppImage in src-tauri/target/release/bundle/)
cargo tauri build
```

## File search database

`locate` requires an initial database build. Run once after install:

```bash
sudo updatedb
```
