import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario, modsPane } from "./support";

/** The mod ids of the rendered (virtualized) table rows, in DOM order. */
async function renderedRowIds(
  page: Page,
  kind: "active" | "inactive" = "active",
): Promise<string[]> {
  return modsPane(page, kind)
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

/** The thicker border only the active sort button has (the non-colour cue next to its arrow). */
const ACTIVE_MARK = /(^| )border-2( |$)/;

test.describe("mods display sort", () => {
  test("each button names its criterion, direction and next click, and the default one is marked active", async ({
    page,
  }) => {
    await openMods(page);

    const loadOrder = modsPane(page, "active").getByTestId("mods-sort-loadOrder");
    const activeName = "Sorted by load order, first to last. Click for last to first.";
    await expect(loadOrder).toHaveAccessibleName(activeName);
    await expect(loadOrder).toHaveClass(ACTIVE_MARK);
    await expect(loadOrder).toHaveAttribute("title", activeName);
    await expect(
      modsPane(page, "active").getByTestId("mods-sort-lastUpdated"),
    ).toHaveAccessibleName("Sort by last updated, oldest first");
    await expect(modsPane(page, "active").getByTestId("mods-sort-name")).not.toHaveClass(
      ACTIVE_MARK,
    );
  });

  test("sorts Active by name, both ways, as the backend does (lowercase text, then id)", async ({
    page,
  }) => {
    await openMods(page);
    // Only the generic pool ("Mod 0" ... "Mod 59"), so the fixture owners' names stay out of it.
    await modsPane(page, "active").getByTestId("mods-search").fill("mod.0");
    await expect(modsPane(page, "active").getByTestId("mods-total")).toHaveText("60 mods");

    const nameButton = modsPane(page, "active").getByTestId("mods-sort-name");
    await nameButton.click();
    // "Mod 0" (Core), "Mod 1" (DLC), "Mod 10", "Mod 11", ... : text order, not id order.
    await expect(nameButton).toHaveClass(ACTIVE_MARK);
    await expect(nameButton).toHaveAccessibleName("Sorted by name, A to Z. Click for Z to A.");
    await expect(page.getByTestId("mod-row-mod.010")).toBeVisible();
    expect((await renderedRowIds(page)).slice(0, 4)).toEqual([
      "mod.000",
      "mod.001",
      "mod.010",
      "mod.011",
    ]);

    // A second click on the same button flips the direction.
    await nameButton.click();
    await expect(nameButton).toHaveClass(ACTIVE_MARK);
    await expect(nameButton).toHaveAccessibleName("Sorted by name, Z to A. Click for A to Z.");
    // "Mod 9" is the last name in text order, so it leads descending.
    await expect(page.getByTestId("mod-row-mod.009")).toBeVisible();
    expect((await renderedRowIds(page))[0]).toBe("mod.009");
  });

  test("last updated puts unavailable times last in both directions, ties by id", async ({
    page,
  }) => {
    await openMods(page);
    await modsPane(page, "active").getByTestId("mods-search").fill("mod.00");

    const updatedButton = modsPane(page, "active").getByTestId("mods-sort-lastUpdated");
    await updatedButton.click();
    // mod.000/001 ship with the game, mod.007 has no Steam record; the rest carry a time.
    await expect(page.getByTestId("mod-row-updated-mod.002")).toBeVisible();
    const ascending = await renderedRowIds(page);
    expect(ascending.slice(-3)).toEqual(["mod.000", "mod.001", "mod.007"]);

    await updatedButton.click();
    await expect(updatedButton).toHaveAccessibleName(
      "Sorted by last updated, newest first. Click for oldest first.",
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

    await modsPane(page, "active").getByTestId("mods-search").fill("Mod 1");
    const packageIdButton = modsPane(page, "active").getByTestId("mods-sort-packageId");
    await packageIdButton.click();
    await packageIdButton.click();
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

  test("the Inactive list has no load-order button and keeps its own sort", async ({ page }) => {
    await openMods(page);
    await modsPane(page, "active").getByTestId("mods-sort-packageId").click();

    await expect(page.getByTestId("mod-row-mod.901")).toBeVisible();

    await expect(modsPane(page, "inactive").getByTestId("mods-sort-loadOrder")).toHaveCount(0);
    await expect(modsPane(page, "inactive").getByTestId("mods-sort-name")).toHaveClass(ACTIVE_MARK);
    await modsPane(page, "inactive").getByTestId("mods-sort-name").click();
    // Name, Z to A: "Inactive Mod 9" first.
    await expect(page.getByTestId("mod-row-mod.909")).toBeVisible();
    expect((await renderedRowIds(page, "inactive"))[0]).toBe("mod.909");
    expect(await page.evaluate(() => window.__LIST_INACTIVE_MODS_CALLS__?.at(-1)?.sort)).toEqual({
      key: "name",
      direction: "descending",
    });

    await expect(modsPane(page, "active").getByTestId("mods-sort-packageId")).toHaveClass(
      ACTIVE_MARK,
    );
  });

  test("sorting is display only: it writes nothing and load order returns unchanged", async ({
    page,
  }) => {
    await openMods(page);
    const original = (await renderedRowIds(page)).slice(0, 6);
    const snapshotBefore = await sessionSnapshot(page);

    await modsPane(page, "active").getByTestId("mods-sort-name").click();
    await modsPane(page, "active").getByTestId("mods-sort-name").click();
    await modsPane(page, "active").getByTestId("mods-sort-lastUpdated").click();
    await modsPane(page, "active").getByTestId("mods-sort-loadOrder").click();
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

    await modsPane(page, "active").getByTestId("mods-sort-name").click();
    await expect(page.getByTestId("mod-row-position-mod.000")).toBeVisible();
    await expect(page.getByTestId("mod-row-position-mod.000")).toHaveText("#1");
    await expect(page.getByTestId("mod-row-mod.000")).toContainText("Load order position 1");

    await modsPane(page, "active").getByTestId("mods-sort-loadOrder").click();
    await expect(page.getByTestId("mod-row-position-mod.000")).toHaveCount(0);
  });

  test("a sort button works from the keyboard", async ({ page }) => {
    await openMods(page);

    const button = modsPane(page, "active").getByTestId("mods-sort-name");
    await button.focus();
    await page.keyboard.press("Enter");
    await expect(button).toHaveClass(ACTIVE_MARK);
    await expect(button).toHaveAccessibleName("Sorted by name, A to Z. Click for Z to A.");

    await page.keyboard.press("Space");
    await expect(button).toHaveAccessibleName("Sorted by name, Z to A. Click for A to Z.");
    await expect(modsPane(page, "active").getByTestId("mods-sort-loadOrder")).not.toHaveClass(
      ACTIVE_MARK,
    );
  });
});
