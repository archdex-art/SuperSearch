import { useState } from "react";
import { HexColorPicker } from "react-colorful";
import type { Settings } from "../types";
import { Card, SectionHeading } from "../ui";

/** Built-in accent presets. "amber" is the palette's default identity —
 *  listed first and treated as "no override" (omits `accent_color`). */
const PRESETS: { id: string; label: string; hex: string }[] = [
  { id: "amber", label: "Amber", hex: "#f5a623" },
  { id: "cyan", label: "Cyan", hex: "#22d3ee" },
  { id: "rose", label: "Rose", hex: "#fb7185" },
  { id: "emerald", label: "Emerald", hex: "#34d399" },
  { id: "violet", label: "Violet", hex: "#a78bfa" },
];

const DEFAULT_ACCENT = "#f5a623";

export function AppearancePane({
  settings,
  onChange,
}: {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => void;
}) {
  const current = settings.accent_color ?? DEFAULT_ACCENT;
  const [customOpen, setCustomOpen] = useState(false);

  return (
    <div className="flex flex-col gap-6">
      <div>
        <SectionHeading>Accent Color</SectionHeading>
        <Card>
          <div className="flex flex-wrap items-center gap-3 py-4">
            {PRESETS.map((p) => (
              <button
                key={p.id}
                type="button"
                onClick={() => onChange({ accent_color: p.id === "amber" ? null : p.hex })}
                aria-label={p.label}
                title={p.label}
                className={`relative h-8 w-8 shrink-0 rounded-full ring-2 ring-offset-2 ring-offset-[hsl(32,14%,7%)] transition-transform hover:scale-110 ${
                  current.toLowerCase() === p.hex.toLowerCase() ? "ring-white/70" : "ring-transparent"
                }`}
                style={{ backgroundColor: p.hex }}
              />
            ))}

            {/* Custom swatch — shows the live custom color when one is picked
                outside the presets, otherwise a neutral "+" affordance. */}
            <button
              type="button"
              onClick={() => setCustomOpen((o) => !o)}
              aria-label="Custom color"
              title="Custom color"
              className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-[15px] leading-none ring-2 ring-offset-2 ring-offset-[hsl(32,14%,7%)] transition-transform hover:scale-110 ${
                customOpen ? "ring-white/70" : "ring-white/[0.15]"
              }`}
              style={
                !PRESETS.some((p) => p.hex.toLowerCase() === current.toLowerCase())
                  ? { backgroundColor: current }
                  : { background: "conic-gradient(from 0deg, #f5a623, #22d3ee, #fb7185, #a78bfa, #f5a623)" }
              }
            >
              {PRESETS.some((p) => p.hex.toLowerCase() === current.toLowerCase()) ? "+" : ""}
            </button>
          </div>

          {customOpen && (
            <div className="flex flex-col items-center gap-3 pb-4">
              <HexColorPicker
                color={current}
                onChange={(hex) => onChange({ accent_color: hex })}
                style={{ width: "100%", height: 140 }}
              />
              <span className="font-mono text-[11px] uppercase tracking-wide text-white/40">{current}</span>
            </div>
          )}
        </Card>
      </div>

      <div>
        <SectionHeading>Preview</SectionHeading>
        <Card>
          <div className="flex items-center gap-3 py-4">
            <span
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[9px] text-lg ring-1 ring-inset"
              style={{ backgroundColor: `${current}26`, boxShadow: `inset 0 0 0 1px ${current}40` }}
            >
              🎵
            </span>
            <div className="flex min-w-0 flex-col">
              <span className="truncate text-[14px] font-medium text-white/90">Apple Music</span>
              <span className="truncate text-[12px] text-white/40">/Applications/Apple Music.app</span>
            </div>
            <kbd
              className="ml-auto rounded-md border px-1.5 py-0.5 font-mono text-[11px]"
              style={{ borderColor: `${current}40`, backgroundColor: `${current}1a`, color: current }}
            >
              ↵
            </kbd>
          </div>
        </Card>
      </div>
    </div>
  );
}
