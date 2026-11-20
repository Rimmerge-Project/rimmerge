import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario, modsPane } from "./support";

const EXTRA_INACTIVE = 250;
// The scenario's own ten inactive mods plus the extras a spec asks for.
const INACTIVE_TOTAL = 10 + EXTRA_INACTIVE;

async function openMods(page: Page): Promise<void> {
  await page.getByTestId("nav-mods").click();
  await expect(modsPane(page, "active").getByTestId("mod-table")).toBeVisible();
}

test.describe("mods selection", () => {
  test("Select all on the Inactive list covers every row, past the old 200-row page", async ({
    page,
  }) => {
    await page.addInitScript((extra) => {
      window.__EXTRA_INACTIVE_MODS__ = extra;
    }, EXTRA_INACTIVE);
    await loadScenario(page);
    await openMods(page);

    await expect(modsPane(page, "inactive").getByTestId("mods-total")).toContainText(
      `${INACTIVE_TOTAL} mods`,
    );
    await expect(modsPane(page, "inactive").getByTestId("mods-page-cap-hint")).toHaveCount(0);
    await expect(modsPane(page, "inactive").getByTestId("mods-select-all-label")).toHaveText(
      `Select all ${INACTIVE_TOTAL} shown`,
    );

    await modsPane(page, "inactive").getByTestId("mods-select-all").locator("input").check();
    await expect(modsPane(page, "inactive").getByTestId("mods-selected-count")).toContainText(
      `${INACTIVE_TOTAL} selected`,
    );
    await page.getByTestId("activate-selected-button").click();

    await expect(page.getByTestId("activate-confirm-dialog")).toBeVisible();
    await expect(page.getByTestId("activate-plan-to-add")).toHaveCount(INACTIVE_TOTAL);
    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();

    const calls = await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? []);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.ids).toHaveLength(INACTIVE_TOTAL);
    await expect(page.getByTestId("pending-changes-banner")).toBeVisible();
  });

  test("Select all on the Active list sends every id and the plan refuses Core", async ({
    page,
  }) => {
    await loadScenario(page);
    await openMods(page);

    await modsPane(page, "active").getByTestId("mods-select-all").locator("input").check();
    await page.getByTestId("deactivate-selected-button").click();

    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeVisible();
    await expect(page.getByTestId("deactivate-plan-refused")).toHaveCount(1);
    await page.getByTestId("deactivate-confirm-submit").click();
    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeHidden();

    const calls = await page.evaluate(() => window.__DEACTIVATE_CALLS__ ?? []);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.ids).toContain("ludeon.rimworld");
    expect(calls[0]?.ids.length).toBeGreaterThan(50);
  });

  test("with a filter, Select all picks only the matching rows and says so", async ({ page }) => {
    await loadScenario(page);
    await openMods(page);

    await modsPane(page, "inactive").getByTestId("mods-search").fill("Inactive Mod 1");
    // "Inactive Mod 1" and "Inactive Mod 10".
    await expect(modsPane(page, "inactive").getByTestId("mods-select-all-label")).toHaveText(
      "Select all 2 shown",
    );
    await modsPane(page, "inactive").getByTestId("mods-select-all").locator("input").check();
    await expect(modsPane(page, "inactive").getByTestId("mods-selected-count")).toContainText(
      "2 selected",
    );
    await page.getByTestId("activate-selected-button").click();

    await expect(page.getByTestId("activate-plan-to-add")).toHaveCount(2);
    await page.getByTestId("activate-confirm-cancel").click();
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([]);
  });

  test("a row a later filter hides stays selected, is disclosed, and Clear removes it", async ({
    page,
  }) => {
    await loadScenario(page);
    await openMods(page);
    await modsPane(page, "inactive").getByTestId("mods-select-all").locator("input").check();
    await expect(modsPane(page, "inactive").getByTestId("mods-selected-count")).toContainText(
      "10 selected",
    );

    await modsPane(page, "inactive").getByTestId("mods-search").fill("Inactive Mod 1");

    await expect(modsPane(page, "inactive").getByTestId("mods-select-all-label")).toHaveText(
      "Select all 2 shown",
    );
    await expect(modsPane(page, "inactive").getByTestId("mods-selected-count")).toContainText(
      "10 selected",
    );
    await expect(modsPane(page, "inactive").getByTestId("mods-selected-hidden")).toHaveText(
      "(8 not shown)",
    );

    await modsPane(page, "inactive").getByTestId("mods-clear-selection").click();
    await expect(modsPane(page, "inactive").getByTestId("mods-selected-count")).toHaveText("");
    await expect(page.getByTestId("activate-selected-button")).toBeDisabled();
  });

  test("selecting writes nothing: no activation call until the dialog is confirmed", async ({
    page,
  }) => {
    await loadScenario(page);
    await openMods(page);

    await modsPane(page, "inactive").getByTestId("mods-select-all").locator("input").check();
    await modsPane(page, "inactive").getByTestId("mods-clear-selection").click();

    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([]);
    await expect(page.getByTestId("pending-changes-banner")).toHaveCount(0);
  });
});

test.describe("mods multi-select gestures", () => {
  const inactiveCount = (page: Page) =>
    modsPane(page, "inactive").getByTestId("mods-selected-count");
  const inactiveRow = (page: Page, number: number) =>
    modsPane(page, "inactive").getByTestId(`mod-row-mod.9${String(number).padStart(2, "0")}`);

  test("Ctrl+click toggles and Shift+click adds a range; Activate selected lists exactly those", async ({
    page,
  }) => {
    await loadScenario(page);
    await openMods(page);

    await inactiveRow(page, 2).click({ modifiers: ["ControlOrMeta"] });
    await inactiveRow(page, 4).click({ modifiers: ["Shift"] });
    await inactiveRow(page, 8).click({ modifiers: ["ControlOrMeta"] });

    await expect(inactiveCount(page)).toContainText("4 selected");
    await expect(inactiveRow(page, 3)).toHaveAttribute("aria-selected", "true");
    await expect(inactiveRow(page, 5)).toHaveAttribute("aria-selected", "false");
    await expect(page.getByTestId("mod-info-panel")).toHaveCount(0);

    await page.getByTestId("activate-selected-button").click();
    await expect(page.getByTestId("activate-plan-to-add")).toHaveCount(4);
    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();

    const calls = await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? []);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.ids.toSorted()).toEqual(["mod.902", "mod.903", "mod.904", "mod.908"]);
  });

  test("a plain click opens the info panel and leaves the selection alone", async ({ page }) => {
    await loadScenario(page);
    await openMods(page);
    await inactiveRow(page, 2).click({ modifiers: ["ControlOrMeta"] });

    await inactiveRow(page, 6).click();

    await expect(page.getByTestId("mod-info-panel")).toBeVisible();
    await expect(inactiveCount(page)).toContainText("1 selected");
    await expect(inactiveRow(page, 6)).toHaveAttribute("aria-selected", "false");
  });

  test("keyboard: Shift+Arrow extends, Ctrl+A selects all shown, Escape clears", async ({
    page,
  }) => {
    await loadScenario(page);
    await openMods(page);

    await inactiveRow(page, 1).focus();
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowDown");
    await expect(inactiveCount(page)).toHaveText("3 selected");
    await page.keyboard.press("Shift+ArrowDown");
    await expect(inactiveCount(page)).toHaveText("4 selected");
    await page.keyboard.press("Shift+ArrowUp");
    await expect(inactiveCount(page)).toHaveText("3 selected");

    await page.keyboard.press("ControlOrMeta+a");
    await expect(inactiveCount(page)).toContainText("10 selected");

    await page.keyboard.press("Escape");
    await expect(inactiveCount(page)).toHaveText("");
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([]);
  });

  test("the anchor follows focus: toggle, move on with plain arrows, then Shift+Arrow", async ({
    page,
  }) => {
    await loadScenario(page);
    await openMods(page);

    // Name order: 901, 910, 902, 903, 904, 905.
    await inactiveRow(page, 2).click({ modifiers: ["ControlOrMeta"] });
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await expect(inactiveRow(page, 4)).toBeFocused();
    await expect(inactiveCount(page)).toContainText("1 selected");
    await page.keyboard.press("Shift+ArrowDown");

    await expect(inactiveCount(page)).toHaveText("3 selected");
    for (const [number, isSelected] of [
      [2, true],
      [3, false],
      [4, true],
      [5, true],
    ] as const) {
      await expect(inactiveRow(page, number)).toHaveAttribute("aria-selected", String(isSelected));
    }
  });

  test("a range under a filter never adds the rows the filter hides", async ({ page }) => {
    await loadScenario(page);
    await openMods(page);
    await inactiveRow(page, 2).click({ modifiers: ["ControlOrMeta"] });
    await inactiveRow(page, 3).click({ modifiers: ["ControlOrMeta"] });

    await modsPane(page, "inactive").getByTestId("mods-search").fill("Inactive Mod 1");
    await expect(inactiveRow(page, 4)).toHaveCount(0);
    await inactiveRow(page, 1).click({ modifiers: ["ControlOrMeta"] });
    await inactiveRow(page, 10).click({ modifiers: ["Shift"] });

    await expect(inactiveCount(page)).toHaveText(/4 selected/);
    await expect(modsPane(page, "inactive").getByTestId("mods-selected-hidden")).toHaveText(
      "(2 not shown)",
    );
  });

  test("each list keeps its own selection, and no gesture moves anything", async ({ page }) => {
    await loadScenario(page);
    await openMods(page);

    await inactiveRow(page, 1).click({ modifiers: ["ControlOrMeta"] });
    await modsPane(page, "active")
      .getByTestId("mod-row-mod.001")
      .click({ modifiers: ["Shift"] });

    await expect(inactiveCount(page)).toContainText("1 selected");
    await expect(modsPane(page, "active").getByTestId("mods-selected-count")).toContainText(
      "1 selected",
    );
    expect(await page.evaluate(() => window.__MOVE_MODS_CALLS__ ?? [])).toEqual([]);
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([]);
  });

  test("dragging a selected row after a range carries the whole range", async ({ page }) => {
    await loadScenario(page);
    await openMods(page);
    await inactiveRow(page, 2).click({ modifiers: ["ControlOrMeta"] });
    await inactiveRow(page, 4).click({ modifiers: ["Shift"] });

    await inactiveRow(page, 3).dragTo(page.getByTestId("mod-row-mod.002"), {
      targetPosition: { x: 80, y: 4 },
    });

    const calls = await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? []);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.ids.toSorted()).toEqual(["mod.902", "mod.903", "mod.904"]);
  });
});

test.describe("mods page rescan and working-tier lists", () => {
  test("the Rescan button is always visible and rescans without pending changes", async ({
    page,
  }) => {
    await loadScenario(page);
    await openMods(page);
    await expect(page.getByTestId("pending-changes-banner")).toHaveCount(0);

    await expect(page.getByTestId("mods-rescan-button")).toBeVisible();
    await page.getByTestId("mods-rescan-button").click();

    await expect.poll(() => page.evaluate(() => window.__RESCAN_CALLS__ ?? 0)).toBe(1);
    await expect(page.getByTestId("mods-rescan-button")).toBeEnabled();
  });

  test("an activated mod is listed as pending its scan at once, then as scanned after a rescan", async ({
    page,
  }) => {
    await loadScenario(page);
    await openMods(page);
    await page.getByTestId("mod-row-select-mod.903").locator("input").check();
    await page.getByTestId("activate-selected-button").click();
    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();

    await expect(page.getByTestId("mod-row-mod.903")).toHaveCount(0);
    await modsPane(page, "active").getByTestId("mods-search").fill("mod.903");
    await expect(page.getByTestId("mod-row-mod.903")).toBeVisible();
    await expect(page.getByTestId("mod-row-pending-scan-pill")).toBeVisible();

    await page.getByTestId("mods-rescan-button").click();
    await expect(page.getByTestId("mod-row-pending-scan-pill")).toHaveCount(0);
    await expect(page.getByTestId("mod-row-mod.903")).toBeVisible();
  });

  test("a mod ModsConfig.xml lists but no folder backs shows as missing, with an unknown origin", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      window.__MISSING_MODS__ = ["ghost.mod"];
    });
    await loadScenario(page);
    await openMods(page);

    await modsPane(page, "active").getByTestId("mods-search").fill("ghost.mod");

    const row = page.getByTestId("mod-row-ghost.mod");
    await expect(row).toBeVisible();
    await expect(row.getByTestId("mod-row-missing-pill")).toBeVisible();
    await expect(row.getByTestId("source-badge")).toHaveAccessibleName(
      "Origin unknown (not installed)",
    );
  });

  test("Mods that move reads 0 after applying the suggested order, with no rescan", async ({
    page,
  }) => {
    await loadScenario(page);
    await expect(page.getByTestId("moved-mods")).toHaveText("6");

    await page.getByTestId("shell-apply-button").click();
    await expect(page.getByTestId("apply-dialog-order-source")).toHaveText("Suggested");
    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();
    await expect(page.getByTestId("apply-dialog")).toBeHidden();

    await expect(page.getByTestId("moved-mods")).toHaveText("0");
    expect(await page.evaluate(() => window.__RESCAN_CALLS__ ?? 0)).toBe(0);
  });
});
