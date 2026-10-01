import { defineConfig } from "@playwright/test";

import { buildScratchProfileOnly, writeScratchAppSettingsNetworkOff } from "../scratch-game";

// A dedicated CDP smoke config — never folded into
// `e2e/smoke/playwright.config.ts`'s shared `webServer`, which every
// other smoke spec attaches to over one already-running window: that
// window's `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` is fixed for the
// whole shared run, so a language override here would force every
// other spec's window into zh-CN too. This config starts its own
// separate `tauri dev` on its own CDP port instead, verifying that
// WebView2 reports the OS display language via `navigator.languages`
// (so `detectLocale` needs no `tauri-plugin-os` dependency) and that
// vue-i18n's runtime message compiler works under this app's real
// `script-src 'self'` CSP (no console violation).
const scratch = buildScratchProfileOnly();
// Belt and braces alongside the first-run gate — see
// `writeScratchAppSettingsNetworkOff`'s own doc comment and the shared
// smoke config's identical call, which this one had been missing.
writeScratchAppSettingsNetworkOff(scratch.profileDir);
const CDP_PORT = 9223;

export default defineConfig({
  testDir: ".",
  testMatch: /.*\.spec\.ts/,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: "list",
  timeout: 90_000,
  globalTeardown: "../global-teardown.ts",
  webServer: {
    command: "bun run tauri dev",
    url: `http://localhost:${CDP_PORT}/json/version`,
    reuseExistingServer: false,
    cwd: "../../..",
    timeout: 180_000,
    env: {
      RIMMERGE_PROFILE_DIR: scratch.profileDir,
      // `--lang` is a Chromium/WebView2 flag, not an OS setting — it
      // only affects this one launched window, and is removed with the
      // process. Combined with `--remote-debugging-port` the same way
      // the shared smoke config combines its own CDP flag.
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--lang=zh-CN --remote-debugging-port=${CDP_PORT}`,
    },
  },
  projects: [{ name: "i18n-smoke" }],
});
