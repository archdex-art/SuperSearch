import { useCallback, useState } from "react";
import { invoke, isTauri } from "../../bridge";
import { Button, Card, Row, SectionHeading } from "../ui";

/** Injected by Vite from `src-tauri/Cargo.toml` at build time via
 *  `define` in vite.config.ts — falls back to "dev" in browser preview. */
declare const __SUPERSEARCH_VERSION__: string;

export function AboutPane() {
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<string | null>(null);

  const checkForUpdates = useCallback(async () => {
    setChecking(true);
    setResult(null);
    try {
      if (!isTauri) {
        setResult("Updates aren't available in browser preview.");
        return;
      }
      // Ok(Some(version)) = update found, Ok(None) = up to date, Err = the
      // build wasn't compiled with the `updater` feature or isn't configured.
      const latest = await invoke<string | null>("check_for_updates");
      setResult(latest ? `Update available: v${latest}` : "You're on the latest version.");
    } catch (e) {
      setResult(String(e));
    } finally {
      setChecking(false);
    }
  }, []);

  const version = typeof __SUPERSEARCH_VERSION__ !== "undefined" ? __SUPERSEARCH_VERSION__ : "dev";

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col items-center gap-3 py-6 text-center">
        <span className="relative flex h-14 w-14 items-center justify-center rounded-2xl border border-accent/40 bg-accent/[0.07]">
          <span className="h-2.5 w-2.5 rounded-full bg-accent shadow-[0_0_10px_1px_rgb(var(--accent-rgb)/0.6)]" />
        </span>
        <div className="flex flex-col gap-0.5">
          <span className="text-[16px] font-semibold text-white/95">SuperSearch</span>
          <span className="font-mono text-[12px] text-white/40">v{version}</span>
        </div>
      </div>

      <div>
        <SectionHeading>Updates</SectionHeading>
        <Card>
          <Row>
            <span className="text-[13.5px] text-white/80">{result ?? "Check for the latest release"}</span>
            <Button onClick={checkForUpdates} disabled={checking}>
              {checking ? "Checking…" : "Check now"}
            </Button>
          </Row>
        </Card>
      </div>

      <div>
        <SectionHeading>Links</SectionHeading>
        <Card>
          {[
            { label: "GitHub Repository", url: "https://github.com/archdex-art/SuperSearch" },
            { label: "Changelog", url: "https://github.com/archdex-art/SuperSearch/blob/main/CHANGELOG.md" },
            { label: "Report an Issue", url: "https://github.com/archdex-art/SuperSearch/issues" },
          ].map((link) => (
            <Row key={link.url}>
              <span className="text-[13.5px] text-white/80">{link.label}</span>
              <a
                href={link.url}
                target="_blank"
                rel="noreferrer"
                className="text-[12.5px] text-accent/80 hover:text-accent"
              >
                Open ↗
              </a>
            </Row>
          ))}
        </Card>
      </div>
    </div>
  );
}
