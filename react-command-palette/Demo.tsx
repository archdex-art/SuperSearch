import { useCallback, useState } from "react";
import { CommandPalette } from "./CommandPalette";
import { useGlobalHotkey } from "./useGlobalHotkey";
import type { CommandAction } from "./types";

const ACTIONS: CommandAction[] = [
  { id: "music", title: "Apple Music", group: "Applications", icon: "🎵", hint: "Open", keywords: "songs audio" },
  { id: "vscode", title: "Visual Studio Code", subtitle: "/Applications/Visual Studio Code.app", group: "Applications", icon: "🧩", hint: "Open" },
  { id: "slack", title: "Slack", group: "Applications", icon: "💬", hint: "Open" },
  { id: "figma", title: "Figma", group: "Applications", icon: "🎨", hint: "Open" },
  { id: "lock", title: "Lock Screen", group: "Commands", icon: "🔒", hint: "Run", keywords: "secure" },
  { id: "dnd", title: "Toggle Do Not Disturb", group: "Commands", icon: "🔕", hint: "Run" },
  { id: "dark", title: "Toggle Dark Mode", group: "Commands", icon: "🌗", hint: "Run", keywords: "theme appearance" },
  { id: "shot", title: "Screenshot", group: "Commands", icon: "📸", hint: "Run", keywords: "capture" },
];

export default function Demo() {
  const [open, setOpen] = useState(false);
  useGlobalHotkey(useCallback(() => setOpen((o) => !o), []));

  return (
    <div className="grid min-h-screen place-items-center bg-gradient-to-br from-[#08090a] via-[#0f0b06] to-[#120d08] font-sans text-white">
      <button
        onClick={() => setOpen(true)}
        className="rounded-xl border border-amber-300/20 bg-white/[0.05] px-5 py-2.5 font-mono text-sm font-medium tracking-wide backdrop-blur transition-transform active:scale-95"
      >
        Press ⌘K
      </button>

      <CommandPalette
        open={open}
        onOpenChange={setOpen}
        actions={ACTIONS}
        onSelect={(a) => console.log("selected:", a.id)}
      />
    </div>
  );
}
