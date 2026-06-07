# Command Palette — premium Spotlight-grade motion

React + TypeScript + TailwindCSS + Framer Motion. A calm, frictionless,
native-feeling command palette inspired by macOS Spotlight, Raycast, and Linear.

```
react-command-palette/
  CommandPalette.tsx   # orchestrator: state, filtering, keyboard, AnimatePresence
  CommandItem.tsx      # one row + the shared-layout active highlight
  variants.ts          # the entire motion vocabulary (reusable)
  useGlobalHotkey.ts   # ⌘K / Ctrl+K
  types.ts · index.ts · Demo.tsx
```

## Install & use
```bash
npm i framer-motion
```
```tsx
import { CommandPalette, type CommandAction } from "./react-command-palette";

const actions: CommandAction[] = [
  { id: "music", title: "Apple Music", group: "Applications", icon: "🎵", hint: "Open", perform: () => {} },
];

<CommandPalette open={open} onOpenChange={setOpen} actions={actions} />
```
Tailwind: ensure the folder is in your `content` globs. Requires the
`backdrop-blur` utilities (default in Tailwind).

---

## Motion architecture

### 1. Layered AnimatePresence
The overlay mounts/unmounts through a single `<AnimatePresence>`. Three layers
animate **independently** so each reads naturally:

| Layer | Property | Why |
|-------|----------|-----|
| backdrop | `opacity` only | fading a *pre-blurred* layer means the blur never "pops" |
| panel | `opacity + scale + y` | the summon gesture |
| rows | `opacity + y`, staggered | content arriving *after* the container |

Exit uses a shorter tween than enter — closing should feel a touch faster and
more decisive (an Apple signature). AnimatePresence keeps exits interruptible:
reopen mid-close and it springs back from wherever it is.

### 2. Open = spring, Close = tween
- **Open** (`openSpring`, stiffness 520 / damping 38 / mass 0.9): leaves the
  initial state immediately, settles with no visible wobble — "calm, not bouncy."
- **Close** (`closeTween`, 0.13s, accelerating ease): quick and out of the way.

Initial → animate is exactly the spec: `{opacity:0, scale:0.96, y:8} → {1, 1, 0}`.

### 3. The gliding active highlight (the headline trick)
The selection background is **not** a per-row class. It's one element with
`layoutId="cmd-highlight"` rendered *inside whichever row is active*. When the
active index changes, Framer's shared-layout engine animates that element from
the old row's box to the new one with `highlightSpring` — a true glide, on the
GPU transform path, no `top`/`left` animation, no reflow. `<LayoutGroup>` scopes
the measurement to the list.

Hover and keyboard both write the same `activeIndex`, so mousing over a row makes
the highlight glide to it (Raycast/Spotlight behavior) — one source of truth,
one animation.

### 4. Subtle, one-time stagger
`listVariants` orchestrates `staggerChildren: 0.025` once when the panel opens.
Rows use a stiff spring (`itemVariants`) so they arrive crisply, not floatily.
We deliberately **don't** re-stagger on every keystroke — that would flicker;
instead the list re-renders cheaply and the highlight glides.

### 5. Microinteractions
- Rows select on `pointermove` (magnetic-feeling) and call `preventDefault` on
  `pointerdown` to **keep the input focused** (so typing never breaks).
- The right-side hint (`Open ↵`) cross-fades + slides in only on the active row.
- Buttons use Tailwind `active:scale-95` for a tactile pressed state.

## Framer Motion configuration choices
- **Springs over easing** for summon + selection; **tweens** for dismissals.
- `useReducedMotion()` swaps in `reducedVariants` → instant opacity, zero
  transforms, `highlight` transition duration 0. Fully honored, no media-query
  duplication.
- `AnimatePresence` only at the top level; nested state changes use `layout`/
  `layoutId` rather than mount/unmount churn.
- Motion is confined to `opacity` + `transform`; `willChange` is hinted on the
  panel. No animated `width`/`height`/`filter` interpolation.

## Performance
- `CommandItem` is `React.memo`'d with a custom comparator — only the two rows
  whose `active` flips actually re-render on navigation.
- Filtering/grouping is `useMemo`'d; handlers are `useCallback`'d to keep
  children stable.
- 60fps: compositor-only animations, `overscroll-contain` for natural scroll
  momentum without rubber-banding the page.

## Accessibility
- `role="dialog"` (modal) › `role="combobox"` input with `aria-controls` /
  `aria-activedescendant` › `role="listbox"` / `role="option"`.
- Focus moves to the input on open (after a `requestAnimationFrame` so the caret
  appears instantly), and is **restored** to the previously-focused element on
  close.
- Full keyboard model: ↑/↓ (wrapping), Home/End, ↵, Esc.
- `prefers-reduced-motion` collapses all motion to instant.
