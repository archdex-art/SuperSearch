/** Mirrors `src-tauri/src/settings.rs::Settings`. */
export interface Settings {
  toggle_shortcut: string;
  hide_on_blur: boolean;
  /** Base theme id — "dark" is the only built-in today. Accent color and
   *  font are separate fields so a future light theme doesn't collide with
   *  a user's chosen accent. */
  theme: string;
  /** Accent hex, e.g. "#f5a623". Absent → the built-in amber default. */
  accent_color?: string | null;
}

/** Mirrors `crates/supersearch-runtime/src/extension/registry.rs::ExtensionInfo`. */
export interface ExtensionInfo {
  id: string;
  name: string;
  version: string;
  author: string | null;
  description: string | null;
  kind: "script" | "wasm";
  enabled: boolean;
  trusted: boolean;
  needs_trust: boolean;
  permissions: PermissionInfo[];
}

/** Mirrors `PermissionInfo`. */
export interface PermissionInfo {
  permission: string;
  justification: string;
}
