import { mkdtempSync, readFileSync, statSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { chromium, expect, test } from "@playwright/test";

const CDP_URL = "http://localhost:9222";
// The real Vite dev server this config's `webServer` starts (matches
// `src-tauri/tauri.conf.json`'s `devUrl`) — a full navigation here, not a
// click through the SPA's own router, so this spec starts from a clean
// session regardless of what `dashboard.spec.ts`/`merge.spec.ts` left
// loaded in the same shared, already-running app (all three spec files
// attach to one `tauri dev` process over CDP; `workers: 1` runs them one
// after another).
const APP_URL = "http://localhost:5173";

const MOD_A = "fixture.moda";
const MOD_B = "fixture.modb";
const PACKAGE_ID = "smoke.wallpatch";
const FOLDER_NAME = PACKAGE_ID.replaceAll(".", "_");

test("creates a patch scoped to ModA+ModB, merges Fixture_Wall, and exports a reproducible compat patch mod — never touching the real install", async () => {
  const gameDir = process.env.RIMMERGE_SMOKE_PATCH_GAME_DIR;
  const workshopDir = process.env.RIMMERGE_SMOKE_PATCH_WORKSHOP_DIR;
  const modsConfig = process.env.RIMMERGE_SMOKE_PATCH_MODS_CONFIG;
  const profileDir = process.env.RIMMERGE_SMOKE_PATCH_PROFILE_DIR;
  if (!gameDir || !workshopDir || !modsConfig || !profileDir) {
    throw new Error("scratch merge-game paths were not set by e2e/smoke/playwright.config.ts");
  }
  // Under the OS temp dir, prefixed `rimmerge-smoke-` like every other
  // scratch tree this suite builds — `global-teardown.ts`'s prefix sweep
  // removes it too, without this spec needing its own cleanup.
  const exportRoot = mkdtempSync(path.join(os.tmpdir(), "rimmerge-smoke-patch-export-"));

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
    // See `merge.spec.ts`'s identical note: left unfilled, this would
    // read/write `decisions.json`/`patches/` in the same directory every
    // other smoke spec's default (`get_default_paths`, hashing the real
    // `ModsConfig.xml`) resolves to.
    await page.getByTestId("profile-dir-input").fill(profileDir);
    await page.getByTestId("load-project-button").click();

    // `ModA`/`ModB`, per `crates/rim-io/tests/fixtures/merge_game`.
    await expect(page.getByTestId("mod-count")).toHaveText("2", { timeout: 30_000 });

    await page.getByTestId("nav-patches").click();
    await expect(page).toHaveURL(/\/patches$/, { timeout: 15_000 });
    await page.keyboard.press("n");
    await expect(page.getByTestId("patch-create-form")).toBeVisible();

    await page.getByTestId("patch-name-input").fill("Wall smoke patch");
    await page.getByTestId("patch-package-id-input").fill(PACKAGE_ID);
    await page.getByTestId("patch-display-name-input").fill("Wall Smoke Patch");

    for (const modId of [MOD_A, MOD_B]) {
      await page.getByTestId("scope-search").fill(modId);
      await expect(page.getByTestId(`scope-add-${modId}`)).toBeVisible({ timeout: 15_000 });
      await page.getByTestId(`scope-add-${modId}`).click();
    }
    await page.getByTestId("patch-save-button").click();

    await expect(page).toHaveURL(/\/patches\/[0-9a-f]{12}$/, { timeout: 15_000 });
    const patchId = page.url().split("/").pop() ?? "";

    // Both fixture mods share `<author>Fixture Author</author>` — same
    // as `merge.spec.ts`'s own note, `ledger::scoped`'s status derives
    // from the same confidence/threshold rule, so this DefOverride is
    // `Auto` and hidden by the scoped inbox's default "needs input"
    // status filter too.
    const patchInbox = page.getByTestId("patch-inbox");
    await patchInbox.getByTestId("status-chip-all").click();
    await patchInbox.getByTestId("finding-search").fill("Fixture_Wall");
    // Both fixture mods also ship a same-path `Fixture_Wall.png` — fully
    // inside this patch's scope too — so a bare "Fixture_Wall" search
    // matches two cards here as well (see `merge.spec.ts`'s identical
    // note). The `def_override:` testid prefix disambiguates.
    const wallDefOverride = patchInbox.locator(
      '[data-testid^="finding-card-def_override:ThingDef/Fixture_Wall"]',
    );
    await expect(wallDefOverride).toHaveCount(1, { timeout: 15_000 });
    await wallDefOverride.first().click();
    await expect(page.getByTestId("suggestion-panel")).toBeVisible({ timeout: 15_000 });

    await page.keyboard.press("m");
    await expect(page).toHaveURL(new RegExp(`/patches/${patchId}/merge/`), { timeout: 15_000 });
    await expect(page.getByTestId("merge-header-def-key")).toHaveText("ThingDef/Fixture_Wall");
    // Both of `Fixture_Wall`'s owners are inside this patch's own scope.
    await expect(page.getByTestId("merge-header-out-of-scope")).toHaveCount(0);

    // Same one real choice `merge.spec.ts` makes: `statBases/MaxHitPoints`
    // starts auto-resolved toward `fixture.modb` (a no-op) — picking
    // `fixture.moda`'s value instead gives the exported patch an actual
    // op to carry.
    await page.getByTestId("merge-section-auto").click();
    const maxHitPointsFromModA = page.getByTestId(
      "merge-radio-statBases/MaxHitPoints-fixture.moda",
    );
    await maxHitPointsFromModA.check();
    await expect(maxHitPointsFromModA).toBeChecked({ timeout: 15_000 });

    // `isInert` (shared by every `useXKeys` composable) treats a focused
    // `<input>` — including the radio `.check()` just landed focus on —
    // as a place to type, not a place a global shortcut fires; move
    // focus off it first so `Escape` actually reaches `useMergeKeys`.
    await page.getByTestId("merge-header-def-key").click();
    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(new RegExp(`/patches/${patchId}$`), { timeout: 15_000 });

    // Export — `install` stays unchecked throughout this spec: nothing
    // under `gameDir/Mods` or `modsConfig` is ever touched.
    await page.getByTestId("patch-export-dir-input").fill(exportRoot);
    await page.getByTestId("patch-export-button").click();
    await expect(page.getByTestId("patch-export-last-path")).toBeVisible({ timeout: 30_000 });

    const exportedModDir = path.join(exportRoot, FOLDER_NAME);
    const aboutXml = readFileSync(path.join(exportedModDir, "About", "About.xml"), "utf-8");
    expect(aboutXml).toContain(`<packageId>${PACKAGE_ID}</packageId>`);
    expect(aboutXml).toContain(`<packageId>${MOD_A}</packageId>`);
    expect(aboutXml).toContain(`<packageId>${MOD_B}</packageId>`);

    const patchXml = readFileSync(
      path.join(exportedModDir, "Patches", "rimmerge_ThingDef.xml"),
      "utf-8",
    );
    expect(patchXml).toContain("MaxHitPoints");

    const rimmergeJson = readFileSync(path.join(exportedModDir, "rimmerge.json"), "utf-8");
    expect(rimmergeJson).toContain('"kind":"patch"');

    // Export again with the same decisions and scope — every file must
    // come out byte-identical ("reproducible
    // from the same decisions" requirement; `rimmerge.json` deliberately
    // carries no timestamp for a compat patch, unlike the profile merge
    // mod's). Content is identical by construction, so it can't itself
    // signal that the second write has happened — the atomic swap's own
    // rename still bumps the file's mtime, which a poll here can wait on
    // instead of guessing at a UI-level "done" signal.
    const aboutPath = path.join(exportedModDir, "About", "About.xml");
    const patchXmlPath = path.join(exportedModDir, "Patches", "rimmerge_ThingDef.xml");
    const rimmergeJsonPath = path.join(exportedModDir, "rimmerge.json");
    const beforeSecondExport = {
      about: readFileSync(aboutPath),
      patch: readFileSync(patchXmlPath),
      marker: readFileSync(rimmergeJsonPath),
    };
    const aboutMtimeBefore = statSync(aboutPath).mtimeMs;

    await page.getByTestId("patch-export-button").click();
    await expect
      .poll(() => statSync(aboutPath).mtimeMs, { timeout: 30_000 })
      .toBeGreaterThan(aboutMtimeBefore);

    expect(readFileSync(aboutPath)).toEqual(beforeSecondExport.about);
    expect(readFileSync(patchXmlPath)).toEqual(beforeSecondExport.patch);
    expect(readFileSync(rimmergeJsonPath)).toEqual(beforeSecondExport.marker);
  } finally {
    await browser.close();
  }
});
