import { chromium, expect, test } from "@playwright/test";

const CDP_URL = "http://localhost:9222";
// The real Vite dev server this config's `webServer` starts (matches
// `src-tauri/tauri.conf.json`'s `devUrl`) — a full navigation here, not a
// click through the SPA's own router, so this spec starts from a clean
// session regardless of what another spec left loaded in the same
// shared, already-running app (`workers: 1` runs every smoke spec one
// after another against one `tauri dev` process).
const APP_URL = "http://localhost:5173";

test("inspects a def-override def with no live decisions and sees the winner's own effective field", async () => {
  const gameDir = process.env.RIMMERGE_SMOKE_DEFS_GAME_DIR;
  const workshopDir = process.env.RIMMERGE_SMOKE_DEFS_WORKSHOP_DIR;
  const modsConfig = process.env.RIMMERGE_SMOKE_DEFS_MODS_CONFIG;
  const profileDir = process.env.RIMMERGE_SMOKE_DEFS_PROFILE_DIR;
  if (!gameDir || !workshopDir || !modsConfig || !profileDir) {
    throw new Error("scratch defs-game paths were not set by e2e/smoke/playwright.config.ts");
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
    // See `merge.spec.ts`'s identical note: the setup page's own prefill
    // hashes the real default `ModsConfig.xml`, unrelated to this
    // scratch tree, so every scratch spec must fill this explicitly.
    await page.getByTestId("profile-dir-input").fill(profileDir);
    await page.getByTestId("load-project-button").click();

    // `ModA`/`ModB`, per `crates/rim-io/tests/fixtures/merge_game`.
    await expect(page.getByTestId("mod-count")).toHaveText("2", { timeout: 30_000 });

    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-search").fill("Fixture Mod A");
    await page.getByTestId("mod-row-fixture.moda").click();
    await expect(page).toHaveURL(/\/mods\/fixture\.moda$/);

    const changesRow = page.getByTestId("mod-changes-row-ThingDef/Fixture_Wall");
    await expect(changesRow).toBeVisible({ timeout: 15_000 });
    await changesRow.getByRole("link", { name: "ThingDef/Fixture_Wall" }).click();

    await expect(page.getByTestId("def-page-ref")).toHaveText("ThingDef/Fixture_Wall");
    const owners = page.getByTestId("def-page-owners");
    await expect(owners).toContainText("Fixture Mod A");
    await expect(owners).toContainText("Fixture Mod B");
    // `ModB` loads after `ModA` in this fixture's own `ModsConfig.xml`
    // (`crates/rim-io/tests/merge_end_to_end.rs`'s own finding), so it's
    // the winner with no decision in play.
    await expect(owners.getByTestId("def-page-winner")).toBeVisible();
    await expect(page.getByTestId("def-page-winner").locator("..")).toContainText("Fixture Mod B");

    const fields = page.getByTestId("def-page-fields");
    await expect(fields).toContainText("MaxHitPoints");
    const maxHitPointsRow = fields.locator("tr", { hasText: "MaxHitPoints" });
    // No decision has ever been made in this scratch project — the
    // effective def is exactly the winner's own raw node, so the field
    // is `Owner`-attributed to `ModB`, never a patch or inheritance.
    await expect(maxHitPointsRow).toContainText("owner");
    await expect(maxHitPointsRow).toContainText("Fixture Mod B");

    await page.getByTestId("def-page-resolved-xml").locator("summary").click();
    await expect(page.getByTestId("xml-preview")).toContainText("MaxHitPoints");
  } finally {
    await browser.close();
  }
});
