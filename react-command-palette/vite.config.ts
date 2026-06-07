import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Standalone dev/build for the command-palette demo.
export default defineConfig({
  plugins: [react()],
  server: { port: 5173 },
});
