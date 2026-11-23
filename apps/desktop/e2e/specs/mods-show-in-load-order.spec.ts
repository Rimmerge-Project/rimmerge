import { expect, type Locator, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { node, openMods } from "./organization-support";
import { modsPane } from "./support";

/**
 * "Show in load order" (#55) and the remembered sort and view of each Mods pane (#56). By name
 * descending the rendered rows include mods far down the load order, so showing one in load order
 * has to scroll the virtualized list.
 */

type Pane = "active" | "inactive";

const PANES_KEY = "rimmerge.modsPanes.v1";
const ACTIVE_MARK = /(^| )border-2( |$)/;

/** Creates one group holding `modIds`, as a setup step. */
async function seedGroup(page: Page, name: string, modIds: string[]): Promise<void> {
  await page.evaluate(
    ([groupName, ids]) =>
      window.__TAURI_INTERNALS__.invoke("create_group", {
        request: { name: groupName, parent: null, modIds: ids },
      }),
    [name, modIds] as const,
  );
}

function sortButton(page: Page, pane: Pane, key: string): Locator {
  return modsPane(page, pane).getByTestId(`mods-sort-${key}`);
}

async function sortBy(page: Page, pane: Pane, key: string, clicks = 1): Promise<void> {
  for (let index = 0; index < clicks; index++) {
    await sortButton(page, pane, key).click();
  }
}

async function renderedRowIds(page: Page, pane: Pane = "active"): Promise<string[]> {
  return modsPane(page, pane)
    .getByTestId("mod-table")
    .locator('[role="row"]')
    .evaluateAll((rows) =>
      rows.map((row) => (row.getAttribute("data-testid") ?? "").replace(/^mod-row-/, "")),
    );
}

/** Of the rendered rows, the mod with the highest load-order position (it must be scrolled to in load order). */
async function furthestDownLoadOrder(page: Page): Promise<string> {
  const ids = await renderedRowIds(page);
  const positions = await Promise.all(
    ids.map(async (id) => ({
      id,
      position: Number(
        (await modsPane(page, "active").getByTestId(`mod-row-position-${id}`).innerText()).replace(
          "#",
          "",
        ),
      ),
    })),
  );
  const [furthest] = positions.toSorted((left, right) => right.position - left.position);
  expect(furthest?.position).toBeGreaterThan(20);
  return furthest?.id ?? "";
}

async function openRowMenu(page: Page, pane: Pane, modId: string): Promise<void> {
  await modsPane(page, pane).getByTestId(`mod-row-menu-${modId}`).first().click();
}

/** The row element that holds keyboard focus, by its test id. */
function focusedRowId(page: Page): Promise<string | null> {
  return page.evaluate(() => document.activeElement?.getAttribute("data-testid") ?? null);
}

/** Reloads the page and loads the scenario again; `localStorage` stays, the mock's state does not. */
async function reloadToMods(page: Page): Promise<void> {
  await page.reload();
  await page.getByTestId("load-project-button").click();
  await page.waitForURL(/\/$/);
  await page.getByTestId("nav-mods").click();
  await expect(modsPane(page, "active").getByTestId("mod-table")).toBeVisible();
}

test.describe("Show in load order", () => {
  test("from a name sort it switches to load order, scrolls to the mod and focuses its row", async ({
    page,
  }) => {
    await openMods(page);
    await sortBy(page, "active", "name", 2);
    await expect(sortButton(page, "active", "name")).toHaveClass(ACTIVE_MARK);
    const farId = await furthestDownLoadOrder(page);

    await openRowMenu(page, "active", farId);
    await page.getByTestId("mod-show-in-load-order").click();

    await expect(sortButton(page, "active", "loadOrder")).toHaveClass(ACTIVE_MARK);
    await expect(modsPane(page, "active").getByTestId(`mod-row-${farId}`)).toBeInViewport();
    await expect.poll(() => focusedRowId(page)).toBe(`mod-row-${farId}`);
    await expect(page.getByTestId("mods-move-status")).toHaveText(/Showing .* in load order\./);
  });

  test("from the Groups view it switches that pane to List in load order and focuses the row", async ({
    page,
  }) => {
    await openMods(page);
    await seedGroup(page, "Weapons", ["mod.003", "mod.004"]);
    const active = modsPane(page, "active");
    await active.getByTestId("mods-view-groups").click();
    await expect(active.getByTestId("mod-groups-table")).toBeVisible();

    await active.getByTestId("mod-entry-1-mod.004").getByTestId("mod-row-menu-mod.004").click();
    await page.getByTestId("mod-show-in-load-order").click();

    await expect(active.getByTestId("mod-table")).toBeVisible();
    await expect(active.getByTestId("mods-view-list")).toBeChecked();
    await expect(sortButton(page, "active", "loadOrder")).toHaveClass(ACTIVE_MARK);
    await expect.poll(() => focusedRowId(page)).toBe("mod-row-mod.004");
    // The Inactive pane was not touched.
    await expect(modsPane(page, "inactive").getByTestId("mods-view-list")).toBeChecked();
  });

  test("keeps the filter, and says nothing, when the mod is still shown under it", async ({
    page,
  }) => {
    await openMods(page);
    const active = modsPane(page, "active");
    await active.getByTestId("mods-search").fill("mod.05");
    await expect(active.getByTestId("mods-total")).toHaveText("10 mods");
    await sortBy(page, "active", "name");

    await openRowMenu(page, "active", "mod.052");
    await page.getByTestId("mod-show-in-load-order").click();

    await expect(sortButton(page, "active", "loadOrder")).toHaveClass(ACTIVE_MARK);
    await expect(active.getByTestId("mods-search")).toHaveValue("mod.05");
    await expect(page.getByTestId("mods-show-in-load-order-note")).toHaveCount(0);
    await expect.poll(() => focusedRowId(page)).toBe("mod-row-mod.052");
  });

  test("clears a filter that hides the mod, says so, and focuses the row", async ({ page }) => {
    await openMods(page);
    const active = modsPane(page, "active");
    await active.getByTestId("mods-search").fill("mod.05");
    await active.getByTestId("mod-row-mod.050").click();
    await expect(page.getByTestId("mod-info-panel")).toBeVisible();
    // The open mod is now hidden by the search; the pane is in load order already.
    await active.getByTestId("mods-search").fill("mod.04");
    await expect(active.getByTestId("mod-row-mod.050")).toHaveCount(0);

    await page.getByTestId("mod-info-show-in-load-order").click();

    await expect(active.getByTestId("mods-search")).toHaveValue("");
    await expect(page.getByTestId("mods-show-in-load-order-note")).toContainText(
      "was hidden by the filter, so the filter was cleared.",
    );
    await expect.poll(() => focusedRowId(page)).toBe("mod-row-mod.050");
  });

  test("clears a group scope that hides the mod, and leaves the Inactive pane's own search alone", async ({
    page,
  }) => {
    await openMods(page);
    await seedGroup(page, "Weapons", ["mod.004"]);
    await modsPane(page, "inactive").getByTestId("mods-search").fill("mod.9");
    await modsPane(page, "active").getByTestId("mod-row-mod.003").click();
    await expect(page.getByTestId("mod-info-panel")).toBeVisible();
    await node(page, "Weapons").click();
    await expect(page.getByTestId("group-scope-bar")).toBeVisible();
    await expect(modsPane(page, "active").getByTestId("mod-row-mod.003")).toHaveCount(0);

    await page.getByTestId("mod-info-show-in-load-order").click();

    await expect(page.getByTestId("group-scope-bar")).toHaveCount(0);
    await expect(page.getByTestId("mods-show-in-load-order-note")).toBeVisible();
    await expect.poll(() => focusedRowId(page)).toBe("mod-row-mod.003");
    await expect(modsPane(page, "inactive").getByTestId("mods-search")).toHaveValue("mod.9");
  });

  test("is not offered while the pane already shows load order, nor for an inactive mod", async ({
    page,
  }) => {
    await openMods(page);
    await openRowMenu(page, "active", "mod.003");
    await expect(page.getByTestId("mod-move-menu").getByTestId("mod-move-up")).toBeVisible();
    await expect(page.getByTestId("mod-show-in-load-order")).toHaveCount(0);
    await page.keyboard.press("Escape");

    await modsPane(page, "inactive").getByTestId("mod-row-menu-mod.901").first().click();
    await expect(page.getByTestId("mod-show-in-load-order")).toHaveCount(0);
    await page.keyboard.press("Escape");

    // The panel's button follows the same rule: gone for a mod shown in load order.
    await modsPane(page, "active").getByTestId("mod-row-mod.003").click();
    await expect(page.getByTestId("mod-info-panel")).toBeVisible();
    await expect(page.getByTestId("mod-info-show-in-load-order")).toHaveCount(0);
  });

  test("the panel's button works from a name sort, and the choice is the remembered sort", async ({
    page,
  }) => {
    await openMods(page);
    await sortBy(page, "active", "name");
    await modsPane(page, "active").getByTestId("mod-row-mod.000").click();
    await expect(page.getByTestId("mod-info-panel")).toBeVisible();

    await page.getByTestId("mod-info-show-in-load-order").click();

    await expect(sortButton(page, "active", "loadOrder")).toHaveClass(ACTIVE_MARK);
    await expect.poll(() => focusedRowId(page)).toBe("mod-row-mod.000");
    expect(await page.evaluate((key) => localStorage.getItem(key), PANES_KEY)).toContain(
      '"key":"loadOrder"',
    );
  });
});

test.describe("remembered sort and view", () => {
  test("each pane comes back with its own sort and view after a reload", async ({ page }) => {
    await openMods(page);
    await seedGroup(page, "Weapons", ["mod.003", "mod.901"]);
    await sortBy(page, "active", "name", 2);
    await sortBy(page, "inactive", "lastUpdated");
    await modsPane(page, "inactive").getByTestId("mods-view-groups").click();

    await reloadToMods(page);
    await seedGroup(page, "Weapons", ["mod.003", "mod.901"]);

    await expect(sortButton(page, "active", "name")).toHaveClass(ACTIVE_MARK);
    await expect(sortButton(page, "active", "name")).toHaveAccessibleName(/Sorted by name, Z to A/);
    await expect(sortButton(page, "inactive", "lastUpdated")).toHaveClass(ACTIVE_MARK);
    await expect(modsPane(page, "active").getByTestId("mods-view-list")).toBeChecked();
    await expect(modsPane(page, "inactive").getByTestId("mods-view-groups")).toBeChecked();
    await expect(modsPane(page, "inactive").getByTestId("mod-groups-table")).toBeVisible();
  });

  test("a restored non-load-order Active sort still shows the reorder-off note and the positions", async ({
    page,
  }) => {
    await openMods(page);
    await sortBy(page, "active", "name");

    await reloadToMods(page);

    await expect(page.getByTestId("mods-reorder-blocked")).toBeVisible();
    await expect(sortButton(page, "active", "loadOrder")).not.toHaveClass(ACTIVE_MARK);
    const firstId = (await renderedRowIds(page))[0] ?? "";
    await expect(modsPane(page, "active").getByTestId(`mod-row-position-${firstId}`)).toBeVisible();
  });

  test("a stored value the app no longer knows falls back to the default, pane by pane", async ({
    page,
  }) => {
    await openMods(page);
    await page.evaluate((key) => {
      localStorage.setItem(
        key,
        JSON.stringify({
          active: { sort: { key: "author", direction: "ascending" }, view: "list" },
          inactive: { sort: { key: "packageId", direction: "descending" }, view: "list" },
        }),
      );
    }, PANES_KEY);

    await reloadToMods(page);

    await expect(sortButton(page, "active", "loadOrder")).toHaveClass(ACTIVE_MARK);
    await expect(sortButton(page, "inactive", "packageId")).toHaveClass(ACTIVE_MARK);
    await expect(page.getByTestId("mods-reorder-blocked")).toHaveCount(0);
  });

  test("malformed JSON in storage gives both panes their defaults", async ({ page }) => {
    await openMods(page);
    await page.evaluate((key) => localStorage.setItem(key, "{not json"), PANES_KEY);

    await reloadToMods(page);

    await expect(sortButton(page, "active", "loadOrder")).toHaveClass(ACTIVE_MARK);
    await expect(sortButton(page, "inactive", "name")).toHaveClass(ACTIVE_MARK);
    await expect(modsPane(page, "active").getByTestId("mods-view-list")).toBeChecked();
    await expect(modsPane(page, "inactive").getByTestId("mods-view-list")).toBeChecked();
  });
});
