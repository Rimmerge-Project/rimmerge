import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

async function openModInfo(page: Page, modId: string): Promise<void> {
  await page.getByTestId("nav-mods").click();
  await page.getByTestId("mods-search").fill(modId);
  await page.getByTestId(`mod-row-${modId}`).click();
  await expect(page.getByTestId("mod-description")).toBeVisible();
}

test.describe("mods", () => {
  test("searches the mod list and opens a mod's info panel", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await expect(page).toHaveURL(/\/mods$/);
    await expect(page.getByTestId("mod-table")).toBeVisible();

    await page.getByTestId("mods-search").fill("Mod 20");
    await expect(page.getByTestId("mods-total")).toHaveText("1 mod");

    await page.getByTestId("mod-row-mod.020").click();

    await expect(page).toHaveURL(/\/mods\/mod\.020$/);
    await expect(page.getByTestId("mod-info-panel")).toBeVisible();
    await expect(page.getByTestId("mod-info-name")).toHaveText("Mod 20");
  });

  test("a one-word description has no Show more toggle", async ({ page }) => {
    await loadScenario(page);
    await openModInfo(page, "mod.020");

    await expect(page.getByTestId("mod-description-text")).toHaveText("Continued");
    await expect(page.getByTestId("mod-description-toggle")).toHaveCount(0);
  });

  test("a long description has the toggle, and expanding it grows the text", async ({ page }) => {
    await loadScenario(page);
    await openModInfo(page, "mod.021");

    const text = page.getByTestId("mod-description-text");
    const toggle = page.getByTestId("mod-description-toggle");
    await expect(toggle).toHaveText("Show more");
    const clampedHeight = await text.evaluate((element) => element.clientHeight);

    await toggle.click();

    await expect(toggle).toHaveText("Show less");
    const expandedHeight = await text.evaluate((element) => element.clientHeight);
    expect(expandedHeight).toBeGreaterThan(clampedHeight);
  });

  test("the Inactive tab resolves real names, not raw ids, and opens too", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-tab-inactive").click();

    const row = page.getByTestId("mod-row-mod.901");
    await expect(row).toContainText("Inactive Mod 1");
    await expect(row).not.toContainText("mod.901");

    await row.click();
    await expect(page).toHaveURL(/\/mods\/mod\.901$/);
    await expect(page.getByTestId("mod-info-name")).toHaveText("Inactive Mod 1");
    // No position/tier/dependents/findings sections for an inactive mod.
    await expect(page.getByTestId("mod-info-facts-placement")).not.toBeAttached();
  });

  test("a mod's findings link opens the inbox filtered to it, with a removable chip", async ({
    page,
  }) => {
    // In-app navigation throughout, not `page.goto` — a full page load
    // would reset the client-only `session.loaded` state and bounce the
    // router guard back to `/setup`.
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-search").fill("Mod 0");
    await page.getByTestId("mod-row-mod.000").click();
    await expect(page).toHaveURL(/\/mods\/mod\.000$/);

    await page.getByTestId("mod-info-findings-link").click();

    await expect(page).toHaveURL(/\/inbox\?mod=mod\.000$/);
    const chip = page.getByTestId("mod-filter-chip");
    await expect(chip).toBeVisible();
    await expect(chip).toContainText("Mod 0");

    await page.getByTestId("mod-filter-chip-clear").click();
    await expect(chip).not.toBeAttached();
  });

  test("the panel's all-details link opens the full mod detail page", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-search").fill("Mod 0");
    await page.getByTestId("mod-row-mod.000").click();
    await expect(page).toHaveURL(/\/mods\/mod\.000$/);

    await page.getByTestId("mod-info-all-details-link").click();

    await expect(page).toHaveURL(/\/mods\/mod\.000\/details$/);
    await expect(page.getByTestId("mod-detail-name")).toHaveText("Mod 0");
  });

  test("the panel's homepage link opens through the backend opener, never a URL the frontend builds", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-search").fill("Mod 0");
    await page.getByTestId("mod-row-mod.000").click();
    await expect(page).toHaveURL(/\/mods\/mod\.000$/);

    await page.getByTestId("mod-info-homepage-link").click();

    await expect
      .poll(() => page.evaluate(() => window.__OPEN_MOD_LINK_CALLS__ ?? []))
      .toEqual([{ modId: "mod.000", link: "homepage" }]);
  });

  test("closing the panel with Escape returns to the plain /mods list", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-search").fill("Mod 0");
    await page.getByTestId("mod-row-mod.000").click();
    await expect(page.getByTestId("mod-info-panel")).toBeVisible();

    // Escape only reaches the panel's own `@keydown` listener once
    // something inside it holds focus — the close button is the one
    // naturally focusable element there.
    await page.getByTestId("mod-info-panel-close").focus();
    await page.keyboard.press("Escape");

    await expect(page).toHaveURL(/\/mods$/);
    await expect(page.getByTestId("mod-info-panel")).not.toBeAttached();
  });
});
