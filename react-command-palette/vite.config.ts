import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Standalone dev/build for the command-palette demo.
export default defineConfig({
  plugins: [react()],
  // Fixed port so Tauri's devUrl always matches; fail fast if taken.
  server: { port: 5173, strictPort: true },
  // Relative base so the built assets load from Tauri's file:// origin.
  base: "./",
  build: { target: "es2021", outDir: "dist", emptyOutDir: true },
});
