import { chromium, expect, test } from "@playwright/test";

const CDP_URL = "http://localhost:9222";
const APP_URL = "http://localhost:5173";

const PACKAGE_ID = "smoke.graphics";

test("the row editor shows the real texture of a target def, and a facing click re-reads another file", async () => {
  const gameDir = process.env.RIMMERGE_SMOKE_GRAPHICS_GAME_DIR;
  const workshopDir = process.env.RIMMERGE_SMOKE_GRAPHICS_WORKSHOP_DIR;
  const modsConfig = process.env.RIMMERGE_SMOKE_GRAPHICS_MODS_CONFIG;
  const profileDir = process.env.RIMMERGE_SMOKE_GRAPHICS_PROFILE_DIR;
  if (!gameDir || !workshopDir || !modsConfig || !profileDir) {
    throw new Error("scratch graphics-game paths were not set by e2e/smoke/playwright.config.ts");
  }

  const browser = await chromium.connectOverCDP(CDP_URL);
  const context = browser.contexts()[0];
  if (!context) {
    throw new Error("WebView2 exposes no browser context over CDP");
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
    await expect(page.getByTestId("mod-count")).toHaveText("3", { timeout: 30_000 });

    // The same patch-maker project `assignments.spec.ts` builds, over the
    // copy of that fixture whose `Race0` ships a four-direction texture set.
    await page.getByTestId("nav-assignments").click();
    await page.getByTestId("new-assignment-button").click();
    await expect(page).toHaveURL(/\/assignments\/new$/, { timeout: 15_000 });
    await page.getByTestId("assignment-name-input").fill("Example graphics smoke");
    await page.getByTestId("assignment-package-id-input").fill(PACKAGE_ID);
    await page.getByTestId("assignment-display-name-input").fill("Example Graphics Smoke");
    for (const ref of ["fixture.framework", "fixture.parts"]) {
      await page.getByTestId("refs-search").fill(ref);
      await expect(page.getByTestId(`refs-add-${ref}`)).toBeVisible({ timeout: 15_000 });
      await page.getByTestId(`refs-add-${ref}`).click();
    }
    await page.getByTestId("targets-search").fill("fixture.target");
    await expect(page.getByTestId("targets-add-fixture.target")).toBeVisible({ timeout: 15_000 });
    await page.getByTestId("targets-add-fixture.target").click();
    await page.getByTestId("propose-button").click();
    await page.getByTestId("candidate-radio-example.PartAssignmentDef").click();
    await expect(page.getByTestId("schema-row-speciesNames")).toBeVisible({ timeout: 30_000 });
    await page.getByTestId("assignment-create-button").click();
    await expect(page).toHaveURL(/\/assignments\/[0-9a-f]{12}$/, { timeout: 15_000 });

    await expect(page.getByTestId("coverage-row-Race0")).toBeVisible({ timeout: 15_000 });
    await page.getByTestId("coverage-row-Race0").click();
    await expect(page.getByTestId("row-editor")).toBeVisible();

    const viewer = page.getByTestId("def-graphic-viewer");
    await expect(viewer).toBeVisible({ timeout: 15_000 });
    // The real backend located the PNG: an <img>, never the fallback glyph.
    const image = viewer.getByTestId("def-texture-image");
    await expect(image).toBeVisible({ timeout: 15_000 });
    await expect(image).toHaveAttribute("src", /^data:image\/png;base64,/);

    // A Multi graphic offers its directions; east is a different file, so
    // the viewer re-reads and still shows an image.
    await expect(viewer.getByTestId("def-graphic-facing")).toBeVisible();
    await viewer.getByTestId("def-graphic-facing-east").click();
    await expect(viewer.getByTestId("def-graphic-facing-east")).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await expect(viewer.getByTestId("def-texture-image")).toBeVisible({ timeout: 15_000 });
  } finally {
    await browser.close();
  }
});
