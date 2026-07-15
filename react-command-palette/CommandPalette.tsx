import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  AnimatePresence,
  LayoutGroup,
  motion,
  useReducedMotion,
} from "framer-motion";
import type { CommandAction, CommandPaletteProps } from "./types";
import { CommandItem } from "./CommandItem";
import { categoryStyle } from "./categories";
import {
  backdropVariants,
  highlightSpring,
  itemVariants,
  listVariants,
  panelVariants,
  reducedVariants,
} from "./variants";

/** Cheap, allocation-light fuzzy filter (subsequence match + scoring). */
function score(action: CommandAction, q: string): number {
  if (!q) return 1;
  const hay = `${action.title} ${action.subtitle ?? ""} ${action.keywords ?? ""}`.toLowerCase();
  const needle = q.toLowerCase();
  if (hay.includes(needle)) return action.title.toLowerCase().startsWith(needle) ? 3 : 2;
  // subsequence fallback
  let i = 0;
  for (const ch of hay) if (ch === needle[i]) i++;
  return i === needle.length ? 1 : 0;
}

export function CommandPalette({
  open,
  onOpenChange,
  actions,
  placeholder = "Search for apps and commands…",
  onSelect,
  emptyState,
}: CommandPaletteProps) {
  const prefersReduced = useReducedMotion();
  // Memoized so child props (itemVariants/highlight) stay referentially stable
  // → CommandItem's memo actually holds; only the rows whose `active` flips
  // re-render on navigation.
  const v = useMemo(
    () =>
      prefersReduced
        ? reducedVariants
        : {
            backdrop: backdropVariants,
            panel: panelVariants,
            list: listVariants,
            item: itemVariants,
            highlight: highlightSpring,
          },
    [prefersReduced],
  );

  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const restoreFocusRef = useRef<HTMLElement | null>(null);
  const baseId = useId();

  // Filter + rank, then keep stable section grouping.
  const results = useMemo(() => {
    const scored = actions
      .map((a) => ({ a, s: score(a, query) }))
      .filter((x) => x.s > 0)
      .sort((x, y) => y.s - x.s);
    return scored.map((x) => x.a);
  }, [actions, query]);

  const groups = useMemo(() => {
    const map = new Map<string, CommandAction[]>();
    for (const a of results) {
      const key = a.group ?? "";
      (map.get(key) ?? map.set(key, []).get(key)!).push(a);
    }
    // Flatten back with section boundaries, assigning each row its GLOBAL index
    // in display (grouped) order.
    let index = 0;
    return [...map.entries()].map(([label, items]) => ({
      label,
      items: items.map((a) => ({ action: a, index: index++ })),
    }));
  }, [results]);

  // Canonical display order — keyboard selection & Enter use THIS, so the
  // highlighted row and the executed action can never diverge.
  const ordered = useMemo(
    () => groups.flatMap((g) => g.items.map((it) => it.action)),
    [groups],
  );

  // Clamp/reset selection whenever the result set changes.
  useEffect(() => {
    setActiveIndex((i) => (ordered.length === 0 ? 0 : Math.min(i, ordered.length - 1)));
  }, [ordered.length]);

  // Focus management: focus input on open, restore focus on close.
  useEffect(() => {
    if (open) {
      restoreFocusRef.current = document.activeElement as HTMLElement;
      // rAF so the element is mounted/visible before focusing (smooth caret).
      const raf = requestAnimationFrame(() => inputRef.current?.focus());
      return () => cancelAnimationFrame(raf);
    }
    setQuery("");
    setActiveIndex(0);
    restoreFocusRef.current?.focus?.();
  }, [open]);

  const choose = useCallback(
    (action: CommandAction | undefined) => {
      if (!action) return;
      onSelect ? onSelect(action) : action.perform?.();
      onOpenChange(false);
    },
    [onSelect, onOpenChange],
  );

  const scrollActiveIntoView = useCallback((idx: number) => {
    const el = listRef.current?.querySelector<HTMLElement>(`[data-idx="${idx}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, []);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setActiveIndex((i) => {
            const n = ordered.length ? (i + 1) % ordered.length : 0;
            scrollActiveIntoView(n);
            return n;
          });
          break;
        case "ArrowUp":
          e.preventDefault();
          setActiveIndex((i) => {
            const n = ordered.length ? (i - 1 + ordered.length) % ordered.length : 0;
            scrollActiveIntoView(n);
            return n;
          });
          break;
        case "Home":
          e.preventDefault();
          setActiveIndex(0);
          scrollActiveIntoView(0);
          break;
        case "End":
          e.preventDefault();
          setActiveIndex(ordered.length - 1);
          scrollActiveIntoView(ordered.length - 1);
          break;
        case "Enter":
          e.preventDefault();
          choose(ordered[activeIndex]);
          break;
        case "Escape":
          e.preventDefault();
          onOpenChange(false);
          break;
      }
    },
    [ordered, activeIndex, choose, onOpenChange, scrollActiveIntoView],
  );

  const activeDomId = `${baseId}-opt-${activeIndex}`;

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className="fixed inset-0 z-[1000] flex items-start justify-center px-4 pt-[14vh]"
          initial="hidden"
          animate="visible"
          exit="exit"
        >
          {/* Backdrop — fades a *pre-blurred* layer so the blur never pops. */}
          <motion.div
            variants={v.backdrop}
            onClick={() => onOpenChange(false)}
            className="absolute inset-0 bg-black/40 backdrop-blur-xl"
            aria-hidden
          />

          {/* Panel */}
          <motion.div
            variants={v.panel}
            style={{ willChange: "transform, opacity" }}
            className="relative w-full max-w-[640px]"
          >
            <div className="aurora-frame relative rounded-[19px] p-[1.5px] shadow-[0_32px_80px_-16px_rgba(0,0,0,0.6)]">
              <div
                role="dialog"
                aria-modal="true"
                aria-label="Command palette"
                onKeyDown={onKeyDown}
                className="relative overflow-hidden rounded-[17.5px] border border-white/[0.05]
                           bg-[hsla(255,20%,10%,0.78)] ring-1 ring-inset ring-white/[0.05] backdrop-blur-2xl"
              >
                <div
                  className="pointer-events-none absolute inset-0"
                  style={{
                    background:
                      "radial-gradient(120% 90% at 0% 0%, rgba(139,92,246,0.14), transparent 55%), " +
                      "radial-gradient(120% 90% at 100% 100%, rgba(245,166,35,0.09), transparent 55%)",
                  }}
                />

                {/* Search input */}
                <div className="relative z-10 flex items-center gap-3 px-4 h-[58px] border-b border-white/[0.06]">
                  <SearchIcon active={query.length > 0} />
                  <input
                    ref={inputRef}
                    value={query}
                    onChange={(e) => {
                      setQuery(e.target.value);
                      setActiveIndex(0);
                    }}
                    placeholder={placeholder}
                    role="combobox"
                    aria-expanded
                    aria-controls={`${baseId}-listbox`}
                    aria-activedescendant={ordered.length ? activeDomId : undefined}
                    autoComplete="off"
                    autoCorrect="off"
                    spellCheck={false}
                    className="h-full flex-1 bg-transparent text-[18px] font-normal text-white
                               caret-violet-400 outline-none placeholder:text-white/35"
                  />
                </div>

                {/* Results */}
                <LayoutGroup>
                  <motion.ul
                    ref={listRef}
                    id={`${baseId}-listbox`}
                    role="listbox"
                    aria-label="Results"
                    variants={v.list}
                    className="relative z-10 max-h-[min(52vh,420px)] overflow-y-auto overscroll-contain p-2
                               [scrollbar-width:thin]"
                  >
                    {ordered.length === 0 ? (
                      <li className="flex flex-col items-center gap-3 px-4 py-14 text-center text-white/40">
                        {emptyState ?? (
                          <>
                            <span className="relative flex h-10 w-10 items-center justify-center">
                              <span className="absolute inset-0 animate-pulse rounded-full bg-gradient-to-br from-violet-500/25 to-amber-400/15 blur-md" />
                              <span className="relative h-2.5 w-2.5 rounded-full bg-gradient-to-br from-violet-300 to-amber-200" />
                              <span className="absolute h-6 w-6 rounded-full border border-white/15" />
                            </span>
                            <span className="text-[13px]">No results</span>
                          </>
                        )}
                      </li>
                    ) : (
                      groups.map((group) => (
                        <li key={group.label || "_"} role="presentation">
                          {group.label && (
                            <div className="flex items-center gap-1.5 px-3 pb-1 pt-3 text-[11px] font-semibold uppercase tracking-wide text-white/40">
                              <span className={`h-1 w-1 rounded-full ${categoryStyle(group.items[0]?.action.group).dot}`} />
                              {group.label}
                            </div>
                          )}
                          <ul role="presentation" className="flex flex-col">
                            {group.items.map(({ action, index }) => (
                              <div key={action.id} data-idx={index}>
                                <CommandItem
                                  action={action}
                                  active={index === activeIndex}
                                  domId={`${baseId}-opt-${index}`}
                                  itemVariants={v.item}
                                  highlightTransition={v.highlight}
                                  onActivate={() => setActiveIndex(index)}
                                  onSelect={() => choose(action)}
                                />
                              </div>
                            ))}
                          </ul>
                        </li>
                      ))
                    )}
                  </motion.ul>
                </LayoutGroup>

                {/* Footer */}
                <div className="relative z-10 flex items-center gap-4 border-t border-white/[0.06] px-4 py-2.5 text-[12px] text-white/45">
                  <span className="mr-auto flex items-center gap-2 font-medium text-white/55">
                    <BrandMark />
                    SuperSearch
                  </span>
                  <Hint k="↑↓" label="Navigate" />
                  <Hint k="↵" label="Open" />
                  <Hint k="esc" label="Close" />
                </div>
              </div>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function Hint({ k, label }: { k: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <kbd className="rounded-[5px] border border-violet-300/15 bg-violet-400/[0.08] px-1.5 py-0.5 text-[11px] text-white/70">
        {k}
      </kbd>
      <span className="text-white/40">{label}</span>
    </span>
  );
}

function BrandMark() {
  return (
    <span className="relative flex h-4 w-4 shrink-0 items-center justify-center rounded-[5px] bg-gradient-to-br from-violet-500 to-indigo-600 shadow-[0_0_10px_-2px_rgba(139,92,246,0.8)]">
      <span className="h-2 w-2 rounded-full border border-white/70" />
      <span className="absolute h-[3px] w-[3px] rounded-full bg-white" />
    </span>
  );
}

function SearchIcon({ active }: { active?: boolean }) {
  return (
    <svg
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      className={`h-5 w-5 shrink-0 transition-colors duration-200 ${active ? "text-violet-300" : "text-white/35"}`}
      aria-hidden
    >
      <circle cx="8.5" cy="8.5" r="5.5" />
      <line x1="13" y1="13" x2="18" y2="18" />
    </svg>
  );
}
