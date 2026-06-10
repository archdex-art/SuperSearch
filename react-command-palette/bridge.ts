/**
 * Tauri IPC bridge for the React front-end.
 *
 * In a Tauri window it calls the real Rust commands; in a plain browser
 * (`npm run dev`) it falls back to mock data so the UI is still developable.
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Backend search result (mirrors src-tauri SearchResult). */
export interface BackendResult {
  id: string;
  title: string;
  subtitle: string;
  category: string;
  icon: string;
  score: number;
  /**
   * For extension results (`category === "Extension"`, id `ext:<id>::<title>`),
   * the declared action to run via `execute_extension_action`. Absent otherwise.
   */
  action?: unknown | null;
}

/** Extension query hit (mirrors ExtensionQueryHit). */
export interface ExtensionHit {
  extension_id: string;
  title: string;
  subtitle: string;
  action: unknown | null;
}

/** Persisted app settings (mirrors the Rust settings store). */
export interface AppSettings {
  toggle_shortcut: string;
  hide_on_blur: boolean;
  theme: "dark" | "light";
}

/** A requested permission, rendered in the consent dialog. */
export interface PermissionInfo {
  permission: string;
  justification: string;
}

/** Installed-extension summary (mirrors ExtensionInfo). */
export interface ExtensionInfo {
  id: string;
  name: string;
  version: string;
  author?: string | null;
  description?: string | null;
  kind: "script" | "wasm";
  enabled: boolean;
  /** User has trusted this (unsandboxed) script extension to run. */
  trusted: boolean;
  /** A script that still needs an explicit trust grant before it will run. */
  needs_trust: boolean;
  permissions: PermissionInfo[];
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) return tauriInvoke<T>(cmd, args);
  return mock(cmd, args) as T;
}

export async function listen(event: string, cb: () => void): Promise<UnlistenFn> {
  if (isTauri) return tauriListen(event, cb);
  return () => {};
}

// ── Browser-dev mocks ──────────────────────────────────────────────────────
const MOCK: BackendResult[] = [
  { id: "app:/Applications/Apple Music.app", title: "Apple Music", subtitle: "/Applications/Apple Music.app", category: "Application", icon: "🎵", score: 0.9 },
  { id: "app:/Applications/Visual Studio Code.app", title: "Visual Studio Code", subtitle: "/Applications/Visual Studio Code.app", category: "Application", icon: "🧩", score: 0.85 },
  { id: "app:/Applications/Slack.app", title: "Slack", subtitle: "/Applications/Slack.app", category: "Application", icon: "💬", score: 0.8 },
  { id: "app:/Applications/Figma.app", title: "Figma", subtitle: "/Applications/Figma.app", category: "Application", icon: "🎨", score: 0.75 },
  { id: "sys:lock", title: "Lock Screen", subtitle: "Lock the screen immediately", category: "System", icon: "🔒", score: 0.0 },
  { id: "sys:dark_mode", title: "Toggle Dark Mode", subtitle: "Switch appearance", category: "System", icon: "🌗", score: 0.0 },
  { id: "sys:screenshot", title: "Screenshot", subtitle: "Capture the screen", category: "System", icon: "📸", score: 0.0 },
];

function mock(cmd: string, args?: Record<string, unknown>): unknown {
  const q = String((args?.query as string) ?? "").toLowerCase();
  switch (cmd) {
    case "search_query": {
      if (!q) return [];
      return MOCK.filter((r) => `${r.title} ${r.subtitle}`.toLowerCase().includes(q));
    }
    case "query_extensions":
      return [];
    case "agent_query":
      return { query: q, intent: "Mock", plan_description: q, total_steps: 1, steps: [], success: true, summary: `Mock ran: ${q}`, duration_ms: 12 };
    case "agent_check":
      return /^open |^launch |^search /.test(q);
    case "execute_action":
    case "execute_extension_action":
    case "hide_window":
      return null;
    case "get_settings":
      return { toggle_shortcut: "Alt+Space", hide_on_blur: true, theme: "dark" };
    case "list_extensions":
      return [
        {
          id: "ddg", name: "DuckDuckGo", version: "1.0.0", author: "demo",
          description: "Instant answers", kind: "script", enabled: true,
          trusted: false, needs_trust: true,
          permissions: [{ permission: "NetworkConnect", justification: "fetch answers" }],
        },
      ];
    case "install_extension":
      return "installed-id";
    case "set_extension_enabled":
    case "set_extension_trusted":
    case "uninstall_extension":
    case "update_settings":
      return null;
    default:
      return null;
  }
}
