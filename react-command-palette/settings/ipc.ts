/**
 * Typed IPC for the settings window.
 *
 * Reuses the same Tauri-or-mock strategy as the palette's `bridge.ts`: in a
 * Tauri window it calls the real Rust commands; in a plain browser
 * (`npm run dev`, opening /settings.html) it falls back to in-memory mocks so
 * the whole UI is developable without the desktop shell.
 */
import { invoke as tauriInvoke, isTauri } from "../bridge";
import type { ExtensionInfo } from "./types";
import type { Settings } from "./types";

// ── Settings ────────────────────────────────────────────────────────────────

export async function getSettings(): Promise<Settings> {
  if (isTauri) return tauriInvoke<Settings>("get_settings");
  return { ...mockSettings };
}

/** `rev` is a strictly-increasing counter the caller bumps once per issued
 *  patch (see `SettingsApp.tsx`'s `patchSettings`) — the settings window
 *  fires this on every step of a color-picker drag, so several calls can be
 *  in flight at once with no guarantee they resolve in the order they were
 *  issued. The backend (and this mock, for parity) discard a write whose
 *  `rev` isn't newer than the last-applied one, so a slow, out-of-order
 *  early drag frame can never clobber the final color back onto disk. */
export async function updateSettings(settings: Settings, rev: number): Promise<void> {
  if (isTauri) {
    await tauriInvoke("update_settings", { settings, rev });
    return;
  }
  if (rev <= mockAppliedRev) return; // stale — a newer patch already won
  mockAppliedRev = rev;
  mockSettings = { ...settings };
}

/** Rust-side validation of a candidate accelerator string. `ok: false` carries
 *  a human-readable reason (e.g. a macOS-reserved combo). */
export interface ShortcutCheck {
  ok: boolean;
  reason?: string;
}

export async function validateShortcut(shortcut: string): Promise<ShortcutCheck> {
  if (isTauri) return tauriInvoke<ShortcutCheck>("validate_shortcut", { shortcut });
  // Mirror the Rust reserved-combo rule for browser dev.
  const parts = shortcut.split("+").map((p) => p.trim().toLowerCase());
  const key = parts.filter((p) => !MODS[p]).join("");
  const mods = parts.filter((p) => MODS[p]).map(normMod).sort();
  const reserved =
    key === "space" &&
    (JSON.stringify(mods) === JSON.stringify(["control"]) ||
      JSON.stringify(mods) === JSON.stringify(["alt", "control"]));
  return reserved
    ? { ok: false, reason: `"${shortcut}" is reserved by macOS for switching input sources.` }
    : { ok: true };
}

/** Unregister the currently-bound global hotkey for the duration of a
 *  capture session, so every keystroke (including the combo already bound)
 *  reaches the settings window instead of being swallowed by the OS-level
 *  shortcut hook. No-op in browser dev, where no real hotkey is registered. */
export async function suspendToggleShortcut(): Promise<void> {
  if (isTauri) await tauriInvoke("suspend_toggle_shortcut");
}

/** Re-arm the persisted toggle hotkey once a capture session ends
 *  (committed, cancelled, or the pane unmounts mid-capture). */
export async function resumeToggleShortcut(): Promise<void> {
  if (isTauri) await tauriInvoke("resume_toggle_shortcut");
}

// ── Extensions ───────────────────────────────────────────────────────────────

export async function listExtensions(): Promise<ExtensionInfo[]> {
  if (isTauri) return tauriInvoke<ExtensionInfo[]>("list_extensions");
  return mockExtensions.map((e) => ({ ...e }));
}

export async function installExtension(path: string): Promise<string> {
  if (isTauri) return tauriInvoke<string>("install_extension", { path });
  const id = path.split("/").filter(Boolean).pop() ?? "extension";
  mockExtensions.push({
    id, name: id, version: "1.0.0", author: null, description: "Locally installed",
    kind: "script", enabled: false, trusted: false, needs_trust: true, permissions: [],
  });
  return id;
}

export async function uninstallExtension(id: string): Promise<void> {
  if (isTauri) { await tauriInvoke("uninstall_extension", { id }); return; }
  mockExtensions = mockExtensions.filter((e) => e.id !== id);
}

export async function setExtensionEnabled(id: string, enabled: boolean): Promise<void> {
  if (isTauri) { await tauriInvoke("set_extension_enabled", { id, enabled }); return; }
  const e = mockExtensions.find((x) => x.id === id);
  if (e) e.enabled = enabled;
}

export async function setExtensionTrusted(id: string, trusted: boolean): Promise<void> {
  if (isTauri) { await tauriInvoke("set_extension_trusted", { id, trusted }); return; }
  const e = mockExtensions.find((x) => x.id === id);
  if (e) { e.trusted = trusted; e.needs_trust = !trusted; }
}

/** Open a native file picker for an extension directory. Returns null if the
 *  user cancelled. Tauri-only; browser dev returns a fake path. */
export async function pickExtensionDir(): Promise<string | null> {
  if (isTauri) return tauriInvoke<string | null>("pick_extension_dir");
  return "/Users/you/dev/my-extension";
}

// ── Browser-dev mocks ────────────────────────────────────────────────────────

const MODS: Record<string, true> = {
  control: true, ctrl: true, alt: true, option: true, shift: true,
  super: true, cmd: true, command: true, meta: true, commandorcontrol: true, cmdorctrl: true,
};
function normMod(m: string): string {
  if (m === "ctrl") return "control";
  if (m === "option") return "alt";
  if (["cmd", "command", "meta", "commandorcontrol", "cmdorctrl"].includes(m)) return "super";
  return m;
}

let mockSettings: Settings = {
  toggle_shortcut: "Alt+Space",
  hide_on_blur: true,
  theme: "dark",
  accent_color: null,
};
/** Mirrors the rev guard in `SettingsStore::set` for browser-dev parity. */
let mockAppliedRev = 0;

let mockExtensions: ExtensionInfo[] = [
  {
    id: "ddg", name: "DuckDuckGo Search", version: "1.0.0", author: "SuperSearch",
    description: "Open a DuckDuckGo search from the palette", kind: "script",
    enabled: true, trusted: true, needs_trust: false,
    permissions: [{ permission: "NetworkConnect", justification: "Open search results in your default browser" }],
  },
  {
    id: "weather", name: "Weather", version: "0.2.0", author: null,
    description: "Current conditions for a city", kind: "script",
    enabled: false, trusted: false, needs_trust: true,
    permissions: [{ permission: "NetworkConnect", justification: "Fetch forecast data" }],
  },
];
