import { memo } from "react";
import { motion, type Transition, type Variants } from "framer-motion";
import type { CommandAction } from "./types";

interface CommandItemProps {
  action: CommandAction;
  active: boolean;
  domId: string;
  itemVariants: Variants;
  highlightTransition: Transition;
  /** Hover drives selection (Spotlight/Raycast behavior) → highlight glides. */
  onActivate: () => void;
  onSelect: () => void;
}

/**
 * A single result row.
 *
 * The selection background is NOT a per-row class toggle — it's a single shared
 * element (`layoutId="cmd-highlight"`) that lives inside whichever row is active.
 * Framer Motion's shared-layout engine then *glides* it from the old row to the
 * new one with `highlightSpring`. That's the entire trick behind the smooth
 * "active item slides between selections" feel, and it stays on the GPU
 * transform path (no top/left animation, no reflow).
 */
function CommandItemBase({
  action,
  active,
  domId,
  itemVariants,
  highlightTransition,
  onActivate,
  onSelect,
}: CommandItemProps) {
  return (
    <motion.li
      id={domId}
      role="option"
      aria-selected={active}
      variants={itemVariants}
      onPointerMove={onActivate}
      onPointerDown={(e) => e.preventDefault() /* keep input focus */}
      onClick={onSelect}
      className="relative flex h-[52px] cursor-default select-none items-center gap-3 rounded-xl px-3"
    >
      {active && (
        <motion.div
          layoutId="cmd-highlight"
          transition={highlightTransition}
          className="absolute inset-0 rounded-xl bg-white/[0.10] ring-1 ring-inset ring-white/[0.08] shadow-[0_1px_0_0_rgba(255,255,255,0.06)_inset]"
        />
      )}

      <span className="relative z-10 flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-lg text-xl">
        {typeof action.icon === "string" && /^https?:|^data:|\//.test(action.icon) ? (
          <img src={action.icon} alt="" className="h-full w-full object-cover" />
        ) : (
          action.icon ?? "•"
        )}
      </span>

      <span className="relative z-10 flex min-w-0 flex-col">
        <span className="truncate text-[15px] font-medium leading-tight text-white/95">
          {action.title}
        </span>
        {action.subtitle && (
          <span className="truncate text-[12.5px] leading-tight text-white/45">
            {action.subtitle}
          </span>
        )}
      </span>

      {/* Right-side action hint, only on the active row (Spotlight-style). */}
      <span className="relative z-10 ml-auto flex shrink-0 items-center gap-2 pl-3">
        <motion.span
          initial={false}
          animate={{ opacity: active ? 1 : 0, x: active ? 0 : 4 }}
          transition={{ duration: 0.14, ease: "easeOut" }}
          className="flex items-center gap-2 text-[13px] text-white/55"
        >
          {action.hint && <span>{action.hint}</span>}
          <kbd className="rounded-md border border-white/15 bg-white/10 px-1.5 py-0.5 text-[11px] text-white/80">
            ↵
          </kbd>
        </motion.span>
      </span>
    </motion.li>
  );
}

/** Memoized: only re-renders when its own `active`/`action` actually change. */
export const CommandItem = memo(CommandItemBase, (a, b) => {
  return (
    a.active === b.active &&
    a.action === b.action &&
    a.domId === b.domId &&
    a.itemVariants === b.itemVariants
  );
});
