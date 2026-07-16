import { motion } from "framer-motion";
import type { CommandAction } from "./types";
import { actionVerb, categoryStyle, detailValueLabel, sectionLabel } from "./categories";

/**
 * Right-hand preview for the active result — title, icon, and an
 * "Information" block (category / action / path). Mirrors the
 * list-plus-preview shape of a master-detail launcher, filled entirely
 * from fields the row already carries (nothing fabricated).
 */
export function DetailPane({ action }: { action?: CommandAction }) {
  if (!action) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-white/35">
        <span className="relative flex h-10 w-10 items-center justify-center">
          <span className="absolute inset-0 animate-pulse rounded-full bg-amber-400/10 blur-md" />
          <span className="absolute h-7 w-7 rounded-full border border-amber-300/20" />
          <span className="relative h-1.5 w-1.5 rounded-full bg-amber-300 shadow-[0_0_8px_1px_rgba(245,166,35,0.55)]" />
        </span>
        <span className="text-[13px]">Select a result to see details</span>
      </div>
    );
  }

  const style = categoryStyle(action.group);
  const category = sectionLabel(action.group ?? "");
  const verb = actionVerb(action.group ?? "");
  const isImg = typeof action.icon === "string" && /^https?:|^data:|\//.test(action.icon);

  return (
    <motion.div
      key={action.id}
      initial={{ opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.16, ease: "easeOut" }}
      className="flex h-full flex-col overflow-y-auto px-5 py-5"
    >
      <div className="flex flex-col items-center gap-3 pb-6 text-center">
        <span
          className={`flex h-14 w-14 shrink-0 items-center justify-center overflow-hidden rounded-2xl text-3xl ${style.chip}`}
        >
          {isImg ? (
            <img src={action.icon as string} alt="" className="h-full w-full object-cover" />
          ) : (
            (action.icon ?? "•")
          )}
        </span>
        <div className="flex flex-col gap-1">
          <span className="text-[16px] font-semibold leading-tight text-white/95">{action.title}</span>
          {action.subtitle && (
            <span className="max-w-[260px] truncate text-[12.5px] text-white/45">{action.subtitle}</span>
          )}
        </div>
      </div>

      <div className="border-t border-white/[0.06]" />

      <div className="flex flex-col gap-3 pt-4">
        <span className="font-mono text-[10.5px] font-semibold uppercase tracking-[0.14em] text-amber-200/40">Information</span>
        <DetailRow label="Category" value={category} dot={style.dot} />
        <DetailRow label="Action" value={verb} />
        {action.subtitle && (
          <DetailRow label={detailValueLabel(action.group)} value={action.subtitle} mono />
        )}
      </div>
    </motion.div>
  );
}

function DetailRow({
  label,
  value,
  dot,
  mono,
}: {
  label: string;
  value: string;
  dot?: string;
  mono?: boolean;
}) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-white/[0.05] pb-3 text-[12.5px] last:border-0 last:pb-0">
      <span className="shrink-0 font-mono text-[11px] uppercase tracking-wide text-white/35">{label}</span>
      <span
        className={`flex min-w-0 items-center gap-1.5 text-right text-white/75 ${mono ? "font-mono text-[11.5px]" : ""}`}
      >
        {dot && <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${dot}`} />}
        <span className="truncate">{value}</span>
      </span>
    </div>
  );
}
