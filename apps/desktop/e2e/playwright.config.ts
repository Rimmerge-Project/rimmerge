import { defineConfig } from "@playwright/test";

// The mock-IPC tier: runs against the plain Vite dev server in a real
// browser, with `@tauri-apps/api`'s IPC replaced by `mockIPC` (see
// `src/e2e-mock-bootstrap.ts` and `services/ipc.mock.ts`) — no Tauri
// binary involved. `e2e/smoke/playwright.config.ts` is the other tier,
// against the real `bun run tauri dev` build over CDP.
// The server is started with `RIMMERGE_E2E_HTTPS=1`, so it is HTTPS with a
// throwaway self-signed certificate and Vite speaks HTTP/2 (`vite.config.ts` says why:
// over HTTP/1.1 the per-test contexts' closed keep-alive sockets pile up in
// TIME_WAIT and Windows eventually refuses a module request, #58);
// `specs/http2.spec.ts` pins it.
export default defineConfig({
  testDir: "./specs",
  fullyParallel: true,
  // Bounds how many browser contexts hit the one dev server (and one
  // machine's CPU and memory) at once. On a cold server the first
  // concurrent burst of dynamic-import requests is what a small cap keeps
  // modest; `vite.config.ts`'s `server.warmup` and `optimizeDeps.include`
  // are what actually prevent the mid-transform 500s.
  workers: 4,
  forbidOnly: !!process.env.CI,
  // CI-only. The cold-server 500s are handled by `vite.config.ts`'s
  // `server.warmup` and `optimizeDeps.include`, and the blank-page
  // TIME_WAIT flake by HTTP/2 (#58). A local retry would hide a real
  // failure a developer needs to see immediately.
  retries: process.env.CI ? 1 : 0,
  reporter: "list",
  use: {
    baseURL: "https://localhost:5183",
    // The certificate is self-signed, made once and cached under `node_modules/.vite`.
    ignoreHTTPSErrors: true,
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
    env: { RIMMERGE_E2E_HTTPS: "1" },
    url: "https://localhost:5183",
    ignoreHTTPSErrors: true,
    // Never reuse a server this config did not start itself — the exact
    // hazard above. CI already ran with this off; it is now off
    // everywhere, so a local run has the identical guarantee.
    reuseExistingServer: false,
    cwd: "..",
  },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
});
