import { useCallback, useEffect, useRef, useState } from "react";
import type { Settings } from "../types";
import { resumeToggleShortcut, suspendToggleShortcut, validateShortcut } from "../ipc";
import { Card, Row, SectionHeading, Toggle } from "../ui";

/** Modifier key labels shown while capturing, in a stable display order. */
const MOD_KEYS: Record<string, string> = {
  MetaLeft: "Super", MetaRight: "Super",
  AltLeft: "Alt", AltRight: "Alt",
  ControlLeft: "Control", ControlRight: "Control",
  ShiftLeft: "Shift", ShiftRight: "Shift",
};

/** Converts a captured KeyboardEvent into Tauri accelerator syntax, e.g.
 *  "Alt+Space", "Control+Shift+K". Returns null for a bare modifier press
 *  (nothing to bind yet) or an unsupported key. */
function toAccelerator(e: KeyboardEvent): string | null {
  if (e.code in MOD_KEYS) return null;
  const mods: string[] = [];
  if (e.metaKey) mods.push("Super");
  if (e.ctrlKey) mods.push("Control");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (mods.length === 0) return null; // require at least one modifier
  const key = codeToAcceleratorKey(e.code, e.key);
  if (key == null) return null;
  return [...mods, key].join("+");
}

/** Resolve the pressed key to Tauri accelerator syntax from `code` (the
 *  *physical* key) wherever possible, not `key` (the *composed* character).
 *
 *  macOS recomposes many keys under Option/Alt into a different Unicode
 *  character than what's physically printed on the keycap — Option+Space's
 *  `key` is a non-breaking space (U+00A0), not `" "`; Option+letters compose
 *  accented characters ("Option+C" → "ç"). Since Alt is one of the four
 *  modifiers this exact capture UI expects users to hold, keying off `key`
 *  silently persisted a corrupted accelerator like `"Alt+\u00A0"` straight
 *  to `settings.json` — invisible in the UI (looked like a normal capture),
 *  but rejected outright by the OS accelerator parser at registration time
 *  ("Found empty token while parsing hotkey"), leaving the hotkey broken
 *  until the boot-time self-heal fell back to the default. `code` names the
 *  physical key regardless of what a modifier composes it into. */
function codeToAcceleratorKey(code: string, fallbackKey: string): string | null {
  if (code === "Space") return "Space";
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  // Unrecognized physical key (arrows, Escape, function keys, punctuation,
  // …) — `key` is stable for these regardless of modifiers, since Option
  // only recomposes *printable* characters.
  if (fallbackKey.length === 1) return fallbackKey.toUpperCase();
  if (/^[A-Za-z0-9]/.test(fallbackKey)) return fallbackKey;
  return null; // Tab, Escape's siblings that aren't bindable, etc.
}

export function GeneralPane({
  settings,
  onChange,
}: {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => void;
}) {
  const [capturing, setCapturing] = useState(false);
  const [check, setCheck] = useState<{ ok: boolean; reason?: string } | null>(null);
  const [checking, setChecking] = useState(false);
  const captureRef = useRef<HTMLButtonElement>(null);

  // Suspend the OS-level global hotkey for the duration of a capture
  // session — otherwise pressing the combo currently bound (or any combo
  // macOS/Tauri already intercepts) never reaches this keydown listener,
  // and "Listening…" looks permanently stuck instead of capturing.
  useEffect(() => {
    if (!capturing) return;
    let live = true;
    void suspendToggleShortcut();
    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.key === "Escape") {
        setCapturing(false);
        return;
      }
      const accel = toAccelerator(e);
      if (!accel) return; // still waiting on a real key, not just a modifier
      setCapturing(false);
      setChecking(true);
      validateShortcut(accel)
        .then((result) => {
          setCheck(result);
          if (result.ok) onChange({ toggle_shortcut: accel });
        })
        .finally(() => setChecking(false));
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      // Re-arm immediately on cancel/unmount; on a successful capture the
      // pending `update_settings` call re-registers the *new* shortcut, so
      // resuming the old one here would just be clobbered a moment later.
      if (live) void resumeToggleShortcut();
      live = false;
    };
  }, [capturing, onChange]);

  const startCapture = useCallback(() => {
    setCheck(null);
    setCapturing(true);
    captureRef.current?.focus();
  }, []);

  return (
    <div className="flex flex-col gap-6">
      <div>
        <SectionHeading>Global Hotkey</SectionHeading>
        <Card>
          <Row>
            <span className="flex flex-col gap-0.5">
              <span className="text-[13.5px] font-medium text-ink/90">Summon SuperSearch</span>
              <span className="text-[12px] text-ink/40">
                {capturing ? "Press a key combination… (Esc to cancel)" : "Click, then press your new shortcut"}
              </span>
            </span>
            <button
              ref={captureRef}
              type="button"
              onClick={startCapture}
              className={`min-w-[140px] rounded-lg border px-3 py-1.5 text-center font-mono text-[12.5px] transition-colors ${
                capturing
                  ? "border-accent/50 bg-accent/[0.12] text-accent"
                  : "border-ink/[0.1] bg-ink/[0.05] text-ink/80 hover:bg-ink/[0.08]"
              }`}
            >
              {capturing ? "Listening…" : settings.toggle_shortcut}
            </button>
          </Row>
          <div aria-live="polite">
            {checking && <div className="pb-3 text-[12px] text-ink/40">Checking…</div>}
            {check && !check.ok && (
              <div className="flex items-start gap-2 pb-3 text-[12px] leading-snug text-rose-300/90">
                <span className="mt-[3px] h-1.5 w-1.5 shrink-0 rounded-full bg-rose-400" />
                {check.reason ?? "That combination can't be used."}
              </div>
            )}
          </div>
        </Card>
      </div>

      <div>
        <SectionHeading>Behavior</SectionHeading>
        <Card>
          <Toggle
            checked={settings.hide_on_blur}
            onChange={(hide_on_blur) => onChange({ hide_on_blur })}
            label="Hide when SuperSearch loses focus"
            description="Spotlight-style — dismiss automatically when you click another app"
          />
        </Card>
      </div>
    </div>
  );
}
