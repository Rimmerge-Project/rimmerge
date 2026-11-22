import { expect, type Page, test } from "@playwright/test";
import { chooseLocale, chooseSort, loadScenario, modsPane } from "./support";

type Language = { readonly locale: "en" | "de"; readonly pendingText: RegExp };

const LANGUAGES: readonly Language[] = [
  { locale: "en", pendingText: /not scanned yet/ },
  { locale: "de", pendingText: /noch nicht gescannt/ },
];
// 1024 is the narrowest width where the two lists sit side by side (`lg`).
const WIDTHS = [1280, 1024] as const;
const SORTS = ["loadOrder", "lastUpdated"] as const;

/** Activates a mod without a rescan, so its row is Added and "not scanned yet". */
async function activateWithoutRescan(page: Page, modId: string): Promise<void> {
  await modsPane(page, "inactive").getByTestId("mods-search").fill(modId);
  await modsPane(page, "inactive").getByTestId(`mod-row-select-${modId}`).locator("input").check();
  await page.getByTestId("activate-selected-button").click();
  await page.getByTestId("activate-confirm-submit").click();
  await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();
}

/** The row's name keeps room to show (ellipsized is fine) and its menu button lies inside the row. */
async function expectRowFits(page: Page, modId: string): Promise<void> {
  const pane = modsPane(page, "active");
  await pane.getByTestId("mods-search").fill(modId);
  const row = pane.getByTestId(`mod-row-${modId}`);
  // Measure only once the search answer has replaced the old rows: the table is
  // rebuilt when the debounced search lands, and a row caught before that has no box.
  await expect(pane.locator('[data-testid^="mod-row-mod."]')).toHaveCount(1);
  await expect(row).toBeVisible();

  const rowBox = await row.boundingBox();
  const nameBox = await pane.getByTestId(`mod-row-name-${modId}`).boundingBox();
  const menuBox = await pane.getByTestId(`mod-row-menu-${modId}`).boundingBox();
  if (rowBox === null || nameBox === null || menuBox === null) {
    throw new Error(`row ${modId} is missing its name or menu button`);
  }
  expect(nameBox.width).toBeGreaterThanOrEqual(40);
  expect(menuBox.x).toBeGreaterThanOrEqual(rowBox.x);
  expect(menuBox.x + menuBox.width).toBeLessThanOrEqual(rowBox.x + rowBox.width);
}

for (const { locale, pendingText } of LANGUAGES) {
  for (const width of WIDTHS) {
    test(`rows keep their name and menu inside the pane (${locale}, ${width}px)`, async ({
      page,
    }) => {
      await page.setViewportSize({ width, height: 800 });
      await loadScenario(page);
      await page.getByTestId("welcome-card-keep").click();
      await chooseLocale(page, locale);
      await page.getByTestId("nav-mods").click();
      await activateWithoutRescan(page, "mod.905");

      for (const sort of SORTS) {
        await chooseSort(page, "active", sort);
        // A scanned row, and the row with the Added mark and the pending-scan pill.
        await expectRowFits(page, "mod.004");
        await expectRowFits(page, "mod.905");
        await expect(page.getByTestId("membership-badge")).toBeVisible();
        const pill = page.getByTestId("mod-row-pending-scan-pill");
        await expect(pill).toBeVisible();
        await expect(pill).toHaveAccessibleName(pendingText);
        await expect(page.getByTestId("mod-row-mod.905")).toHaveAccessibleName(pendingText);
      }
    });
  }
}
