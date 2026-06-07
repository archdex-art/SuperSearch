import type { ReactNode } from "react";

/** A single actionable row in the palette. */
export interface CommandAction {
  id: string;
  title: string;
  subtitle?: string;
  /** Emoji, image URL string, or any React node for the leading icon. */
  icon?: ReactNode;
  /** Section label, e.g. "Applications", "Commands". */
  group?: string;
  /** Right-side hint shown only on the active row, e.g. "Open". */
  hint?: string;
  /** Extra terms to widen fuzzy matching (not displayed). */
  keywords?: string;
  /** Invoked on Enter / click. */
  perform?: () => void;
}

export interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  actions: CommandAction[];
  placeholder?: string;
  /** Called with the chosen action; falls back to `action.perform()`. */
  onSelect?: (action: CommandAction) => void;
  /** Optional empty-state node. */
  emptyState?: ReactNode;
}
