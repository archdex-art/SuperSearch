/**
 * Shared accent-color plumbing between the palette and the settings window.
 *
 * The accent is applied as a single CSS custom property (`--accent-rgb`, an
 * "R G B" triple) on `<html>`. Tailwind's `accent` color
 * (`tailwind.config.js`) reads it via the `rgb(var(--accent-rgb) / <alpha-value>)`
 * pattern, so `bg-accent/10`, `text-accent`, `border-accent/40`, etc. all
 * repaint instantly from one variable instead of needing a full reload.
 */

/** The built-in "Instrument" identity color, used when no override is set. */
export const DEFAULT_ACCENT_HEX = "#f5a623";
const DEFAULT_ACCENT_RGB = "245 166 35";

function hexToRgbTriple(hex: string): string {
  const m = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex.trim());
  if (!m) return DEFAULT_ACCENT_RGB;
  const [, r, g, b] = m;
  return `${parseInt(r, 16)} ${parseInt(g, 16)} ${parseInt(b, 16)}`;
}

/** Apply an accent color (hex, or nullish for the built-in default) as the
 *  CSS variable every themed surface reads from. */
export function applyAccent(hex: string | null | undefined): void {
  document.documentElement.style.setProperty(
    "--accent-rgb",
    hex ? hexToRgbTriple(hex) : DEFAULT_ACCENT_RGB,
  );
}
