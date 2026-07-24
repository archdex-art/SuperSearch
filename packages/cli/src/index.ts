#!/usr/bin/env node

/**
 * SuperSearch Developer CLI (Phase 12 / Milestone 4)
 *
 * Provides scaffolding, production builds (with 5MB quotas), and the
 * `dev` hot-reloading loop bridging esbuild watch mode with the Rust Host.
 */

import { Command } from "commander";
import * as esbuild from "esbuild";
import { WebSocket } from "ws";
import * as path from "path";
import * as fs from "fs";

const program = new Command();
program.name("supersearch").description("SuperSearch Extension Developer CLI").version("1.0.0");

const MAX_BUNDLE_SIZE = 5 * 1024 * 1024; // 5MB

program
    .command("build")
    .description("Compile the extension for production distribution")
    .action(async () => {
        console.log("Building extension...");
        const result = await esbuild.build({
            entryPoints: ["src/index.tsx"],
            bundle: true,
            minify: true,
            format: "esm",
            outfile: "dist/bundle.js",
            metafile: true,
        });

        const stat = fs.statSync("dist/bundle.js");
        if (stat.size > MAX_BUNDLE_SIZE) {
            console.error(`❌ Build failed: Bundle size (${(stat.size / 1024 / 1024).toFixed(2)} MB) exceeds 5MB quota.`);
            process.exit(1);
        }

        console.log("✅ Build complete.");
    });

program
    .command("dev")
    .description("Start the local development server with Fast Refresh")
    .action(async () => {
        console.log("Starting SuperSearch Dev Server...");

        // Connect to the Rust Host's Dev Mode socket (Task 4.3)
        const ws = new WebSocket("ws://127.0.0.1:9999/hmr");

        ws.on("open", () => {
            console.log("🔌 Connected to SuperSearch Host.");
        });

        ws.on("error", (err) => {
            console.error("❌ Failed to connect to Host. Is SuperSearch running in Developer Mode?", err.message);
        });

        const ctx = await esbuild.context({
            entryPoints: ["src/index.tsx"],
            bundle: true,
            format: "esm",
            outfile: "dist/bundle.js",
            plugins: [
                {
                    name: "supersearch-hmr",
                    setup(build) {
                        build.onEnd((result) => {
                            if (result.errors.length > 0) {
                                console.error("❌ Build failed", result.errors);
                                return;
                            }
                            console.log("⚡ Rebuilt extension. Pushing to Host...");
                            if (ws.readyState === WebSocket.OPEN) {
                                const code = fs.readFileSync("dist/bundle.js", "utf-8");
                                ws.send(JSON.stringify({ type: "hmr_update", code }));
                            }
                        });
                    },
                },
            ],
        });

        await ctx.watch();
        console.log("👀 Watching for changes...");
    });

program.parse(process.argv);
