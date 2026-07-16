/**
 * Small hand-built primitives shared by every settings pane. Six controls
 * isn't enough surface to justify pulling in a component library — these
 * just need to read as siblings of the palette's "Instrument" identity
 * (amber accent, Martian Mono labels, Instrument Sans body).
 */
import type { ReactNode } from "react";

export function Toggle({
  checked,
  onChange,
  label,
  description,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** Omit (or pass "") for a bare switch with no label row — e.g. inline in
   *  a list row that already shows its own title/description. */
  label?: string;
  description?: string;
  disabled?: boolean;
}) {
  const switchEl = (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label || undefined}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-[22px] w-[38px] shrink-0 rounded-full transition-colors duration-150 ${
        checked ? "bg-accent/80" : "bg-ink/[0.12]"
      }`}
    >
      <span
        className={`absolute left-0 top-[3px] h-4 w-4 rounded-full bg-white shadow-[0_1px_3px_rgba(0,0,0,0.4)] transition-transform duration-150 ${
          checked ? "translate-x-[19px]" : "translate-x-[3px]"
        }`}
      />
    </button>
  );

  if (!label) return switchEl;

  return (
    <label className={`flex items-center justify-between gap-4 py-3 ${disabled ? "opacity-40" : "cursor-pointer"}`}>
      <span className="flex min-w-0 flex-col gap-0.5">
        <span className="text-[13.5px] font-medium text-ink/90">{label}</span>
        {description && <span className="text-[12px] leading-snug text-ink/40">{description}</span>}
      </span>
      {switchEl}
    </label>
  );
}


export function Button({
  children,
  onClick,
  variant = "secondary",
  disabled,
  type = "button",
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "primary" | "secondary" | "danger";
  disabled?: boolean;
  type?: "button" | "submit";
}) {
  const styles: Record<typeof variant, string> = {
    primary: "border-accent/40 bg-accent/[0.14] text-accent hover:bg-accent/[0.2]",
    secondary: "border-ink/[0.1] bg-ink/[0.05] text-ink/75 hover:bg-ink/[0.08] hover:text-ink/95",
    danger: "border-rose-400/30 bg-rose-500/[0.1] text-rose-200 hover:bg-rose-500/[0.16]",
  };
  return (
    <button
      type={type}
      onClick={onClick}
      disabled={disabled}
      className={`rounded-lg border px-3.5 py-1.5 text-[12.5px] font-medium transition-colors active:scale-[0.97] disabled:pointer-events-none disabled:opacity-40 ${styles[variant]}`}
    >
      {children}
    </button>
  );
}

export function SectionHeading({ children }: { children: ReactNode }) {
  return (
    <h2 className="mb-1 font-mono text-[11px] font-semibold uppercase tracking-[0.14em] text-accent/45">
      {children}
    </h2>
  );
}

export function Card({ children }: { children: ReactNode }) {
  return (
    <div className="divide-y divide-ink/[0.06] rounded-xl border border-ink/[0.07] bg-ink/[0.025] px-4">
      {children}
    </div>
  );
}

export function Row({ children }: { children: ReactNode }) {
  return <div className="flex items-center justify-between gap-4 py-3">{children}</div>;
}

export function Pill({ tone = "neutral", children }: { tone?: "neutral" | "amber" | "rose"; children: ReactNode }) {
  const styles: Record<typeof tone, string> = {
    neutral: "bg-ink/[0.07] text-ink/50 ring-ink/[0.08]",
    amber: "bg-accent/10 text-accent/90 ring-accent/25",
    rose: "bg-rose-500/10 text-rose-200/90 ring-rose-300/25",
  };
  return (
    <span className={`rounded-full px-2 py-0.5 text-[10.5px] font-medium ring-1 ring-inset ${styles[tone]}`}>
      {children}
    </span>
  );
}
