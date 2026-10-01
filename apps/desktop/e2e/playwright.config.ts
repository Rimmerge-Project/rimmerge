import { defineConfig } from "@playwright/test";

// The mock-IPC tier: runs against the plain Vite dev server in a real
// browser, with `@tauri-apps/api`'s IPC replaced by `mockIPC` (see
// `src/e2e-mock-bootstrap.ts` and `services/ipc.mock.ts`) — no Tauri
// binary involved. `e2e/smoke/playwright.config.ts` is the other tier,
// against the real `bun run tauri dev` build over CDP.
export default defineConfig({
  testDir: "./specs",
  fullyParallel: true,
  // A wide-open worker count throws many browser contexts at Vite's dev
  // server at once; on a cold server (freshly started, nothing
  // transformed yet — every CI run, and any local run right after
  // editing source) that first concurrent burst of dynamic-import
  // requests can occasionally 500 mid-transform, breaking the app's
  // very first route resolution for whichever test hit it. Capping
  // workers keeps the burst small enough that Vite's transform cache is
  // warm well before the tail of the suite runs.
  workers: 4,
  forbidOnly: !!process.env.CI,
  // CI-only: the cold-dev-server flake above is now addressed directly,
  // at the source, by `vite.config.ts`'s `server.warmup` and
  // `optimizeDeps.include` — a local retry would just paper back over a
  // real failure a developer needs to see immediately.
  retries: process.env.CI ? 1 : 0,
  reporter: "list",
  use: {
    baseURL: "http://localhost:5183",
    trace: "on-first-retry",
    // Pins `navigator.languages` so `detectLocale` (`src/i18n/locales.ts`)
    // resolves to `en` deterministically on a zh-CN or pt-BR developer
    // machine — every existing English text assertion across this
    // suite's specs stays valid regardless of the host OS's own
    // language. Each context also starts with empty `localStorage`, so
    // no persisted `rimmerge.locale` leaks between tests either.
    locale: "en-US",
  },
  webServer: {
    // A dedicated port, never 5173 (the one `vite.config.ts` pins for
    // `tauri dev`'s own `devUrl`): an unrelated Vite dev server already
    // listening on 5173 would otherwise make every test in this suite
    // silently run against *that* app instead and time out — this tier must
    // start and own its own server, never attach to whatever else happens
    // to be on the shared default port.
    // `--strictPort` makes Vite exit immediately, non-zero, if 5183 is
    // already taken (instead of silently picking another port the way
    // it does by default), so a busy port fails this config fast with
    // Playwright's own "Process from config.webServer was not able to
    // start" message rather than reconnecting to a foreign server.
    command: "bun run dev -- --port 5183 --strictPort",
    url: "http://localhost:5183",
    // Never reuse a server this config did not start itself — the exact
    // hazard above. CI already ran with this off; it is now off
    // everywhere, so a local run has the identical guarantee.
    reuseExistingServer: false,
    cwd: "..",
  },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
});
