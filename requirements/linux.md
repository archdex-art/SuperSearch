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

## Known limitations

### Global hotkey doesn't fire under GNOME Wayland sessions

Verified live on Ubuntu 24.04 (GNOME 46, Wayland — the default session type):
`register_hotkey` succeeds and logs `Global toggle shortcut registered`, but
the summon never fires — confirmed by injecting the exact key combo at the
X11 protocol level (`xdotool key alt+space` through XWayland, which XTest
delivers identically to a real keypress reaching a passive `XGrabKey` grab)
and observing no `Toggle hotkey fired` log line.

Root cause is upstream, not app-specific: SuperSearch's global shortcut
plugin is built on the `global-hotkey` crate (v0.8.0), whose only Linux
backend is `x11rb`/`XGrabKey` — there is no Wayland-native path. On GNOME's
Mutter compositor, key grabs registered by an X11 client (even via XWayland)
are not reliably forwarded to unfocused background clients under Wayland;
the crate's own connect error even says so: *"Other window systems on Linux
are not supported by `global-hotkey` crate."* Properly fixing this means
adding `org.freedesktop.portal.GlobalShortcuts` (the xdg-desktop-portal
D-Bus API) support — a real feature addition (new async D-Bus session
handshake, user consent dialog, `Activated` signal handling), not a small
patch, and out of scope here.

**Workaround:** log in via "Ubuntu on Xorg" (select the gear icon on the
GDM login screen) instead of the default Wayland session — the same
`XGrabKey` mechanism works reliably on a real X11 session since key grabs
don't need cross-protocol forwarding.

All other `sys:*` commands were verified working normally under this same
Wayland session (Do Not Disturb, Dark Mode toggle, Empty Trash, Show
Desktop via `wmctrl` through XWayland, terminal launch, system info) — this
limitation is specific to the global summon hotkey.
