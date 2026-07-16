import defaultTheme from "tailwindcss/defaultTheme";

/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./settings.html", "./*.{ts,tsx}", "./settings/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        sans: ['"Instrument Sans"', ...defaultTheme.fontFamily.sans],
        mono: ['"Martian Mono"', ...defaultTheme.fontFamily.mono],
      },
      colors: {
        // User-customizable accent — see theme.ts. The <alpha-value>
        // placeholder lets every Tailwind opacity modifier (accent/10,
        // accent/40, …) work against the live CSS variable.
        accent: "rgb(var(--accent-rgb) / <alpha-value>)",
        // Theme-aware foreground/overlay ink — white in dark, near-black in
        // light. Every `text-white/*`, `bg-white/*`, `border-white/*`, etc.
        // in the settings window uses this instead of the literal `white`
        // so `theme.ts:applyTheme()` can flip light/dark by repainting one
        // CSS variable, no per-component branching.
        ink: "rgb(var(--ink-rgb) / <alpha-value>)",
        // Window/base surface color — replaces the hardcoded dark hsl().
        canvas: "rgb(var(--canvas-rgb) / <alpha-value>)",
      },
    },
  },
  plugins: [],
};
