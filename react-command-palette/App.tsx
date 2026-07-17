import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { AnimatePresence, LayoutGroup, motion, useReducedMotion } from "framer-motion";
import { CommandItem } from "./CommandItem";
import { DetailPane } from "./DetailPane";
import {
  actionVerb,
  CATEGORY_RANK,
  categoryStyle,
  sectionLabel,
} from "./categories";
import {
  highlightSpring,
  itemVariants,
  listVariants,
  panelVariants,
  reducedVariants,
} from "./variants";
import { invoke, listen, type BackendResult, type ExecuteActionResponse } from "./bridge";
import { applyAccent, applyTheme } from "./theme";
import type { CommandAction } from "./types";

/** A palette row plus how to run it. */
type Row = CommandAction & { perform: () => void | Promise<void> };

export default function App() {
  const prefersReduced = useReducedMotion();
  const v = useMemo(
    () =>
      prefersReduced
        ? reducedVariants
        : { panel: panelVariants, list: listVariants, item: itemVariants, highlight: highlightSpring },
    [prefersReduced],
  );

  const [query, setQuery] = useState("");
  const [rows, setRows] = useState<Row[]>([]);
  const [activeIndex, setActiveIndex] = useState(0);
  const [categoryFilter, setCategoryFilter] = useState<string | null>(null);
  const [filterOpen, setFilterOpen] = useState(false);
  const [summonKey, setSummonKey] = useState(0); // bumped on each summon → replays entrance
  // True while the panel plays its exit animation; the *native* window only
  // actually hides once that finishes (see `onPanelAnimationComplete`), so
  // every dismissal path — Escape, selecting a result, the hotkey toggle,
  // and blur — gets the same smooth collapse instead of an instant snap.
  const [closing, setClosing] = useState(false);
  // Set when a selected action's underlying OS call fails (bad/missing path,
  // no default handler, an extension throwing, …) — surfaced inline instead
  // of the palette just silently closing as if nothing happened.
  const [actionError, setActionError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const baseId = useId();
  // Mirrors `closing` for the toggle-hotkey listener below (registered once
  // on mount, so a plain closure over `closing` would read a stale value).
  // Updated synchronously by `setClosingBoth`, NOT via a `useEffect` mirror —
  // an effect only runs after React commits the next render, which leaves a
  // real gap: a hotkey re-summon landing in that gap during the ~150ms exit
  // animation would read the *previous* value of `closingRef.current` and
  // silently drop the summon (see the `toggle-request` listener) instead of
  // cancelling the close and reopening. This is user-visible as "the hotkey
  // doesn't do anything" whenever a press lands while the panel is still
  // finishing a close.
  const closingRef = useRef(false);
  const setClosingBoth = useCallback((value: boolean) => {
    closingRef.current = value;
    setClosing(value);
  }, []);

  const hide = useCallback(() => setClosingBoth(true), [setClosingBoth]);

  /** Fires when the panel's `animate` variant finishes transitioning. Only
   *  the "exit" variant should ever trigger the real window hide. */
  const onPanelAnimationComplete = useCallback((definition: unknown) => {
    if (definition === "exit") {
      void invoke("hide_window");
      setClosingBoth(false);
    }
  }, [setClosingBoth]);

  // Debounced server-side search (native results + enabled extensions).
  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setRows([]);
      setActiveIndex(0);
      setCategoryFilter(null);
      setFilterOpen(false);
      return;
    }
    // Guards the async response below: cleared whenever this effect re-runs
    // (a newer keystroke) before the in-flight `invoke` resolves, so a slower
    // stale response can never clobber a faster, newer one's rows.
    let cancelled = false;
    const t = setTimeout(async () => {
      try {
        // Single source of truth: `search_query` already merges native results
        // AND enabled extensions, ranked together server-side (B3). Extension
        // rows carry their action and an `ext:<id>::<title>` id for routing.
        const results = await invoke<BackendResult[]>("search_query", { query: q });
        if (cancelled) return;
        const rows: Row[] = (results ?? []).map((r) => ({
          id: r.id,
          title: r.title,
          subtitle: r.subtitle,
          icon: r.icon,
          group: r.category,
          hint: actionVerb(r.category),
          perform: async () => {
            setActionError(null);
            try {
              if (r.id.startsWith("ext:") && r.action != null) {
                // Extension result — dispatch through its capability token.
                const extId = r.id.slice("ext:".length).split("::")[0];
                await invoke("execute_extension_action", { id: extId, action: r.action });
              } else {
                const response = await invoke<ExecuteActionResponse>("execute_action", {
                  request: { action_id: r.id, with_meta: false },
                });
                if (!response.success) {
                  // Surface the real OS-level failure (bad/missing path, no
                  // default handler, a permission gate, …) instead of
                  // silently closing as if nothing happened.
                  setActionError(response.detail.replace(/^✗\s*/, ""));
                  return;
                }
              }
              hide();
            } catch (e) {
              setActionError(String(e instanceof Error ? e.message : e));
            }
          },
        }));
        rows.sort((a, b) => (CATEGORY_RANK[a.group ?? ""] ?? 99) - (CATEGORY_RANK[b.group ?? ""] ?? 99));
        setRows(rows);
        setActiveIndex(0);
      } catch (e) {
        if (cancelled) return;
        console.error("[palette] search failed", e);
        setRows([]);
      }
    }, 60);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [query, hide]);

  // Summon: clear + refocus + replay entrance when the backend emits reset.
  useEffect(() => {
    inputRef.current?.focus();
    let un: undefined | (() => void);
    // If the component unmounts before `listen` resolves, `un` is still
    // undefined when the cleanup below runs — the listener registered
    // moments later would otherwise never be unregistered. `cancelled`
    // makes the `.then` unlisten immediately instead of stashing the handle.
    let cancelled = false;
    listen("supersearch://reset", () => {
      setQuery("");
      setRows([]);
      setActiveIndex(0);
      setCategoryFilter(null);
      setFilterOpen(false);
      setActionError(null);
      setSummonKey((k) => k + 1);
      requestAnimationFrame(() => inputRef.current?.focus());
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        un = fn;
      }
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);

  // Apply the persisted accent + base theme on boot, then keep both live:
  // the settings window broadcasts `settings-changed` on every save, so a
  // color or theme picked there repaints the palette immediately — no
  // reopen required. Each Tauri window is its own webview with an
  // independent `document`, so this listener (not just the one in the
  // settings window) is what makes the palette itself pick up a theme
  // change instead of always rendering the dark default.
  useEffect(() => {
    void invoke<{ accent_color?: string | null; theme?: string }>("get_settings").then((s) => {
      applyAccent(s.accent_color);
      applyTheme(s.theme);
    });
    let un: undefined | (() => void);
    let cancelled = false;
    listen<{ accent_color?: string | null; theme?: string }>("supersearch://settings-changed", (s) => {
      applyAccent(s.accent_color);
      applyTheme(s.theme);
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        un = fn;
      }
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);

  // Rust-initiated, unconditional dismissal (blur-hide) — always animates
  // closed, same as Escape/selecting a result.
  useEffect(() => {
    let un: undefined | (() => void);
    let cancelled = false;
    listen("supersearch://request-close", () => hide()).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        un = fn;
      }
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, [hide]);

  // The global hotkey toggling while the window is still visible has two
  // cases Rust can't distinguish on its own (it only knows OS-level
  // visibility, which stays true for the whole exit animation): if we're
  // genuinely idle-open, this is a normal close. But if a close is already
  // *animating out* (closing === true) — e.g. the user double-tapped the
  // hotkey — snapping shut again would just look like the hotkey didn't do
  // anything; instead cancel the exit and let Framer Motion smoothly
  // retarget back to "visible" from wherever the panel currently is.
  useEffect(() => {
    let un: undefined | (() => void);
    let cancelled = false;
    listen("supersearch://toggle-request", () => {
      if (closingRef.current) {
        setClosingBoth(false);
        requestAnimationFrame(() => inputRef.current?.focus());
      } else {
        hide();
      }
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        un = fn;
      }
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, [hide]);

  // Distinct categories present in the current (unfiltered) result set, in
  // display-rank order — feeds the type filter's option list.
  const availableCategories = useMemo(() => {
    const set = new Set<string>();
    for (const r of rows) if (r.group) set.add(r.group);
    return [...set].sort((a, b) => (CATEGORY_RANK[a] ?? 99) - (CATEGORY_RANK[b] ?? 99));
  }, [rows]);

  // If a fresh search no longer contains the filtered category, drop the filter.
  useEffect(() => {
    if (categoryFilter && !availableCategories.includes(categoryFilter)) {
      setCategoryFilter(null);
    }
  }, [availableCategories, categoryFilter]);

  const filteredRows = useMemo(
    () => (categoryFilter ? rows.filter((r) => r.group === categoryFilter) : rows),
    [rows, categoryFilter],
  );

  useEffect(() => {
    setActiveIndex(0);
  }, [categoryFilter]);

  // Group into contiguous sections; assign each row its global display index.
  const groups = useMemo(() => {
    let index = 0;
    const map = new Map<string, { row: Row; index: number }[]>();
    for (const row of filteredRows) {
      const key = sectionLabel(row.group ?? "");
      const arr = map.get(key) ?? map.set(key, []).get(key)!;
      arr.push({ row, index: index++ });
    }
    return [...map.entries()].map(([label, items]) => ({ label, items }));
  }, [filteredRows]);

  const choose = useCallback(
    (row?: Row) => {
      if (!row) return;
      void row.perform();
    },
    [],
  );

  const scrollInto = useCallback((idx: number) => {
    listRef.current?.querySelector<HTMLElement>(`[data-idx="${idx}"]`)?.scrollIntoView({ block: "nearest" });
  }, []);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Escape" && filterOpen) {
        e.preventDefault();
        setFilterOpen(false);
        return;
      }
      const n = filteredRows.length;
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setActiveIndex((i) => { const x = n ? (i + 1) % n : 0; scrollInto(x); return x; });
          break;
        case "ArrowUp":
          e.preventDefault();
          setActiveIndex((i) => { const x = n ? (i - 1 + n) % n : 0; scrollInto(x); return x; });
          break;
        case "Home": e.preventDefault(); setActiveIndex(0); scrollInto(0); break;
        case "End": e.preventDefault(); setActiveIndex(n - 1); scrollInto(n - 1); break;
        case "Enter": e.preventDefault(); choose(filteredRows[activeIndex]); break;
        case "Escape": e.preventDefault(); hide(); break;
        case ",":
          if (e.metaKey || e.ctrlKey) {
            e.preventDefault();
            void invoke("open_settings_window");
          }
          break;
      }
    },
    [filteredRows, activeIndex, choose, hide, scrollInto, filterOpen],
  );

  const activeRow = filteredRows[activeIndex];

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-transparent p-3 text-ink" onKeyDown={onKeyDown}>
      <motion.div
        key={summonKey}
        variants={v.panel}
        initial="hidden"
        animate={closing ? "exit" : "visible"}
        onAnimationComplete={onPanelAnimationComplete}
        style={{ willChange: "transform, opacity" }}
        className="relative h-full w-full"
      >
        {/* Hairline frame — a static 1px accent-tinted ring around the panel.
            Shadow spread is deliberately small: the window itself is
            transparent with no native shadow (see tauri.conf.json), and the
            outer wrapper only has 12px of padding (`p-3`) around this panel.
            A shadow that reaches further than that gets hard-clipped at the
            window's actual rectangular pixel bounds instead of fading out —
            which reads as a faint translucent rectangle floating around the
            rounded card. Keeping the blur/spread inside that 12px budget
            keeps the shadow soft all the way to nothing. */}
        <div className="relative h-full w-full rounded-[16px] shadow-[0_6px_16px_-12px_rgba(0,0,0,0.6),0_0_10px_-6px_rgb(var(--accent-rgb)/0.35)]">
          <div
            role="dialog"
            aria-label="SuperSearch"
            className="relative flex h-full w-full flex-col overflow-hidden rounded-[16px] border border-accent/[0.14]
                       bg-canvas/[0.88] ring-1 ring-inset ring-ink/[0.04] backdrop-blur-2xl"
          >
            {/* Schematic grid + grain — reads as an instrument surface, not a flat blur. */}
            <div className="hud-grid pointer-events-none absolute inset-0" />
            <div className="grain-overlay pointer-events-none absolute inset-0" />
            {/* Ambient wash, single-hue, sitting on the glass beneath all content. */}
            <div
              className="pointer-events-none absolute inset-0"
              style={{ background: "radial-gradient(120% 90% at 0% 0%, rgb(var(--accent-rgb) / 0.12), transparent 55%)" }}
            />
            <HudCorners />

            {/* Search input */}
            <div className="relative z-10 flex h-[58px] items-center gap-3 border-b border-ink/[0.06] px-5">
              <svg
                viewBox="0 0 20 20"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                className={`h-5 w-5 shrink-0 transition-colors duration-200 ${query ? "text-accent" : "text-ink/35"}`}
                aria-hidden
              >
                <circle cx="8.5" cy="8.5" r="5.5" /><line x1="13" y1="13" x2="18" y2="18" />
              </svg>
              <input
                ref={inputRef}
                value={query}
                onChange={(e) => { setQuery(e.target.value); setActiveIndex(0); setActionError(null); }}
                onFocus={() => setFilterOpen(false)}
                placeholder="Search apps, files, or ask anything…"
                role="combobox"
                aria-expanded
                aria-controls={`${baseId}-list`}
                aria-activedescendant={filteredRows.length ? `${baseId}-opt-${activeIndex}` : undefined}
                autoComplete="off" autoCorrect="off" spellCheck={false}
                className="h-full flex-1 bg-transparent text-[18px] font-normal outline-none caret-accent placeholder:text-ink/35"
              />
              {rows.length > 0 && (
                <span className="shrink-0 rounded-full bg-ink/[0.06] px-2.5 py-1 font-mono text-[11px] font-medium text-ink/40 ring-1 ring-inset ring-ink/[0.06]">
                  {filteredRows.length} {filteredRows.length === 1 ? "result" : "results"}
                </span>
              )}
              {availableCategories.length > 1 && (
                <TypeFilter
                  value={categoryFilter}
                  options={availableCategories}
                  open={filterOpen}
                  onOpenChange={setFilterOpen}
                  onSelect={setCategoryFilter}
                />
              )}
            </div>

            {/* Results — a narrow list plus a detail preview of the active row. */}
            <LayoutGroup>
              <div className="relative z-10 flex-1 overflow-hidden">
                {filteredRows.length === 0 ? (
                  <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-ink/40">
                    <span className="relative flex h-10 w-10 items-center justify-center">
                      <span className="absolute inset-0 animate-pulse rounded-full bg-accent/10 blur-md" />
                      <span className="absolute h-7 w-7 rounded-full border border-accent/20" />
                      <span className="relative h-1.5 w-1.5 rounded-full bg-accent shadow-[0_0_8px_1px_rgb(var(--accent-rgb)/0.55)]" />
                    </span>
                    <span className="text-[13px]">
                      {query
                        ? categoryFilter
                          ? "No results in this category"
                          : "No results"
                        : "Ask anything, or search apps & commands"}
                    </span>
                  </div>
                ) : (
                  <div className="flex h-full overflow-hidden">
                    <motion.ul
                      ref={listRef}
                      id={`${baseId}-list`}
                      role="listbox"
                      variants={v.list}
                      className="w-[44%] min-w-[210px] shrink-0 overflow-y-auto overscroll-contain border-r border-ink/[0.06] p-2"
                    >
                      {groups.map((g) => (
                        <li key={g.label} role="presentation">
                          <div className="flex items-center gap-1.5 px-2.5 pb-1 pt-3 font-mono text-[10.5px] font-semibold uppercase tracking-[0.1em] text-ink/40">
                            <span className={`h-1 w-1 rounded-full ${categoryStyle(g.items[0]?.row.group).dot}`} />
                            {g.label}
                          </div>
                          <ul role="presentation" className="flex flex-col">
                            {g.items.map(({ row, index }) => (
                              <div key={row.id} data-idx={index}>
                                <CommandItem
                                  action={row}
                                  active={index === activeIndex}
                                  domId={`${baseId}-opt-${index}`}
                                  itemVariants={v.item}
                                  highlightTransition={v.highlight}
                                  onActivate={() => setActiveIndex(index)}
                                  onSelect={() => choose(row)}
                                  compact
                                />
                              </div>
                            ))}
                          </ul>
                        </li>
                      ))}
                    </motion.ul>

                    <div className="flex-1 overflow-hidden">
                      <DetailPane action={activeRow} />
                    </div>
                  </div>
                )}
              </div>
            </LayoutGroup>

            <AnimatePresence>
              {actionError && (
                <motion.div
                  role="alert"
                  aria-live="assertive"
                  initial={{ opacity: 0, y: 6 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: 6 }}
                  transition={{ duration: 0.15 }}
                  className="relative z-10 mx-4 mb-2.5 flex items-start gap-2 rounded-lg border border-rose-400/25 bg-rose-500/[0.1] px-3 py-2 text-[12px] leading-snug text-rose-200/90"
                >
                  <span className="mt-[3px] h-1.5 w-1.5 shrink-0 rounded-full bg-rose-400" />
                  <span className="min-w-0 flex-1">{actionError}</span>
                  <button
                    type="button"
                    onClick={() => setActionError(null)}
                    aria-label="Dismiss"
                    className="shrink-0 text-rose-300/60 hover:text-rose-200"
                  >
                    ✕
                  </button>
                </motion.div>
              )}
            </AnimatePresence>

            {/* Footer */}
            <div className="relative z-10 flex items-center gap-4 border-t border-ink/[0.06] px-4 py-2.5 text-[12px] text-ink/45">
              {activeRow ? (
                <span className="mr-auto flex min-w-0 items-center gap-2 font-medium text-ink/70">
                  <span
                    className={`flex h-4 w-4 shrink-0 items-center justify-center overflow-hidden rounded-[5px] text-[10px] ${categoryStyle(activeRow.group).chip}`}
                  >
                    {typeof activeRow.icon === "string" && /^https?:|^data:|\//.test(activeRow.icon) ? (
                      <img src={activeRow.icon} alt="" className="h-full w-full object-cover" />
                    ) : (
                      (activeRow.icon ?? "•")
                    )}
                  </span>
                  <span className="truncate">
                    {activeRow.hint ?? "Open"} {activeRow.title}
                  </span>
                </span>
              ) : (
                <span className="mr-auto flex items-center gap-2 font-mono text-[11px] font-semibold uppercase tracking-[0.12em] text-ink/55">
                  <BrandMark />
                  SuperSearch
                </span>
              )}
              <FooterHint k="↑↓" label="Navigate" />
              <FooterHint k="↵" label={activeRow?.hint ?? "Open"} />
              <FooterHint k="esc" label="Close" />
              <button
                type="button"
                onClick={() => void invoke("open_settings_window")}
                title="Settings (⌘,)"
                aria-label="Open Settings"
                className="flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-ink/35 transition-colors hover:bg-ink/[0.08] hover:text-ink/70"
              >
                <svg viewBox="0 0 16 16" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <circle cx="8" cy="8" r="2" />
                  <path d="M8 1.6v1.5M8 12.9v1.5M14.4 8h-1.5M3.1 8H1.6M12.5 3.5l-1 1M4.5 11.5l-1 1M12.5 12.5l-1-1M4.5 4.5l-1-1" />
                </svg>
              </button>
            </div>
          </div>
        </div>
      </motion.div>
    </div>
  );
}

function FooterHint({ k, label }: { k: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <kbd className="rounded-[5px] border border-accent/20 bg-accent/[0.08] px-1.5 py-0.5 font-mono text-[11px] text-ink/70">{k}</kbd>
      <span className="text-ink/40">{label}</span>
    </span>
  );
}

function BrandMark() {
  return (
    <span className="relative flex h-4 w-4 shrink-0 items-center justify-center rounded-[4px] border border-accent/40 bg-accent/[0.07]">
      <span className="h-[3px] w-[3px] rounded-full bg-accent shadow-[0_0_6px_1px_rgb(var(--accent-rgb)/0.7)]" />
    </span>
  );
}

/** Four viewfinder-style corner ticks inset from the panel's edges — the
 *  frame's signature detail, echoing the "instrument" identity instead of
 *  a soft glow. Purely decorative: absolutely positioned, no pointer events. */
function HudCorners() {
  const base = "pointer-events-none absolute z-10 h-2.5 w-2.5 border-accent/25";
  return (
    <>
      <span className={`${base} left-2.5 top-2.5 border-l border-t`} />
      <span className={`${base} right-2.5 top-2.5 border-r border-t`} />
      <span className={`${base} left-2.5 bottom-2.5 border-l border-b`} />
      <span className={`${base} right-2.5 bottom-2.5 border-r border-b`} />
    </>
  );
}

/** Result-type filter chip — narrows the list to one category's rows. */
function TypeFilter({
  value,
  options,
  open,
  onOpenChange,
  onSelect,
}: {
  value: string | null;
  options: string[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (value: string | null) => void;
}) {
  return (
    <div
      className="relative shrink-0"
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node)) onOpenChange(false);
      }}
    >
      <button
        type="button"
        onClick={() => onOpenChange(!open)}
        className="flex items-center gap-1.5 rounded-lg border border-ink/[0.08] bg-ink/[0.05] px-2.5 py-1.5 text-[12px]
                   font-medium text-ink/60 transition-colors hover:bg-ink/[0.08] hover:text-ink/85"
      >
        <span className={`h-1.5 w-1.5 rounded-full ${value ? categoryStyle(value).dot : "bg-ink/40"}`} />
        {value ? sectionLabel(value) : "All"}
        <svg
          viewBox="0 0 12 12"
          className={`h-3 w-3 text-ink/35 transition-transform duration-150 ${open ? "rotate-180" : ""}`}
          fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round"
          aria-hidden
        >
          <path d="M3 4.5 6 7.5 9 4.5" />
        </svg>
      </button>

      {open && (
        <div
          className="absolute right-0 top-[calc(100%+6px)] z-20 min-w-[152px] overflow-hidden rounded-xl border border-accent/[0.12]
                     bg-canvas/[0.97] py-1 shadow-[0_16px_40px_-12px_rgba(0,0,0,0.6)] backdrop-blur-xl"
        >
          <FilterOption
            label="All"
            active={value === null}
            onClick={() => { onSelect(null); onOpenChange(false); }}
          />
          {options.map((cat) => (
            <FilterOption
              key={cat}
              label={sectionLabel(cat)}
              dot={categoryStyle(cat).dot}
              active={value === cat}
              onClick={() => { onSelect(cat); onOpenChange(false); }}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function FilterOption({
  label,
  dot,
  active,
  onClick,
}: {
  label: string;
  dot?: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-[12.5px] transition-colors ${
        active ? "bg-ink/[0.08] text-ink/95" : "text-ink/60 hover:bg-ink/[0.05] hover:text-ink/85"
      }`}
    >
      <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${dot ?? "bg-ink/40"}`} />
      {label}
    </button>
  );
}
