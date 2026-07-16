import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Single source of truth for the version shown in the settings About pane —
// read at build time so it can never drift from what actually ships.
const tauriConf = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf-8"));

// Standalone dev/build for the command-palette demo.
export default defineConfig({
  plugins: [react()],
  // Fixed port so Tauri's devUrl always matches; fail fast if taken.
  server: { port: 5173, strictPort: true },
  // Relative base so the built assets load from Tauri's file:// origin.
  base: "./",
  define: {
    __SUPERSEARCH_VERSION__: JSON.stringify(tauriConf.version),
  },
  build: {
    target: "es2021",
    outDir: "dist",
    emptyOutDir: true,
    // Two entry HTML files → one build: the frameless palette (index.html)
    // and the decorated settings manager window (settings.html). Tauri loads
    // each by relative path (WebviewUrl::App).
    rollupOptions: {
      input: {
        main: new URL("./index.html", import.meta.url).pathname,
        settings: new URL("./settings.html", import.meta.url).pathname,
      },
    },
  },
});
