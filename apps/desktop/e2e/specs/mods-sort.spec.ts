import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/** The mod ids of the rendered (virtualized) table rows, in DOM order. */
async function renderedRowIds(page: Page): Promise<string[]> {
  return page
    .getByTestId("mod-table")
    .locator('[role="row"]')
    .evaluateAll((rows) =>
      rows.map((row) => (row.getAttribute("data-testid") ?? "").replace(/^mod-row-/, "")),
    );
}

/** The backend state a display sort must never change: pending changes and both orders. */
async function sessionSnapshot(page: Page): Promise<unknown[]> {
  return page.evaluate(async () => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    return [
      await invoke("get_pending_active_changes"),
      await invoke("list_order", { source: "current" }),
      await invoke("list_order", { source: "suggested" }),
    ];
  });
}

async function openMods(page: Page): Promise<void> {
  await loadScenario(page);
  await page.getByTestId("nav-mods").click();
  await expect(page.getByTestId("mod-row-mod.000")).toBeVisible();
}

test.describe("mods display sort", () => {
  test("every arrow names its criterion and direction, and the default one is pressed", async ({
    page,
  }) => {
    await openMods(page);

    const loadOrder = page.getByTestId("mods-sort-loadOrder-ascending");
    await expect(loadOrder).toHaveAccessibleName("Sort by load order, first to last");
    await expect(loadOrder).toHaveAttribute("aria-pressed", "true");
    await expect(loadOrder).toHaveAttribute("title", "Sort by load order, first to last");
    await expect(page.getByTestId("mods-sort-lastUpdated-descending")).toHaveAccessibleName(
      "Sort by last updated, newest first",
    );
    await expect(page.getByTestId("mods-sort-name-ascending")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  test("sorts Active by name, both ways, as the backend does (lowercase text, then id)", async ({
    page,
  }) => {
    await openMods(page);
    // Only the generic pool ("Mod 0" ... "Mod 59"), so the fixture owners' names stay out of it.
    await page.getByTestId("mods-search").fill("mod.0");
    await expect(page.getByTestId("mods-total")).toHaveText("60 mods");

    await page.getByTestId("mods-sort-name-ascending").click();
    // "Mod 0" (Core), "Mod 1" (DLC), "Mod 10", "Mod 11", ... : text order, not id order.
    await expect(page.getByTestId("mods-sort-name-ascending")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect(page.getByTestId("mod-row-mod.010")).toBeVisible();
    expect((await renderedRowIds(page)).slice(0, 4)).toEqual([
      "mod.000",
      "mod.001",
      "mod.010",
      "mod.011",
    ]);

    await page.getByTestId("mods-sort-name-descending").click();
    await expect(page.getByTestId("mods-sort-name-descending")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect(page.getByTestId("mods-sort-name-ascending")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    // "Mod 9" is the last name in text order, so it leads descending.
    await expect(page.getByTestId("mod-row-mod.009")).toBeVisible();
    expect((await renderedRowIds(page))[0]).toBe("mod.009");
  });

  test("last updated puts unavailable times last in both directions, ties by id", async ({
    page,
  }) => {
    await openMods(page);
    await page.getByTestId("mods-search").fill("mod.00");

    await page.getByTestId("mods-sort-lastUpdated-ascending").click();
    // mod.000/001 ship with the game, mod.007 has no Steam record; the rest carry a time.
    await expect(page.getByTestId("mod-row-updated-mod.002")).toBeVisible();
    const ascending = await renderedRowIds(page);
    expect(ascending.slice(-3)).toEqual(["mod.000", "mod.001", "mod.007"]);

    await page.getByTestId("mods-sort-lastUpdated-descending").click();
    await expect(page.getByTestId("mods-sort-lastUpdated-descending")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    const descending = await renderedRowIds(page);
    expect(descending.slice(-3)).toEqual(["mod.000", "mod.001", "mod.007"]);
    // The known ones reverse: the day sets repeat, so equal days keep ascending id order.
    expect(descending.slice(0, -3)).not.toEqual(ascending.slice(0, -3));
    await expect(page.getByTestId("mod-row-updated-mod.000")).toHaveText("Ships with the game");
    await expect(page.getByTestId("mod-row-updated-mod.007")).toHaveText("Update time unknown");
  });

  test("sorting works with the search filter and sends exactly the chosen sort", async ({
    page,
  }) => {
    await openMods(page);

    await page.getByTestId("mods-search").fill("Mod 1");
    await page.getByTestId("mods-sort-packageId-descending").click();
    await expect
      .poll(() => page.evaluate(() => window.__LIST_WORKING_MODS_CALLS__?.at(-1)))
      .toEqual({
        search: "Mod 1",
        tag: null,
        source: null,
        sort: { key: "packageId", direction: "descending" },
      });

    const ids = await renderedRowIds(page);
    expect(ids.length).toBeGreaterThan(1);
    expect(ids).toEqual(ids.toSorted().toReversed());
    expect(ids.every((id) => id.startsWith("mod."))).toBe(true);
  });

  test("the Inactive list has no load-order arrows and keeps its own sort", async ({ page }) => {
    await openMods(page);
    await page.getByTestId("mods-sort-packageId-ascending").click();

    await page.getByTestId("mods-tab-inactive").click();
    await expect(page.getByTestId("mod-row-mod.901")).toBeVisible();

    await expect(page.getByTestId("mods-sort-loadOrder-ascending")).toHaveCount(0);
    await expect(page.getByTestId("mods-sort-name-ascending")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await page.getByTestId("mods-sort-name-descending").click();
    // Name, Z to A: "Inactive Mod 9" first.
    await expect(page.getByTestId("mod-row-mod.909")).toBeVisible();
    expect((await renderedRowIds(page))[0]).toBe("mod.909");
    expect(await page.evaluate(() => window.__LIST_INACTIVE_MODS_CALLS__?.at(-1)?.sort)).toEqual({
      key: "name",
      direction: "descending",
    });

    await page.getByTestId("mods-tab-active").click();
    await expect(page.getByTestId("mods-sort-packageId-ascending")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  test("sorting is display only: it writes nothing and load order returns unchanged", async ({
    page,
  }) => {
    await openMods(page);
    const original = (await renderedRowIds(page)).slice(0, 6);
    const snapshotBefore = await sessionSnapshot(page);

    await page.getByTestId("mods-sort-name-descending").click();
    await page.getByTestId("mods-sort-lastUpdated-ascending").click();
    await page.getByTestId("mods-sort-loadOrder-ascending").click();
    await expect(page.getByTestId("mod-row-mod.000")).toBeVisible();

    expect((await renderedRowIds(page)).slice(0, 6)).toEqual(original);
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([]);
    expect(await page.evaluate(() => window.__DEACTIVATE_CALLS__ ?? [])).toEqual([]);
    expect(await page.evaluate(() => window.__RESCAN_CALLS__ ?? 0)).toBe(0);
    expect(await sessionSnapshot(page)).toEqual(snapshotBefore);
  });

  test("Active rows show their load-order position while sorted by anything else", async ({
    page,
  }) => {
    await openMods(page);
    await expect(page.getByTestId("mod-row-position-mod.000")).toHaveCount(0);

    await page.getByTestId("mods-sort-name-ascending").click();
    await expect(page.getByTestId("mod-row-position-mod.000")).toBeVisible();
    await expect(page.getByTestId("mod-row-position-mod.000")).toHaveText("#1");
    await expect(page.getByTestId("mod-row-mod.000")).toContainText("Load order position 1");

    await page.getByTestId("mods-sort-loadOrder-ascending").click();
    await expect(page.getByTestId("mod-row-position-mod.000")).toHaveCount(0);
  });

  test("an arrow works from the keyboard", async ({ page }) => {
    await openMods(page);

    const arrow = page.getByTestId("mods-sort-name-ascending");
    await arrow.focus();
    await page.keyboard.press("Enter");
    await expect(arrow).toHaveAttribute("aria-pressed", "true");

    const other = page.getByTestId("mods-sort-name-descending");
    await other.focus();
    await page.keyboard.press("Space");
    await expect(other).toHaveAttribute("aria-pressed", "true");
    await expect(arrow).toHaveAttribute("aria-pressed", "false");
  });
});
