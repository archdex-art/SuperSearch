import type { Transition, Variants } from "framer-motion";

/**
 * Motion vocabulary for the palette.
 *
 * Principles:
 * - Springs for anything the user "summons" (panel, active highlight) so it
 *   settles with natural physics instead of a fixed curve.
 * - Short tweens for *exit* — closing should feel a touch faster and more
 *   decisive than opening (Apple does this everywhere).
 * - Only `opacity` + `transform` (scale/translate) are animated → GPU
 *   compositor path, no layout thrash, easy 60fps.
 */

/** Opening spring — quick to leave 0, gentle to settle (no overshoot wobble). */
export const openSpring: Transition = {
  type: "spring",
  stiffness: 520,
  damping: 38,
  mass: 0.9,
};

/** Closing — faster, slightly eased, no spring tail. */
export const closeTween: Transition = {
  duration: 0.13,
  ease: [0.4, 0.0, 1, 1],
};

/** The gliding active-row highlight (shared layout). Snappy but smooth. */
export const highlightSpring: Transition = {
  type: "spring",
  stiffness: 700,
  damping: 46,
  mass: 0.7,
};

export const backdropVariants: Variants = {
  hidden: { opacity: 0 },
  visible: { opacity: 1, transition: { duration: 0.18, ease: "easeOut" } },
  exit: { opacity: 0, transition: { duration: 0.12, ease: "easeIn" } },
};

export const panelVariants: Variants = {
  hidden: { opacity: 0, scale: 0.96, y: 8 },
  visible: { opacity: 1, scale: 1, y: 0, transition: openSpring },
  exit: { opacity: 0, scale: 0.975, y: 4, transition: closeTween },
};

/** Container orchestrates a subtle one-time stagger of the rows on open. */
export const listVariants: Variants = {
  hidden: {},
  visible: {
    transition: { staggerChildren: 0.025, delayChildren: 0.04 },
  },
};

export const itemVariants: Variants = {
  hidden: { opacity: 0, y: 6 },
  visible: {
    opacity: 1,
    y: 0,
    transition: { type: "spring", stiffness: 600, damping: 40 },
  },
};

/** Reduced-motion: collapse everything to instant opacity, no transforms. */
export const reducedVariants = {
  backdrop: {
    hidden: { opacity: 0 },
    visible: { opacity: 1, transition: { duration: 0.001 } },
    exit: { opacity: 0, transition: { duration: 0.001 } },
  } satisfies Variants,
  panel: {
    hidden: { opacity: 0 },
    visible: { opacity: 1, transition: { duration: 0.001 } },
    exit: { opacity: 0, transition: { duration: 0.001 } },
  } satisfies Variants,
  list: { hidden: {}, visible: {} } satisfies Variants,
  item: {
    hidden: { opacity: 1 },
    visible: { opacity: 1 },
  } satisfies Variants,
  highlight: { duration: 0 } as Transition,
};
