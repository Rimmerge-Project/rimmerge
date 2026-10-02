import { defineConfig } from "@playwright/test";

// The post-translation layout pass: a tour of every route in every UI
// locale, screenshotted light and dark, with an overflow audit. Not part
// of `bun run e2e` (its `testDir` is `./specs`) and not a gate: run it
// by hand, after a translation round, with
// `RIMMERGE_SCREENSHOT_DIR=<scratch dir> bun run e2e:screenshots`.
// The same config also runs `docs.spec.ts`, which regenerates the docs'
// screenshots under `docs/screenshots/` and runs only with
// `RIMMERGE_DOCS_SCREENSHOTS=1`; each spec skips without its own variable.
// Same mock-IPC tier and same dedicated port (5183, `--strictPort`) as
// `e2e/playwright.config.ts`, so never run the two at once.
export default defineConfig({
  testDir: ".",
  testMatch: /.*\.spec\.ts/,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: "list",
  timeout: 180_000,
  use: {
    baseURL: "http://localhost:5183",
    locale: "en-US",
    viewport: { width: 1280, height: 800 },
  },
  webServer: {
    command: "bun run dev -- --port 5183 --strictPort",
    url: "http://localhost:5183",
    reuseExistingServer: false,
    cwd: "../..",
  },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
});
