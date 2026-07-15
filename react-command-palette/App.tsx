import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { LayoutGroup, motion, useReducedMotion } from "framer-motion";
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
import { invoke, listen, type BackendResult } from "./bridge";
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
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const baseId = useId();

  const hide = useCallback(() => void invoke("hide_window"), []);

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
            if (r.id.startsWith("ext:") && r.action != null) {
              // Extension result — dispatch through its capability token.
              const extId = r.id.slice("ext:".length).split("::")[0];
              await invoke("execute_extension_action", { id: extId, action: r.action });
            } else {
              await invoke("execute_action", { request: { action_id: r.id, with_meta: false } });
            }
            hide();
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
      }
    },
    [filteredRows, activeIndex, choose, hide, scrollInto, filterOpen],
  );

  const activeRow = filteredRows[activeIndex];

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-transparent p-3 text-white" onKeyDown={onKeyDown}>
      <motion.div
        key={summonKey}
        variants={v.panel}
        initial="hidden"
        animate="visible"
        style={{ willChange: "transform, opacity" }}
        className="relative h-full w-full"
      >
        {/* Animated rim — a slowly rotating conic sweep clipped to a 1.5px ring. */}
        <div className="aurora-frame relative h-full w-full rounded-[20px] p-[1.5px] shadow-[0_30px_80px_-20px_rgba(0,0,0,0.65)]">
          <div
            role="dialog"
            aria-label="SuperSearch"
            className="relative flex h-full w-full flex-col overflow-hidden rounded-[18.5px] border border-white/[0.05]
                       bg-[hsla(255,22%,9%,0.78)] ring-1 ring-inset ring-white/[0.05] backdrop-blur-2xl"
          >
            {/* Ambient color wash, sitting on the glass beneath all content. */}
            <div
              className="pointer-events-none absolute inset-0"
              style={{
                background:
                  "radial-gradient(120% 90% at 0% 0%, rgba(139,92,246,0.14), transparent 55%), " +
                  "radial-gradient(120% 90% at 100% 100%, rgba(245,166,35,0.09), transparent 55%)",
              }}
            />

            {/* Search input */}
            <div className="relative z-10 flex h-[58px] items-center gap-3 border-b border-white/[0.06] px-5">
              <svg
                viewBox="0 0 20 20"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                className={`h-5 w-5 shrink-0 transition-colors duration-200 ${query ? "text-violet-300" : "text-white/35"}`}
                aria-hidden
              >
                <circle cx="8.5" cy="8.5" r="5.5" /><line x1="13" y1="13" x2="18" y2="18" />
              </svg>
              <input
                ref={inputRef}
                value={query}
                onChange={(e) => { setQuery(e.target.value); setActiveIndex(0); }}
                onFocus={() => setFilterOpen(false)}
                placeholder="Search apps, files, or ask anything…"
                role="combobox"
                aria-expanded
                aria-controls={`${baseId}-list`}
                aria-activedescendant={filteredRows.length ? `${baseId}-opt-${activeIndex}` : undefined}
                autoComplete="off" autoCorrect="off" spellCheck={false}
                className="h-full flex-1 bg-transparent text-[18px] font-normal outline-none caret-violet-400 placeholder:text-white/35"
              />
              {rows.length > 0 && (
                <span className="shrink-0 rounded-full bg-white/[0.06] px-2.5 py-1 text-[11px] font-medium text-white/40 ring-1 ring-inset ring-white/[0.06]">
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
                  <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-white/40">
                    <span className="relative flex h-10 w-10 items-center justify-center">
                      <span className="absolute inset-0 animate-pulse rounded-full bg-gradient-to-br from-violet-500/25 to-amber-400/15 blur-md" />
                      <span className="relative h-2.5 w-2.5 rounded-full bg-gradient-to-br from-violet-300 to-amber-200" />
                      <span className="absolute h-6 w-6 rounded-full border border-white/15" />
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
                      className="w-[44%] min-w-[210px] shrink-0 overflow-y-auto overscroll-contain border-r border-white/[0.06] p-2"
                    >
                      {groups.map((g) => (
                        <li key={g.label} role="presentation">
                          <div className="flex items-center gap-1.5 px-2.5 pb-1 pt-3 text-[11px] font-semibold uppercase tracking-wide text-white/40">
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

            {/* Footer */}
            <div className="relative z-10 flex items-center gap-4 border-t border-white/[0.06] px-4 py-2.5 text-[12px] text-white/45">
              {activeRow ? (
                <span className="mr-auto flex min-w-0 items-center gap-2 font-medium text-white/70">
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
                <span className="mr-auto flex items-center gap-2 font-medium text-white/55">
                  <BrandMark />
                  SuperSearch
                </span>
              )}
              <FooterHint k="↑↓" label="Navigate" />
              <FooterHint k="↵" label={activeRow?.hint ?? "Open"} />
              <FooterHint k="esc" label="Close" />
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
      <kbd className="rounded-[5px] border border-violet-300/15 bg-violet-400/[0.08] px-1.5 py-0.5 text-[11px] text-white/70">{k}</kbd>
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
        className="flex items-center gap-1.5 rounded-lg border border-white/[0.08] bg-white/[0.05] px-2.5 py-1.5 text-[12px]
                   font-medium text-white/60 transition-colors hover:bg-white/[0.08] hover:text-white/85"
      >
        <span className={`h-1.5 w-1.5 rounded-full ${value ? categoryStyle(value).dot : "bg-white/40"}`} />
        {value ? sectionLabel(value) : "All"}
        <svg
          viewBox="0 0 12 12"
          className={`h-3 w-3 text-white/35 transition-transform duration-150 ${open ? "rotate-180" : ""}`}
          fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round"
          aria-hidden
        >
          <path d="M3 4.5 6 7.5 9 4.5" />
        </svg>
      </button>

      {open && (
        <div
          className="absolute right-0 top-[calc(100%+6px)] z-20 min-w-[152px] overflow-hidden rounded-xl border border-white/[0.08]
                     bg-[hsla(255,20%,11%,0.96)] py-1 shadow-[0_16px_40px_-12px_rgba(0,0,0,0.6)] backdrop-blur-xl"
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
        active ? "bg-white/[0.08] text-white/95" : "text-white/60 hover:bg-white/[0.05] hover:text-white/85"
      }`}
    >
      <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${dot ?? "bg-white/40"}`} />
      {label}
    </button>
  );
}
