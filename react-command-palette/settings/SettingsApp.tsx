import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { listen } from "../bridge";
import { applyAccent, applyTheme } from "../theme";
import type { Settings } from "./types";
import { getSettings, updateSettings } from "./ipc";
import { GeneralPane } from "./panes/General";
import { AppearancePane } from "./panes/Appearance";
import { ExtensionsPane } from "./panes/Extensions";
import { AboutPane } from "./panes/About";

type Section = "general" | "appearance" | "extensions" | "about";

const SECTIONS: { id: Section; label: string; icon: string }[] = [
  { id: "general", label: "General", icon: "M4 6h12M4 10h12M4 14h8" },
  { id: "appearance", label: "Appearance", icon: "M10 3a7 7 0 1 0 0 14 1.6 1.6 0 0 0 0-3.2h-.5a1.3 1.3 0 0 1 0-2.6H11a3 3 0 0 0 0-6h-1Z" },
  { id: "extensions", label: "Extensions", icon: "M6 3h3v3h2V3h3v3h2v3h-3v2h3v3h-2v3h-3v-3H8v3H6v-3H3v-3h3v-2H3V6h3V3Z" },
  { id: "about", label: "About", icon: "M10 17a7 7 0 1 0 0-14 7 7 0 0 0 0 14ZM10 9v5M10 6.5v.01" },
];

export function SettingsApp() {
  const [section, setSection] = useState<Section>("general");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void getSettings().then((s) => {
      setSettings(s);
      applyAccent(s.accent_color);
      applyTheme(s.theme);
    });
    let un: undefined | (() => void);
    let cancelled = false;
    listen<Settings>("supersearch://settings-changed", (s) => {
      setSettings(s);
      applyAccent(s.accent_color);
      applyTheme(s.theme);
    }).then((fn) => {
      if (cancelled) fn();
      else un = fn;
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);

  const patchSettings = useCallback((patch: Partial<Settings>) => {
    setSettings((prev) => {
      if (!prev) return prev;
      const next = { ...prev, ...patch };
      if ("accent_color" in patch) applyAccent(next.accent_color);
      if ("theme" in patch) applyTheme(next.theme);
      setSaving(true);
      void updateSettings(next).finally(() => setSaving(false));
      return next;
    });
  }, []);

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-canvas text-ink">
      {/* Sidebar */}
      <nav aria-label="Settings sections" className="flex w-[196px] shrink-0 flex-col gap-0.5 border-r border-ink/[0.06] bg-ink/[0.04] px-3 py-4">
        <div className="mb-3 flex items-center gap-2 px-2">
          <span className="relative flex h-5 w-5 shrink-0 items-center justify-center rounded-[5px] border border-accent/40 bg-accent/[0.07]">
            <span className="h-[5px] w-[5px] rounded-full bg-accent" />
          </span>
          <span className="font-mono text-[11px] font-semibold uppercase tracking-[0.12em] text-ink/70">
            Settings
          </span>
        </div>

        {SECTIONS.map((s) => (
          <button
            key={s.id}
            type="button"
            onClick={() => setSection(s.id)}
            aria-current={section === s.id ? "page" : undefined}
            className={`relative flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-[13px] font-medium transition-colors ${
              section === s.id ? "text-ink/95" : "text-ink/50 hover:bg-ink/[0.04] hover:text-ink/80"
            }`}
          >
            {section === s.id && (
              <motion.span
                layoutId="settings-nav-active"
                transition={{ type: "spring", stiffness: 500, damping: 40 }}
                className="absolute inset-0 rounded-lg bg-accent/[0.1] ring-1 ring-inset ring-accent/[0.16]"
              />
            )}
            <svg viewBox="0 0 20 20" className="relative z-10 h-4 w-4 shrink-0" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round">
              <path d={s.icon} />
            </svg>
            <span className="relative z-10">{s.label}</span>
          </button>
        ))}

        <div aria-live="polite" className="mt-auto flex items-center gap-1.5 px-2 pt-3 text-[11px] text-ink/25">
          <span className={`h-1.5 w-1.5 rounded-full transition-colors ${saving ? "bg-accent" : "bg-ink/15"}`} />
          {saving ? "Saving…" : "Saved"}
        </div>
      </nav>

      {/* Content */}
      <main aria-label="Settings content" className="flex-1 overflow-y-auto px-8 py-7">
        <div className="mx-auto max-w-[540px]">
          <h1 className="mb-5 text-[18px] font-semibold text-ink/95">
            {SECTIONS.find((s) => s.id === section)?.label}
          </h1>
          {!settings ? (
            <div className="py-10 text-center text-[13px] text-ink/35">Loading…</div>
          ) : (
            <motion.div
              key={section}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.15 }}
            >
              {section === "general" && <GeneralPane settings={settings} onChange={patchSettings} />}
              {section === "appearance" && <AppearancePane settings={settings} onChange={patchSettings} />}
              {section === "extensions" && <ExtensionsPane />}
              {section === "about" && <AboutPane />}
            </motion.div>
          )}
        </div>
      </main>
    </div>
  );
}
