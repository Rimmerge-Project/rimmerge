import { readdirSync } from "node:fs";
import path from "node:path";
import { chromium, expect, test } from "@playwright/test";

const CDP_URL = "http://localhost:9222";
// A full navigation here, not a click through the SPA's own router — see
// `patch.spec.ts`'s identical note. A fresh `tauri dev` launch starts at
// `/setup` with no project loaded, but a spec file that sorts
// alphabetically before this one can leave the shared window on one of
// its own routes, so every smoke spec navigates explicitly rather than
// relying on being the *first* spec to touch the shared window.
const APP_URL = "http://localhost:5173";

test("loads the scratch project through the real backend and renders the dashboard", async () => {
  const gameDir = process.env.RIMMERGE_SMOKE_GAME_DIR;
  const workshopDir = process.env.RIMMERGE_SMOKE_WORKSHOP_DIR;
  const modsConfig = process.env.RIMMERGE_SMOKE_MODS_CONFIG;
  const profileDir = process.env.RIMMERGE_SMOKE_PROFILE_DIR;
  if (!gameDir || !workshopDir || !modsConfig || !profileDir) {
    throw new Error("scratch game paths were not set by e2e/smoke/playwright.config.ts");
  }

  const browser = await chromium.connectOverCDP(CDP_URL);
  const context = browser.contexts()[0];
  if (!context) {
    throw new Error("WebView2 exposed no browser context over CDP");
  }
  const page = context.pages()[0] ?? (await context.waitForEvent("page"));
  await page.waitForLoadState();

  try {
    await page.goto(`${APP_URL}/setup`);
    await expect(page.getByTestId("game-dir-input")).toBeVisible({ timeout: 30_000 });

    await page.getByTestId("game-dir-input").fill(gameDir);
    await page.getByTestId("workshop-dir-input").fill(workshopDir);
    await page.getByTestId("mods-config-input").fill(modsConfig);
    // The setup page prefills this from `get_default_paths`, which hashes
    // the *real* default `ModsConfig.xml` path — unrelated to this
    // scratch tree, and identical to whatever `merge.spec.ts` sees. Left
    // unfilled, both specs would read and write
    // `decisions.json`/`rules.json` in the same directory.
    await page.getByTestId("profile-dir-input").fill(profileDir);

    await page.getByTestId("load-project-button").click();

    await expect(page.getByTestId("mod-count")).toHaveText("1", { timeout: 30_000 });

    // The order view and inbox against the real backend — no mocks, so
    // this is the one place that proves `list_order`/`list_findings`
    // actually round-trip through a live `rim-session::Session` and
    // render, not just that `get_dashboard` does.
    await page.getByTestId("nav-order").click();
    await expect(page.getByTestId("order-table")).toBeVisible({ timeout: 30_000 });
    await expect(page.locator('[data-testid^="order-row-"]')).toHaveCount(1);

    await page.getByTestId("nav-inbox").click();
    await expect(page.getByTestId("finding-list")).toBeVisible({ timeout: 30_000 });

    // Apply for real, against the scratch project's own `ModsConfig.xml`
    // copy (never the real game install or a real profile) — proves
    // `apply` round-trips through a live `rim-session::Session` +
    // `rim-io::ModsConfigFileStore` and actually creates a backup file
    // next to it.
    await page.getByTestId("shell-apply-button").click();
    await expect(page.getByTestId("apply-dialog")).toBeVisible({ timeout: 10_000 });

    // `apply-dialog-anyway-checkbox` (when rendered at all) starts
    // checked by default, so submit is already enabled here.
    await page.getByTestId("apply-dialog-submit").click();
    await expect(page.getByTestId("apply-dialog")).toBeHidden({ timeout: 30_000 });

    const gameDirEntries = readdirSync(path.dirname(modsConfig));
    const backupFile = gameDirEntries.find((name) => name.includes(".bak-"));
    expect(
      backupFile,
      `expected a ModsConfig.xml.bak-* file in ${gameDirEntries.join(", ")}`,
    ).toBeTruthy();
  } finally {
    await browser.close();
  }
});
