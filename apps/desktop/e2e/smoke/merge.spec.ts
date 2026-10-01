import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { chromium, expect, test } from "@playwright/test";

const CDP_URL = "http://localhost:9222";
// The real Vite dev server this config's `webServer` starts (matches
// `src-tauri/tauri.conf.json`'s `devUrl`) — a full navigation here, not a
// click through the SPA's own router, so this spec starts from a clean
// session regardless of what `dashboard.spec.ts` left loaded in the same
// shared, already-running app (both spec files attach to one `tauri dev`
// process over CDP; `workers: 1` runs them one after another).
const APP_URL = "http://localhost:5173";

test("decides a def-override merge, applies with Write merge mod, and the merge mod lands on disk", async () => {
  const gameDir = process.env.RIMMERGE_SMOKE_MERGE_GAME_DIR;
  const workshopDir = process.env.RIMMERGE_SMOKE_MERGE_WORKSHOP_DIR;
  const modsConfig = process.env.RIMMERGE_SMOKE_MERGE_MODS_CONFIG;
  const profileDir = process.env.RIMMERGE_SMOKE_MERGE_PROFILE_DIR;
  if (!gameDir || !workshopDir || !modsConfig || !profileDir) {
    throw new Error("scratch merge-game paths were not set by e2e/smoke/playwright.config.ts");
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
    // scratch tree, and identical to whatever `dashboard.spec.ts` sees.
    // Left unfilled, both specs would read and write
    // `decisions.json`/`rules.json` in the same directory.
    await page.getByTestId("profile-dir-input").fill(profileDir);
    await page.getByTestId("load-project-button").click();

    // `ModA`/`ModB`, per `crates/rim-io/tests/fixtures/merge_game`.
    await expect(page.getByTestId("mod-count")).toHaveText("2", { timeout: 30_000 });

    await page.getByTestId("nav-inbox").click();
    await expect(page.getByTestId("finding-list")).toBeVisible({ timeout: 30_000 });
    // Both fixture mods share `<author>Fixture Author</author>` — the
    // ledger's `same_author` signal gives this DefOverride confidence 90,
    // at or above the default threshold (80), so it resolves `Auto` and
    // is hidden by the inbox's default "needs input" status filter.
    // Clearing the status filter to "all" is what a real user would do
    // to find it too.
    await page.getByTestId("status-chip-all").click();
    await page.getByTestId("finding-search").fill("Fixture_Wall");
    // Both fixture mods also ship a same-path `Fixture_Wall.png`, so a
    // bare "Fixture_Wall" search matches two cards — this def override
    // *and* the resulting texture-override conflict, both real findings
    // in this fixture. The `def_override:` testid prefix disambiguates.
    const wallDefOverride = page.locator(
      '[data-testid^="finding-card-def_override:ThingDef/Fixture_Wall"]',
    );
    await expect(wallDefOverride).toHaveCount(1, { timeout: 15_000 });
    await wallDefOverride.first().click();
    await expect(page.getByTestId("suggestion-panel")).toBeVisible({ timeout: 15_000 });

    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//, { timeout: 15_000 });
    await expect(page.getByTestId("merge-header-def-key")).toHaveText("ThingDef/Fixture_Wall");

    // `statBases/MaxHitPoints` starts auto-resolved toward `fixture.modb`
    // (the selected order's winner, loaded after `fixture.moda` per
    // `ModsConfig.xml`) — collapsed under "Auto-resolved" by default, per
    // `crates/rim-io/tests/merge_end_to_end.rs`'s own finding ("with no
    // stored choices, MaxHitPoints is OneSided toward ModB -> a no-op").
    // Picking `fixture.moda`'s value instead is this test's one real
    // choice, so the generated patch carries an actual op.
    await page.getByTestId("merge-section-auto").click();
    const maxHitPointsFromModA = page.getByTestId(
      "merge-radio-statBases/MaxHitPoints-fixture.moda",
    );
    await maxHitPointsFromModA.check();
    await expect(maxHitPointsFromModA).toBeChecked({ timeout: 15_000 });

    await page.getByTestId("shell-apply-button").click();
    await expect(page.getByTestId("apply-dialog")).toBeVisible({ timeout: 10_000 });
    // `apply-dialog-anyway-checkbox` (when rendered at all) starts
    // checked by default, so submit is already enabled here.
    await expect(page.getByTestId("apply-dialog-write-merge-mod-checkbox")).toHaveAttribute(
      "data-p-checked",
      "false",
    );
    await page.getByTestId("apply-dialog-write-merge-mod-checkbox").click();
    await expect(page.getByTestId("apply-dialog-write-merge-mod-checkbox")).toHaveAttribute(
      "data-p-checked",
      "true",
    );
    await page.getByTestId("apply-dialog-submit").click();
    await expect(page.getByTestId("apply-dialog-merge-mod-path")).toBeVisible({ timeout: 30_000 });
    await page.getByTestId("apply-dialog-close-summary").click();
    await expect(page.getByTestId("apply-dialog")).toBeHidden({ timeout: 10_000 });

    const modsDir = path.join(gameDir, "Mods");
    const mergeModFolder = readdirSync(modsDir).find((name) => name.startsWith("rimmerge_merge_"));
    expect(mergeModFolder, `expected a rimmerge_merge_* folder under ${modsDir}`).toBeTruthy();
    const mergeModPath = path.join(modsDir, mergeModFolder as string);

    const aboutXml = readFileSync(path.join(mergeModPath, "About", "About.xml"), "utf-8");
    expect(aboutXml).toContain("<packageId>rimmerge.merge.");

    const patchXml = readFileSync(
      path.join(mergeModPath, "Patches", "rimmerge_ThingDef.xml"),
      "utf-8",
    );
    expect(patchXml).toContain("MaxHitPoints");

    const packageId = aboutXml.match(/<packageId>([^<]+)<\/packageId>/)?.[1];
    expect(packageId, "About.xml must declare its own packageId").toBeTruthy();
    const modsConfigXml = readFileSync(modsConfig, "utf-8");
    const mergeIdPosition = modsConfigXml.indexOf(`<li>${packageId}</li>`);
    expect(
      mergeIdPosition,
      `ModsConfig.xml must list the merge mod id: ${modsConfigXml}`,
    ).toBeGreaterThan(-1);
    for (const sourceId of ["fixture.moda", "fixture.modb"]) {
      const sourcePosition = modsConfigXml.indexOf(`<li>${sourceId}</li>`);
      expect(sourcePosition).toBeGreaterThan(-1);
      expect(
        mergeIdPosition,
        `the merge mod id must be appended after ${sourceId}: ${modsConfigXml}`,
      ).toBeGreaterThan(sourcePosition);
    }
  } finally {
    await browser.close();
  }
});
