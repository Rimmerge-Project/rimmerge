import { mkdtempSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { chromium, expect, test } from "@playwright/test";

const CDP_URL = "http://localhost:9222";
const APP_URL = "http://localhost:5173";

const PACKAGE_ID = "smoke.parts";
const FOLDER_NAME = PACKAGE_ID.replaceAll(".", "_");

test("creates a patch-maker project from the assign_game fixture, sets a row, and exports a reproducible Defs/ mod — never touching the real install", async () => {
  const gameDir = process.env.RIMMERGE_SMOKE_ASSIGN_GAME_DIR;
  const workshopDir = process.env.RIMMERGE_SMOKE_ASSIGN_WORKSHOP_DIR;
  const modsConfig = process.env.RIMMERGE_SMOKE_ASSIGN_MODS_CONFIG;
  const profileDir = process.env.RIMMERGE_SMOKE_ASSIGN_PROFILE_DIR;
  if (!gameDir || !workshopDir || !modsConfig || !profileDir) {
    throw new Error("scratch assign-game paths were not set by e2e/smoke/playwright.config.ts");
  }
  const exportRoot = mkdtempSync(path.join(os.tmpdir(), "rimmerge-smoke-assign-export-"));

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
    await page.getByTestId("profile-dir-input").fill(profileDir);
    await page.getByTestId("load-project-button").click();

    // `fixture.framework`/`fixture.parts`/`fixture.target`, per
    // `crates/rim-io/tests/fixtures/assign_game`.
    await expect(page.getByTestId("mod-count")).toHaveText("3", { timeout: 30_000 });

    await page.getByTestId("nav-assignments").click();
    await expect(page).toHaveURL(/\/assignments$/, { timeout: 15_000 });
    await page.getByTestId("new-assignment-button").click();
    await expect(page).toHaveURL(/\/assignments\/new$/, { timeout: 15_000 });

    await page.getByTestId("assignment-name-input").fill("Example smoke patch");
    await page.getByTestId("assignment-package-id-input").fill(PACKAGE_ID);
    await page.getByTestId("assignment-display-name-input").fill("Example Smoke Patch");

    // Both `fixture.framework` and `fixture.parts` must be selected as
    // refs: `fixture.framework`'s own `About.xml` declares no
    // `modDependencies` on `fixture.parts` (see the fixture itself), so
    // the effective-refs closure never pulls it in on its own — selecting
    // `fixture.framework` alone would classify `parts` as a `TargetKey`
    // instead of an `ItemSlot` (rule 3's "inside R" test fails once the
    // slot's own owner isn't in R), and this spec needs a real item
    // picker to exercise.
    for (const ref of ["fixture.framework", "fixture.parts"]) {
      await page.getByTestId("refs-search").fill(ref);
      await expect(page.getByTestId(`refs-add-${ref}`)).toBeVisible({ timeout: 15_000 });
      await page.getByTestId(`refs-add-${ref}`).click();
    }

    await page.getByTestId("targets-search").fill("fixture.target");
    await expect(page.getByTestId("targets-add-fixture.target")).toBeVisible({ timeout: 15_000 });
    await page.getByTestId("targets-add-fixture.target").click();

    await page.getByTestId("propose-button").click();
    // Phase 1 (`list_assignment_candidates`): the cheap summary listing.
    // `example.PartAssignmentDef` is the one candidate type this fixture yields.
    await expect(page.getByTestId("candidate-radio-example.PartAssignmentDef")).toBeVisible({
      timeout: 30_000,
    });
    // Phase 2 (`infer_assignment_candidate`): the real per-type inference
    // for the type just picked (the verified classification:
    // `speciesNames`/`parts`/`enabled`).
    await page.getByTestId("candidate-radio-example.PartAssignmentDef").click();
    await expect(page.getByTestId("schema-row-speciesNames")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("schema-row-parts")).toBeVisible();
    await expect(page.getByTestId("schema-row-enabled")).toBeVisible();

    await page.getByTestId("assignment-create-button").click();
    await expect(page).toHaveURL(/\/assignments\/[0-9a-f]{12}$/, { timeout: 15_000 });

    await expect(page.getByTestId("coverage-row-Race0")).toBeVisible({ timeout: 15_000 });
    await page.getByTestId("coverage-row-Race0").click();
    await expect(page.getByTestId("row-editor")).toBeVisible();

    await page
      .getByTestId("item-picker-checkbox-example.PartDef-Part0")
      .waitFor({ state: "visible", timeout: 15_000 });
    await page.getByTestId("item-picker-checkbox-example.PartDef-Part0").check();
    await page.getByTestId("row-editor-save-button").click();
    await expect(page.getByTestId("coverage-cell-Race0")).toHaveText("this project", {
      timeout: 15_000,
    });

    await page.getByTestId("assignment-export-dir-input").fill(exportRoot);
    await page.getByTestId("assignment-export-button").click();
    await expect(page.getByTestId("assignment-export-last-path")).toBeVisible({
      timeout: 30_000,
    });

    const exportedModDir = path.join(exportRoot, FOLDER_NAME);
    const aboutXml = readFileSync(path.join(exportedModDir, "About", "About.xml"), "utf-8");
    expect(aboutXml).toContain(`<packageId>${PACKAGE_ID}</packageId>`);
    // `modDependencies` names the owner of every item a row actually
    // chose (`fixture.parts`, since the row names `Part0`) — never a
    // pure reference mod with no row-content dependency of its own:
    // `fixture.framework` ships no DLL (the type's own "framework"
    // signal, `dll_owner_of`, is silent), so it's correctly absent here,
    // matching the dependency rule the `rim-session` fixture tests also
    // verify.
    expect(aboutXml).toContain("<packageId>fixture.parts</packageId>");
    expect(aboutXml).not.toContain("<packageId>fixture.framework</packageId>");
    expect(aboutXml).toContain("<li>fixture.target</li>");

    const defsXml = readFileSync(
      path.join(exportedModDir, "Defs", "rimmerge_example.PartAssignmentDef.xml"),
      "utf-8",
    );
    expect(defsXml).toContain("Race0");
    expect(defsXml).toContain("Part0");

    const rimmergeJson = readFileSync(path.join(exportedModDir, "rimmerge.json"), "utf-8");
    expect(rimmergeJson).toContain('"kind":"assignment"');
  } finally {
    await browser.close();
  }
});
