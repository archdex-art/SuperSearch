# Releasing SuperSearch

Distribution is wired but **deactivated until you add credentials**. Everything
below except step 1 is needed only for *signed, notarized, auto-updating*
releases. Without them, `git tag vX.Y.Z && git push --tags` still produces an
**unsigned** `.app`/`.dmg` draft release via `.github/workflows/release.yml`.

## 0. What's already done
- macOS bundle config (`tauri.conf.json`): icons (`icon.icns` + PNGs), category,
  copyright, `app`+`dmg` targets.
- Updater plugin registered; `check_for_updates` IPC command; `updater:default`
  capability.
- Release workflow that builds on tag and publishes a draft GitHub Release,
  signing/notarizing automatically **if** the secrets are present.

## 1. Replace the placeholder icon (recommended)
`src-tauri/icons/icon.png` was only 32×32, so the generated `.icns` is an
upscale. Drop a **1024×1024** master at `src-tauri/icons/icon.png` and run:
```bash
cargo tauri icon src-tauri/icons/icon.png   # regenerates all sizes + icon.icns
```

## 2. Code signing + notarization (needs Apple Developer account)
Add these as **GitHub repo secrets** (Settings → Secrets → Actions):

| Secret | What it is |
|--------|-----------|
| `APPLE_CERTIFICATE` | base64 of your Developer ID Application `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | your Apple ID email |
| `APPLE_PASSWORD` | an app-specific password (appleid.apple.com) |
| `APPLE_TEAM_ID` | your 10-char Team ID |

No Team ID is hardcoded anywhere — signing activates purely from these secrets.

## 3. Auto-update (GitHub Releases channel)
Auto-update is behind the **`updater` Cargo feature** (off by default) because
the updater plugin refuses to start without `plugins.updater.pubkey`. Enabling
it is: generate keys → add the config → build with `--features updater`.

1. Generate an updater keypair:
   ```bash
   cargo tauri signer generate -w ~/.tauri/supersearch.key
   ```
2. Add the **private** key + password as secrets `TAURI_SIGNING_PRIVATE_KEY`
   and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. Add the **public** key to `tauri.conf.json` and enable updater artifacts:
   ```jsonc
   "bundle": { "createUpdaterArtifacts": true, ... },
   "plugins": {
     "updater": {
       "pubkey": "<PASTE PUBLIC KEY>",
       "endpoints": [
         "https://github.com/archdex-art/SuperSearch/releases/latest/download/latest.json"
       ]
     }
   }
   ```
4. Build release artifacts with the feature enabled, e.g. add `--features
   updater` to the release workflow's `args:` (or `cargo tauri build
   --features updater` locally).

   (Without the feature, `check_for_updates` reports "compiled without
   auto-update support" and the app runs normally — the default `cargo tauri
   dev` build does **not** register the updater, so it boots without keys.)

## 4. Cut a release
```bash
git tag v0.1.0
git push origin v0.1.0
```
The workflow builds, signs/notarizes (if secrets present), generates updater
artifacts (if keys present), and opens a **draft** GitHub Release. Review, then
publish.
