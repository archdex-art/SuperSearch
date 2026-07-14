import { useCallback, useEffect, useState } from "react";
import { invoke, type AppSettings } from "./bridge";

const DEFAULTS: AppSettings = { toggle_shortcut: "Alt+Space", hide_on_blur: true, theme: "dark" };

/**
 * Settings view — the global summon hotkey, hide-on-blur, and theme. Persisted
 * via `get_settings`/`update_settings`. Replaces the legacy `ui/scripts/settings.js`.
 */
export function Settings({ onClose }: { onClose: () => void }) {
  const [s, setS] = useState<AppSettings>(DEFAULTS);
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const loaded = await invoke<AppSettings | null>("get_settings");
        if (loaded) setS({ ...DEFAULTS, ...loaded });
      } catch {
        /* keep defaults */
      }
    })();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const save = useCallback(async () => {
    const next: AppSettings = {
      toggle_shortcut: s.toggle_shortcut.trim() || DEFAULTS.toggle_shortcut,
      hide_on_blur: s.hide_on_blur,
      theme: s.theme,
    };
    try {
      await invoke("update_settings", { settings: next });
      setStatus("Saved");
    } catch (e) {
      setStatus(`Failed: ${e}`);
    }
  }, [s]);

  return (
    <div className="flex h-full flex-col gap-4 p-5 text-zinc-100">
      <header className="flex items-center justify-between">
        <h2 className="text-sm font-semibold tracking-wide text-zinc-200">Settings</h2>
        <button
          onClick={onClose}
          className="rounded-md px-2 py-1 text-xs text-zinc-400 hover:bg-white/10 hover:text-zinc-100"
        >
          Esc · Back
        </button>
      </header>

      <label className="flex flex-col gap-1 text-xs text-zinc-400">
        Global summon shortcut
        <input
          value={s.toggle_shortcut}
          onChange={(e) => setS((p) => ({ ...p, toggle_shortcut: e.target.value }))}
          placeholder="Alt+Space"
          className="rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-sm text-zinc-100 outline-none focus:border-white/25"
        />
      </label>

      <label className="flex items-center justify-between text-xs text-zinc-300">
        Hide when the window loses focus
        <input
          type="checkbox"
          checked={s.hide_on_blur}
          onChange={(e) => setS((p) => ({ ...p, hide_on_blur: e.target.checked }))}
          className="h-4 w-4 accent-emerald-500"
        />
      </label>

      <label className="flex items-center justify-between text-xs text-zinc-300">
        Theme
        <select
          value={s.theme}
          onChange={(e) => setS((p) => ({ ...p, theme: e.target.value as AppSettings["theme"] }))}
          className="rounded-md border border-white/10 bg-black/30 px-2 py-1 text-sm text-zinc-100 outline-none focus:border-white/25"
        >
          <option value="dark">Dark</option>
          <option value="light">Light</option>
        </select>
      </label>

      <div className="mt-auto flex items-center gap-3">
        <button
          onClick={() => void save()}
          className="rounded-md bg-emerald-500/80 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-500"
        >
          Save
        </button>
        {status && <span className="text-xs text-zinc-400">{status}</span>}
      </div>
    </div>
  );
}
