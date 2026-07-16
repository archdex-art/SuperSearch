/**
 * Category vocabulary: display labels, verbs, ranking, and color identity.
 *
 * Single source of truth shared by the list (section headers, icon chips,
 * active-row accent) and the detail pane (Information block) so a result's
 * source reads consistently through color + words, not icon shape alone.
 */

/** Raw backend category → the verb shown as its primary action. */
export function actionVerb(category: string): string {
  switch (category) {
    case "Application":
    case "File":
    case "Folder":
      return "Open";
    case "Agent":
      return "Ask";
    default:
      return "Run";
  }
}

/** Raw backend category → the plural section label shown above a group. */
export function sectionLabel(category: string): string {
  switch (category) {
    case "Application": return "Applications";
    case "Command": return "Commands";
    case "Extension": return "Extensions";
    case "System": return "System";
    case "Agent": return "AI Agent";
    case "File":
    case "Folder": return "Files";
    default: return category || "Results";
  }
}

/** Detail-pane label for a result's `subtitle` field, by category. */
export function detailValueLabel(category?: string): string {
  switch (category) {
    case "Application":
    case "File":
    case "Folder":
      return "Path";
    case "Agent":
      return "Query";
    case "Extension":
      return "Source";
    default:
      return "Details";
  }
}

/** Display order for grouped sections (lower sorts first). */
export const CATEGORY_RANK: Record<string, number> = {
  Agent: 0,
  Command: 1,
  Application: 2,
  Extension: 3,
  System: 4,
  Folder: 5,
  File: 6,
};

export interface CategoryStyle {
  /** Icon chip background + ring. */
  chip: string;
  /** Accent bar / dot gradient shown on the active row and detail header. */
  bar: string;
  /** Small solid dot used next to section labels and detail rows. */
  dot: string;
}

const STYLES: Record<string, CategoryStyle> = {
  agent: {
    chip: "bg-gradient-to-br from-accent/30 to-accent/10 ring-1 ring-inset ring-accent/30",
    bar: "bg-accent",
    dot: "bg-accent",
  },
  command: {
    chip: "bg-gradient-to-br from-cyan-400/30 to-cyan-500/10 ring-1 ring-inset ring-cyan-300/25",
    bar: "bg-gradient-to-b from-cyan-300 to-cyan-500",
    dot: "bg-cyan-300",
  },
  application: {
    chip: "bg-gradient-to-br from-blue-400/30 to-indigo-500/10 ring-1 ring-inset ring-blue-300/25",
    bar: "bg-gradient-to-b from-blue-300 to-indigo-400",
    dot: "bg-blue-300",
  },
  extension: {
    chip: "bg-gradient-to-br from-rose-400/30 to-pink-500/10 ring-1 ring-inset ring-rose-300/25",
    bar: "bg-gradient-to-b from-rose-300 to-pink-400",
    dot: "bg-rose-300",
  },
  system: {
    chip: "bg-gradient-to-br from-emerald-400/30 to-teal-500/10 ring-1 ring-inset ring-emerald-300/25",
    bar: "bg-gradient-to-b from-emerald-300 to-teal-400",
    dot: "bg-emerald-300",
  },
  file: {
    chip: "bg-gradient-to-br from-slate-400/25 to-slate-500/10 ring-1 ring-inset ring-slate-300/20",
    bar: "bg-gradient-to-b from-slate-300 to-slate-400",
    dot: "bg-slate-300",
  },
};

/** Maps both raw categories ("Application") and display labels ("Applications") to a style key. */
const ALIASES: Record<string, keyof typeof STYLES> = {
  agent: "agent",
  "ai agent": "agent",
  command: "command",
  commands: "command",
  application: "application",
  applications: "application",
  extension: "extension",
  extensions: "extension",
  system: "system",
  file: "file",
  folder: "file",
  files: "file",
};

const DEFAULT_STYLE: CategoryStyle = {
  chip: "bg-white/10 ring-1 ring-inset ring-white/10",
  bar: "bg-gradient-to-b from-white/60 to-white/30",
  dot: "bg-white/50",
};

export function categoryStyle(group?: string): CategoryStyle {
  if (!group) return DEFAULT_STYLE;
  const key = ALIASES[group.trim().toLowerCase()];
  return (key && STYLES[key]) || DEFAULT_STYLE;
}
