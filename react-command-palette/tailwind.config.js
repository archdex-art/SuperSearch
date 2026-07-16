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
      },
    },
  },
  plugins: [],
};
