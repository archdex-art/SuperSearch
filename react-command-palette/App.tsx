import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { LayoutGroup, motion, useReducedMotion } from "framer-motion";
import { CommandItem } from "./CommandItem";
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

function actionVerb(category: string): string {
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
function sectionLabel(category: string): string {
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
const RANK: Record<string, number> = { Agent: 0, Command: 1, Application: 2, Extension: 3, System: 4, Folder: 5, File: 6 };

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
      return;
    }
    const t = setTimeout(async () => {
      try {
        // Single source of truth: `search_query` already merges native results
        // AND enabled extensions, ranked together server-side (B3). Extension
        // rows carry their action and an `ext:<id>::<title>` id for routing.
        const results = await invoke<BackendResult[]>("search_query", { query: q });
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
        rows.sort((a, b) => (RANK[a.group ?? ""] ?? 99) - (RANK[b.group ?? ""] ?? 99));
        setRows(rows);
        setActiveIndex(0);
      } catch (e) {
        console.error("[palette] search failed", e);
        setRows([]);
      }
    }, 60);
    return () => clearTimeout(t);
  }, [query, hide]);

  // Summon: clear + refocus + replay entrance when the backend emits reset.
  useEffect(() => {
    inputRef.current?.focus();
    let un: undefined | (() => void);
    listen("supersearch://reset", () => {
      setQuery("");
      setRows([]);
      setActiveIndex(0);
      setSummonKey((k) => k + 1);
      requestAnimationFrame(() => inputRef.current?.focus());
    }).then((fn) => (un = fn));
    return () => un?.();
  }, []);

  // Group into contiguous sections; assign each row its global display index.
  const groups = useMemo(() => {
    let index = 0;
    const map = new Map<string, { row: Row; index: number }[]>();
    for (const row of rows) {
      const key = sectionLabel(row.group ?? "");
      const arr = map.get(key) ?? map.set(key, []).get(key)!;
      arr.push({ row, index: index++ });
    }
    return [...map.entries()].map(([label, items]) => ({ label, items }));
  }, [rows]);

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
      const n = rows.length;
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
        case "Enter": e.preventDefault(); choose(rows[activeIndex]); break;
        case "Escape": e.preventDefault(); hide(); break;
      }
    },
    [rows, activeIndex, choose, hide, scrollInto],
  );

  return (
    <div className="h-screen w-screen overflow-hidden bg-transparent p-0 text-white" onKeyDown={onKeyDown}>
      <motion.div
        key={summonKey}
        variants={v.panel}
        initial="hidden"
        animate="visible"
        style={{ willChange: "transform, opacity" }}
        className="flex h-full w-full flex-col overflow-hidden rounded-[18px] border border-white/15
                   bg-[hsla(228,18%,12%,0.72)] shadow-[0_28px_70px_-18px_rgba(0,0,0,0.6)]
                   ring-1 ring-inset ring-white/[0.06] backdrop-blur-2xl"
        role="dialog"
        aria-label="SuperSearch"
      >
        {/* Search input */}
        <div className="flex h-[60px] items-center gap-3 border-b border-white/[0.07] px-5">
          <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" className="h-5 w-5 shrink-0 text-white/35" aria-hidden>
            <circle cx="8.5" cy="8.5" r="5.5" /><line x1="13" y1="13" x2="18" y2="18" />
          </svg>
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => { setQuery(e.target.value); setActiveIndex(0); }}
            placeholder="Search apps, files, or ask anything…"
            role="combobox"
            aria-expanded
            aria-controls={`${baseId}-list`}
            aria-activedescendant={rows.length ? `${baseId}-opt-${activeIndex}` : undefined}
            autoComplete="off" autoCorrect="off" spellCheck={false}
            className="h-full flex-1 bg-transparent text-[19px] font-normal outline-none caret-emerald-400 placeholder:text-white/35"
          />
        </div>

        {/* Results */}
        <LayoutGroup>
          <motion.ul
            ref={listRef}
            id={`${baseId}-list`}
            role="listbox"
            variants={v.list}
            className="flex-1 overflow-y-auto overscroll-contain p-2"
          >
            {rows.length === 0 ? (
              <li className="flex h-full flex-col items-center justify-center gap-1 text-center text-white/40">
                <span className="text-2xl opacity-60">✨</span>
                <span className="text-[13px]">{query ? "No results" : "Ask anything, or search apps & commands"}</span>
              </li>
            ) : (
              groups.map((g) => (
                <li key={g.label} role="presentation">
                  <div className="px-3 pb-1 pt-3 text-[11px] font-semibold uppercase tracking-wide text-white/40">{g.label}</div>
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
        <div className="flex items-center gap-4 border-t border-white/[0.07] px-4 py-2.5 text-[12px] text-white/45">
          <span className="mr-auto font-medium text-white/55">⌘ SuperSearch</span>
          <FooterHint k="↑↓" label="Navigate" />
          <FooterHint k="↵" label="Open" />
          <FooterHint k="esc" label="Close" />
        </div>
      </motion.div>
    </div>
  );
}

function FooterHint({ k, label }: { k: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <kbd className="rounded-[5px] border border-white/15 bg-white/10 px-1.5 py-0.5 text-[11px] text-white/70">{k}</kbd>
      <span className="text-white/40">{label}</span>
    </span>
  );
}
