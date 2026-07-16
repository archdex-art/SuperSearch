import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import type { ExtensionInfo } from "../types";
import {
  installExtension,
  listExtensions,
  pickExtensionDir,
  setExtensionEnabled,
  setExtensionTrusted,
  uninstallExtension,
} from "../ipc";
import { Button, Card, Pill, SectionHeading, Toggle } from "../ui";

export function ExtensionsPane() {
  const [extensions, setExtensions] = useState<ExtensionInfo[] | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmUninstall, setConfirmUninstall] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setExtensions(await listExtensions());
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleInstall = useCallback(async () => {
    setError(null);
    const dir = await pickExtensionDir();
    if (!dir) return;
    setInstalling(true);
    try {
      await installExtension(dir);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setInstalling(false);
    }
  }, [refresh]);

  const handleUninstall = useCallback(
    async (id: string) => {
      setConfirmUninstall(null);
      setError(null);
      try {
        await uninstallExtension(id);
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [refresh],
  );

  const toggleEnabled = useCallback(
    async (id: string, enabled: boolean) => {
      setExtensions((prev) => prev?.map((e) => (e.id === id ? { ...e, enabled } : e)) ?? prev);
      try {
        await setExtensionEnabled(id, enabled);
      } catch (e) {
        setError(String(e));
        await refresh(); // roll back the optimistic flip
      }
    },
    [refresh],
  );

  const toggleTrusted = useCallback(
    async (id: string, trusted: boolean) => {
      setExtensions(
        (prev) => prev?.map((e) => (e.id === id ? { ...e, trusted, needs_trust: !trusted } : e)) ?? prev,
      );
      try {
        await setExtensionTrusted(id, trusted);
      } catch (e) {
        setError(String(e));
        await refresh();
      }
    },
    [refresh],
  );

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <SectionHeading>Installed Extensions</SectionHeading>
        <Button variant="primary" onClick={handleInstall} disabled={installing}>
          {installing ? "Installing…" : "Install…"}
        </Button>
      </div>

      {error && (
        <div className="rounded-lg border border-rose-400/25 bg-rose-500/[0.08] px-3.5 py-2.5 text-[12.5px] text-rose-200/90">
          {error}
        </div>
      )}

      {extensions === null ? (
        <div className="py-10 text-center text-[13px] text-white/35">Loading…</div>
      ) : extensions.length === 0 ? (
        <div className="flex flex-col items-center gap-2 py-14 text-center text-white/35">
          <span className="text-[13px]">No extensions installed</span>
          <span className="text-[12px] text-white/25">
            Install a directory containing a manifest.toml to add one
          </span>
        </div>
      ) : (
        <ul className="flex flex-col gap-2.5">
          {extensions.map((ext, i) => (
            <motion.li
              key={ext.id}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: i * 0.02, duration: 0.16 }}
            >
              <Card>
                <div className="flex items-start justify-between gap-4 py-3.5">
                  <div className="flex min-w-0 flex-col gap-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-[14px] font-medium text-white/90">{ext.name}</span>
                      <span className="font-mono text-[11px] text-white/30">v{ext.version}</span>
                      {ext.needs_trust && <Pill tone="rose">Needs trust</Pill>}
                      {ext.enabled && !ext.needs_trust && <Pill tone="amber">Active</Pill>}
                    </div>
                    {ext.description && (
                      <span className="truncate text-[12.5px] text-white/45">{ext.description}</span>
                    )}
                    {ext.author && <span className="text-[11.5px] text-white/30">by {ext.author}</span>}
                    {ext.permissions.length > 0 && (
                      <ul className="mt-1 flex flex-col gap-0.5">
                        {ext.permissions.map((p) => (
                          <li key={p.permission} className="flex items-baseline gap-1.5 text-[11.5px] text-white/35">
                            <span className="font-mono text-white/50">{p.permission}</span>
                            <span>— {p.justification}</span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                  <div className="flex shrink-0 flex-col items-end gap-2">
                    <Toggle
                      checked={ext.enabled}
                      onChange={(enabled) => toggleEnabled(ext.id, enabled)}
                      disabled={ext.needs_trust}
                    />
                    {ext.needs_trust ? (
                      <Button variant="primary" onClick={() => toggleTrusted(ext.id, true)}>
                        Trust & enable
                      </Button>
                    ) : (
                      confirmUninstall === ext.id ? (
                        <div className="flex gap-1.5">
                          <Button variant="danger" onClick={() => handleUninstall(ext.id)}>
                            Confirm
                          </Button>
                          <Button variant="secondary" onClick={() => setConfirmUninstall(null)}>
                            Cancel
                          </Button>
                        </div>
                      ) : (
                        <button
                          type="button"
                          onClick={() => setConfirmUninstall(ext.id)}
                          className="text-[11.5px] text-white/30 transition-colors hover:text-rose-300"
                        >
                          Uninstall
                        </button>
                      )
                    )}
                  </div>
                </div>
              </Card>
            </motion.li>
          ))}
        </ul>
      )}
    </div>
  );
}
