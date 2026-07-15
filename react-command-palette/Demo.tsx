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
    <div className="grid min-h-screen place-items-center bg-gradient-to-br from-[#0b0a17] via-[#160f2e] to-[#1a1220] text-white">
      <button
        onClick={() => setOpen(true)}
        className="rounded-xl border border-white/15 bg-white/10 px-5 py-2.5 text-sm font-medium backdrop-blur transition-transform active:scale-95"
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
