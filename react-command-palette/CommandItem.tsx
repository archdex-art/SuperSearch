import { memo } from "react";
import { motion, type Transition, type Variants } from "framer-motion";
import type { CommandAction } from "./types";
import { categoryStyle } from "./categories";

interface CommandItemProps {
  action: CommandAction;
  active: boolean;
  domId: string;
  itemVariants: Variants;
  highlightTransition: Transition;
  /** Hover drives selection (Spotlight/Raycast behavior) → highlight glides. */
  onActivate: () => void;
  onSelect: () => void;
  /** Tighter row for narrow master-detail layouts; omits the right-side hint text. */
  compact?: boolean;
}

/**
 * A single result row.
 *
 * The selection background is NOT a per-row class toggle — it's a single shared
 * element (`layoutId="cmd-highlight"`) that lives inside whichever row is active.
 * Framer Motion's shared-layout engine then *glides* it from the old row to the
 * new one with `highlightSpring`. That's the entire trick behind the smooth
 * "active item slides between selections" feel, and it stays on the GPU
 * transform path (no top/left animation, no reflow). A slim category-colored
 * bar rides inside the same glide, so the accent tracks the highlight for free.
 */
function CommandItemBase({
  action,
  active,
  domId,
  itemVariants,
  highlightTransition,
  onActivate,
  onSelect,
  compact = false,
}: CommandItemProps) {
  const style = categoryStyle(action.group);
  const isImg = typeof action.icon === "string" && /^https?:|^data:|\//.test(action.icon);

  return (
    <motion.li
      id={domId}
      role="option"
      aria-selected={active}
      variants={itemVariants}
      onPointerMove={onActivate}
      onPointerDown={(e) => e.preventDefault() /* keep input focus */}
      onClick={onSelect}
      className={`relative flex cursor-default select-none items-center gap-2.5 rounded-xl px-2.5 ${
        compact ? "h-[42px]" : "h-[52px] gap-3 px-3"
      }`}
    >
      {active && (
        <motion.div
          layoutId="cmd-highlight"
          transition={highlightTransition}
          className="absolute inset-0 overflow-hidden rounded-xl bg-gradient-to-r from-accent/[0.08] via-white/[0.05] to-transparent
                     ring-1 ring-inset ring-accent/[0.14] shadow-[0_1px_0_0_rgba(255,255,255,0.06)_inset]"
        >
          <span className={`absolute inset-y-2 left-0 w-[3px] rounded-full ${style.bar}`} />
        </motion.div>
      )}

      <span
        className={`relative z-10 flex shrink-0 items-center justify-center overflow-hidden rounded-[9px] ${style.chip} ${
          compact ? "h-6 w-6 text-[14px]" : "h-8 w-8 text-xl"
        }`}
      >
        {isImg ? (
          <img src={action.icon as string} alt="" className="h-full w-full object-cover" />
        ) : (
          (action.icon ?? "•")
        )}
      </span>

      <span className="relative z-10 flex min-w-0 flex-col">
        <span
          className={`truncate font-medium leading-tight text-white/95 ${compact ? "text-[13.5px]" : "text-[15px]"}`}
        >
          {action.title}
        </span>
        {action.subtitle && !compact && (
          <span className="truncate text-[12.5px] leading-tight text-white/45">{action.subtitle}</span>
        )}
      </span>

      {/* Right-side action hint, only on the active row (Spotlight-style). */}
      <span className="relative z-10 ml-auto flex shrink-0 items-center gap-2 pl-2">
        <motion.span
          initial={false}
          animate={{ opacity: active ? 1 : 0, x: active ? 0 : 4 }}
          transition={{ duration: 0.14, ease: "easeOut" }}
          className="flex items-center gap-2 text-[13px] text-white/55"
        >
          {action.hint && !compact && <span>{action.hint}</span>}
          <kbd className="rounded-md border border-accent/25 bg-accent/10 px-1.5 py-0.5 font-mono text-[11px] text-accent/90">
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
    a.itemVariants === b.itemVariants &&
    a.compact === b.compact
  );
});
