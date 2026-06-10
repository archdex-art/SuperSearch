import { useCallback, useEffect, useState } from "react";
import { invoke, type ExtensionInfo } from "./bridge";

/**
 * Extension manager view — install, enable (with permission consent), trust
 * (for unsandboxed script extensions), and uninstall extensions. Replaces the
 * legacy `ui/scripts/extensions.js` Plugin Manager. All actions go through the
 * Tauri IPC commands backed by `ExtensionRegistry`.
 */
export function ExtensionManager({ onClose }: { onClose: () => void }) {
  const [items, setItems] = useState<ExtensionInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [path, setPath] = useState("");
  // Pending confirmation: enable (show requested permissions) or trust.
  const [confirm, setConfirm] = useState<
    { kind: "enable" | "trust"; ext: ExtensionInfo } | null
  >(null);

  const refresh = useCallback(async () => {
    try {
      setItems((await invoke<ExtensionInfo[]>("list_extensions")) ?? []);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Esc closes the manager (or a pending confirm dialog first).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      if (confirm) setConfirm(null);
      else onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [confirm, onClose]);

  const run = useCallback(
    async (id: string, op: () => Promise<unknown>) => {
      setBusy(id);
      try {
        await op();
        await refresh();
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(null);
      }
    },
    [refresh],
  );

  const onToggle = (ext: ExtensionInfo) => {
    if (ext.enabled) {
      void run(ext.id, () => invoke("set_extension_enabled", { id: ext.id, enabled: false }));
    } else {
      // Enabling grants a capability token — confirm the requested permissions.
      setConfirm({ kind: "enable", ext });
    }
  };

  const confirmAction = () => {
    if (!confirm) return;
    const { kind, ext } = confirm;
    setConfirm(null);
    if (kind === "enable") {
      void run(ext.id, () => invoke("set_extension_enabled", { id: ext.id, enabled: true }));
    } else {
      void run(ext.id, () => invoke("set_extension_trusted", { id: ext.id, trusted: true }));
    }
  };

  const onInstall = () => {
    const p = path.trim();
    if (!p) return;
    void run("__install__", async () => {
      await invoke("install_extension", { path: p });
      setPath("");
    });
  };

  return (
    <div className="flex h-full flex-col gap-3 p-4 text-zinc-100">
      <header className="flex items-center justify-between">
        <h2 className="text-sm font-semibold tracking-wide text-zinc-200">Extensions</h2>
        <button
          onClick={onClose}
          className="rounded-md px-2 py-1 text-xs text-zinc-400 hover:bg-white/10 hover:text-zinc-100"
        >
          Esc · Back
        </button>
      </header>

      {/* Install from a local folder (manifest.toml + entrypoint). */}
      <div className="flex gap-2">
        <input
          value={path}
          onChange={(e) => setPath(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && onInstall()}
          placeholder="Path to an extension folder…"
          className="min-w-0 flex-1 rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs outline-none placeholder:text-zinc-500 focus:border-white/25"
        />
        <button
          onClick={onInstall}
          disabled={!path.trim() || busy === "__install__"}
          className="rounded-md bg-white/10 px-3 py-1.5 text-xs font-medium hover:bg-white/20 disabled:opacity-40"
        >
          Install
        </button>
      </div>

      {error && <p className="rounded-md bg-red-500/15 px-2 py-1 text-xs text-red-300">{error}</p>}

      <ul className="flex flex-col gap-2 overflow-y-auto">
        {items.length === 0 && (
          <li className="py-8 text-center text-xs text-zinc-500">No extensions installed</li>
        )}
        {items.map((ext) => (
          <li key={ext.id} className="rounded-lg border border-white/10 bg-white/[0.03] p-3">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <span className="truncate text-sm font-medium">{ext.name}</span>
                  <span className="rounded bg-white/10 px-1.5 py-0.5 text-[10px] uppercase text-zinc-400">
                    {ext.kind}
                  </span>
                  <span className="text-[10px] text-zinc-500">v{ext.version}</span>
                </div>
                {ext.description && (
                  <p className="mt-0.5 truncate text-xs text-zinc-400">{ext.description}</p>
                )}
                {ext.permissions.length > 0 && (
                  <div className="mt-1.5 flex flex-wrap gap-1">
                    {ext.permissions.map((p) => (
                      <span
                        key={p.permission}
                        title={p.justification}
                        className="rounded bg-amber-500/15 px-1.5 py-0.5 text-[10px] text-amber-300"
                      >
                        {p.permission}
                      </span>
                    ))}
                  </div>
                )}
                {ext.needs_trust && (
                  <p className="mt-1.5 text-[11px] text-orange-300">
                    ⚠ Unsandboxed script — needs trust to run.
                  </p>
                )}
              </div>

              <div className="flex shrink-0 items-center gap-2">
                {ext.needs_trust && (
                  <button
                    onClick={() => setConfirm({ kind: "trust", ext })}
                    disabled={busy === ext.id}
                    className="rounded-md bg-orange-500/20 px-2 py-1 text-[11px] font-medium text-orange-200 hover:bg-orange-500/30 disabled:opacity-40"
                  >
                    Trust
                  </button>
                )}
                <button
                  onClick={() => onToggle(ext)}
                  disabled={busy === ext.id}
                  className={
                    "rounded-md px-2 py-1 text-[11px] font-medium disabled:opacity-40 " +
                    (ext.enabled
                      ? "bg-emerald-500/20 text-emerald-200 hover:bg-emerald-500/30"
                      : "bg-white/10 text-zinc-300 hover:bg-white/20")
                  }
                >
                  {ext.enabled ? "Enabled" : "Enable"}
                </button>
                <button
                  onClick={() =>
                    run(ext.id, () => invoke("uninstall_extension", { id: ext.id }))
                  }
                  disabled={busy === ext.id}
                  title="Uninstall"
                  className="rounded-md px-2 py-1 text-[11px] text-zinc-500 hover:bg-red-500/15 hover:text-red-300 disabled:opacity-40"
                >
                  Remove
                </button>
              </div>
            </div>
          </li>
        ))}
      </ul>

      {/* Consent / trust confirmation. */}
      {confirm && (
        <div
          className="absolute inset-0 z-10 flex items-center justify-center bg-black/50 p-6"
          onMouseDown={(e) => e.target === e.currentTarget && setConfirm(null)}
        >
          <div className="w-full max-w-sm rounded-xl border border-white/10 bg-zinc-900 p-4 shadow-2xl">
            {confirm.kind === "enable" ? (
              <>
                <h3 className="text-sm font-semibold">Enable {confirm.ext.name}?</h3>
                <p className="mt-1 text-xs text-zinc-400">
                  This grants the extension a revocable capability token for:
                </p>
                <ul className="mt-2 flex flex-wrap gap-1">
                  {confirm.ext.permissions.length === 0 && (
                    <li className="text-xs text-zinc-500">No special permissions.</li>
                  )}
                  {confirm.ext.permissions.map((p) => (
                    <li key={p.permission} className="rounded bg-amber-500/15 px-1.5 py-0.5 text-[11px] text-amber-300">
                      {p.permission}
                    </li>
                  ))}
                </ul>
              </>
            ) : (
              <>
                <h3 className="text-sm font-semibold text-orange-300">Trust {confirm.ext.name}?</h3>
                <p className="mt-1 text-xs text-zinc-400">
                  This is a <strong>script</strong> extension. It runs with your full user
                  privileges and is <strong>not sandboxed</strong>. Only trust extensions whose
                  code you have reviewed.
                </p>
              </>
            )}
            <div className="mt-4 flex justify-end gap-2">
              <button
                onClick={() => setConfirm(null)}
                className="rounded-md px-3 py-1.5 text-xs text-zinc-400 hover:bg-white/10"
              >
                Cancel
              </button>
              <button
                onClick={confirmAction}
                className={
                  "rounded-md px-3 py-1.5 text-xs font-medium " +
                  (confirm.kind === "trust"
                    ? "bg-orange-500/80 text-white hover:bg-orange-500"
                    : "bg-emerald-500/80 text-white hover:bg-emerald-500")
                }
              >
                {confirm.kind === "trust" ? "Trust" : "Enable"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
