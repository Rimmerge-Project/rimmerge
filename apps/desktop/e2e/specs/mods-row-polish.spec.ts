import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import {
  createGroup,
  expectNothingMoved,
  openMods,
  openRowMenu,
  WIDE,
} from "./organization-support";
import { modsPane } from "./support";

/**
 * Row polish on the Mods page (#54 batch A): the open-row highlight, creating a group from "Add
 * to group", the double click that (de)activates through the plan dialogs, colour swatches in the
 * row menu, and the group chips that scope the lists. Every spec runs against the mock IPC.
 */

type Pane = "active" | "inactive";

async function selectRows(page: Page, pane: Pane, ids: readonly string[]): Promise<void> {
  for (const id of ids) {
    await modsPane(page, pane).getByTestId(`mod-row-select-${id}`).locator("input").check();
  }
}

async function organizationRequests(page: Page, command: string): Promise<unknown[]> {
  return page.evaluate(
    (name) =>
      (window.__ORGANIZATION_CALLS__ ?? [])
        .filter((call) => call.command === name)
        .map((call) => call.request),
    command,
  );
}

test.describe("mods row polish", () => {
  test("a plain click draws the ring and tint on the open row, and moves with the next click", async ({
    page,
  }) => {
    await openMods(page);
    const active = modsPane(page, "active");
    const first = active.getByTestId("mod-row-mod.003");
    const second = active.getByTestId("mod-row-mod.004");

    await first.getByTestId("mod-row-name-mod.003").click();
    await expect(page.getByTestId("mod-info-panel")).toBeVisible();

    await expect(first).toHaveAttribute("aria-current", "true");
    await expect(first).toHaveClass(/(^|\s)bg-accent-soft/);
    await expect(first).toHaveClass(/inset-ring-2/);
    await expect(second).not.toHaveAttribute("aria-current", "true");

    await second.getByTestId("mod-row-name-mod.004").click();
    await expect(second).toHaveAttribute("aria-current", "true");
    await expect(first).not.toHaveAttribute("aria-current", "true");
    await expect(first).not.toHaveClass(/(^|\s)bg-accent-soft/);
  });

  test("shows the load-order number in the Active list, in load order too", async ({ page }) => {
    await openMods(page);

    await expect(modsPane(page, "active").getByTestId("mod-row-position-mod.003")).toBeVisible();
    await expect(
      modsPane(page, "inactive").locator('[data-testid^="mod-row-position-"]'),
    ).toHaveCount(0);
  });

  test("creates a group from the bulk Add to group dialog and adds the selection in one save", async ({
    page,
  }) => {
    const before = await openMods(page);
    const active = modsPane(page, "active");
    await selectRows(page, "active", ["mod.003", "mod.004"]);

    await active.getByTestId("bulk-add-to-group-active").click();
    await page.getByTestId("group-search-input").fill("  Loot ");
    await expect(page.getByTestId("group-create-button")).toContainText("Create group");
    await page.getByTestId("group-create-button").click();
    await expect(page.getByTestId("bulk-groups-dialog")).toBeHidden();

    await expect(page.getByText("Created Loot and added 2 mods to it")).toBeVisible();
    await expect(active.getByTestId("mod-row-mod.003")).toContainText("Loot");
    await expect(active.getByTestId("mod-row-mod.004")).toContainText("Loot");
    const creates = await organizationRequests(page, "create_group");
    expect(creates).toEqual([{ name: "Loot", parent: null, modIds: ["mod.003", "mod.004"] }]);
    // One create call and no separate add: the create and the add are one save.
    expect(await organizationRequests(page, "add_group_members")).toEqual([]);
    await expectNothingMoved(page, before);
  });

  test("creates a subgroup from the dialog, offers no create for an existing name, and filters", async ({
    page,
  }) => {
    const before = await openMods(page);
    await createGroup(page, "Weapons");
    const active = modsPane(page, "active");
    await selectRows(page, "active", ["mod.003"]);

    await active.getByTestId("bulk-add-to-group-active").click();
    await page.getByTestId("group-search-input").fill("weapons");
    // An existing name (in any letter case) offers no create; the list shows it.
    await expect(page.getByTestId("group-create-option")).toHaveCount(0);
    await expect(page.getByTestId("bulk-target-1")).toBeVisible();

    await page.getByTestId("group-search-input").fill("Melee");
    await expect(page.getByTestId("bulk-groups-no-match")).toBeVisible();
    await page.getByTestId("group-create-parent").selectOption({ label: "Weapons" });
    await page.getByTestId("group-create-button").click();
    await expect(page.getByTestId("bulk-groups-dialog")).toBeHidden();

    const creates = await organizationRequests(page, "create_group");
    expect(creates.at(-1)).toEqual({ name: "Melee", parent: 1, modIds: ["mod.003"] });
    await expectNothingMoved(page, before);
  });

  test("creates a group from a single mod's Groups dialog and shows it checked", async ({
    page,
  }) => {
    const before = await openMods(page);

    await openRowMenu(page, "active", "mod.003");
    await page.getByTestId("mod-menu-groups").click();
    await page.getByTestId("group-search-input").fill("Favourites");
    await page.getByTestId("group-create-button").click();

    await expect(page.getByTestId("mod-groups-dialog").getByLabel(/^Favourites$/)).toBeChecked();
    await expect(page.getByTestId("mod-groups-dialog")).toBeVisible();
    const creates = await organizationRequests(page, "create_group");
    expect(creates).toEqual([{ name: "Favourites", parent: null, modIds: ["mod.003"] }]);
    await page.getByTestId("mod-groups-done").click();
    await expectNothingMoved(page, before);
  });

  test("a double click on an Active row opens the deactivate plan, and confirming deactivates only it", async ({
    page,
  }) => {
    await openMods(page);
    const row = modsPane(page, "active").getByTestId("mod-row-mod.003");

    await row.dblclick();

    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeVisible();
    expect(await page.evaluate(() => window.__DEACTIVATE_CALLS__ ?? [])).toEqual([]);
    await page.getByTestId("deactivate-confirm-submit").click();
    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeHidden();
    expect(await page.evaluate(() => window.__DEACTIVATE_CALLS__ ?? [])).toEqual([
      { ids: ["mod.003"] },
    ]);
  });

  test("a double click on an Inactive row opens the activate plan, and Cancel changes nothing", async ({
    page,
  }) => {
    await openMods(page);
    const row = modsPane(page, "inactive").locator('[role="row"]').first();

    await row.dblclick();

    await expect(page.getByTestId("activate-confirm-dialog")).toBeVisible();
    await page.getByTestId("activate-confirm-cancel").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([]);
  });

  test("a Ctrl or Shift double click is selection, never an activation", async ({ page }) => {
    await openMods(page);
    const active = modsPane(page, "active");
    // One row is selected first so the selection bar is already shown and no row shifts between
    // the two clicks of a double click.
    await selectRows(page, "active", ["mod.005"]);
    await expect(active.getByTestId("mods-selected-count")).toContainText("1 selected");

    // Ctrl+click toggles, so the two clicks leave the row as it was: unselected.
    await active.getByTestId("mod-row-mod.003").dblclick({ modifiers: ["Control"] });
    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeHidden();
    await expect(active.getByTestId("mods-selected-count")).toContainText("1 selected");

    // Shift+click extends a range from the anchor (mod.003, the row just toggled).
    await active.getByTestId("mod-row-mod.004").dblclick({ modifiers: ["Shift"] });
    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeHidden();
    await expect(active.getByTestId("mods-selected-count")).toContainText("3 selected");
    expect(await page.evaluate(() => window.__DEACTIVATE_CALLS__ ?? [])).toEqual([]);
  });

  test("sets and clears a colour from the swatches in the row menu", async ({ page }) => {
    const before = await openMods(page);
    const row = modsPane(page, "active").getByTestId("mod-row-mod.003");

    await openRowMenu(page, "active", "mod.003");
    await page.getByTestId("mod-menu-swatch-blue").click();
    await expect(row.getByTestId("mod-row-color")).toHaveAttribute("data-color", "blue");
    await expect(row).toContainText("Colour: Blue");

    await openRowMenu(page, "active", "mod.003");
    await expect(page.getByTestId("mod-menu-swatch-blue")).toHaveAttribute("aria-pressed", "true");
    await page.getByTestId("mod-menu-swatch-none").click();
    await expect(row.getByTestId("mod-row-color")).toHaveCount(0);

    const requests = await organizationRequests(page, "set_mod_colors");
    expect(requests).toEqual([
      { modIds: ["mod.003"], color: "blue" },
      { modIds: ["mod.003"], color: null },
    ]);
    await expectNothingMoved(page, before);
  });

  test("the Colour entry still opens the dialog beside the swatches", async ({ page }) => {
    await openMods(page);

    await openRowMenu(page, "active", "mod.003");
    await page.getByTestId("mod-menu-color").click();

    await expect(page.getByTestId("color-dialog")).toBeVisible();
  });

  test("a group's colour shows faded on its members' rows and names the group in text", async ({
    page,
  }) => {
    await openMods(page);
    await createGroup(page, "Weapons");
    await page.evaluate(async () => {
      await window.__TAURI_INTERNALS__.invoke("set_group_color", {
        request: { id: 1, color: "red" },
      });
      await window.__TAURI_INTERNALS__.invoke("add_group_members", {
        request: { id: 1, modIds: ["mod.003"] },
      });
    });
    const row = modsPane(page, "active").getByTestId("mod-row-mod.003");

    await expect(row.getByTestId("mod-row-color")).toHaveAttribute("data-color", "red");
    await expect(row.getByTestId("mod-row-color")).toHaveAttribute("data-inherited", "true");
    await expect(row.getByTestId("mod-row-color-name")).toHaveText(
      "Colour from group Weapons: Red",
    );
  });

  test("clicking a group chip on a row scopes both lists to that group", async ({ page }) => {
    await openMods(page);
    await createGroup(page, "Weapons");
    await page.evaluate(() =>
      window.__TAURI_INTERNALS__.invoke("add_group_members", {
        request: { id: 1, modIds: ["mod.003"] },
      }),
    );
    const row = modsPane(page, "active").getByTestId("mod-row-mod.003");

    await row.getByTestId("mod-row-group-1").click();

    await expect(page.getByTestId("group-scope-text")).toContainText("Weapons");
    await expect(modsPane(page, "active").getByTestId("mod-row-mod.004")).toHaveCount(0);
    await expect(page.getByTestId("mod-info-panel")).toBeHidden();
  });

  test("the groups panel highlights the open mod's groups", async ({ page }) => {
    await openMods(page);
    await createGroup(page, "Weapons");
    await createGroup(page, "Armor");
    await page.evaluate(() =>
      window.__TAURI_INTERNALS__.invoke("add_group_members", {
        request: { id: 1, modIds: ["mod.003"] },
      }),
    );

    await modsPane(page, "active").getByTestId("mod-row-name-mod.003").click();

    // A dot, not the scope or drop tint, so the open mod's groups never read as the scope.
    await expect(page.getByTestId("group-node-1").getByTestId("group-node-open-dot")).toBeVisible();
    await expect(page.getByTestId("group-node-1")).not.toHaveClass(/(^|\s)bg-accent-soft/);
    await expect(page.getByTestId("group-node-1").getByTestId("group-node-open-mod")).toHaveText(
      "The open mod is in this group",
    );
    await expect(page.getByTestId("group-node-2").getByTestId("group-node-open-dot")).toHaveCount(
      0,
    );
  });

  test("creates and adds from the keyboard, picking the parent with the keys", async ({ page }) => {
    await openMods(page);
    await createGroup(page, "Weapons");
    await openRowMenu(page, "active", "mod.003");
    await page.getByTestId("mod-menu-groups").click();
    const input = page.getByTestId("group-search-input");
    await input.fill("Melee");

    await page.keyboard.press("Tab");
    await expect(page.getByTestId("group-create-button")).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(page.getByTestId("group-create-parent")).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Shift+Tab");
    await expect(page.getByTestId("group-create-button")).toBeFocused();
    await page.keyboard.press("Enter");

    await expect(page.getByTestId("mod-groups-dialog").getByLabel(/^Melee$/)).toBeChecked();
    const creates = await organizationRequests(page, "create_group");
    expect(creates.at(-1)).toEqual({ name: "Melee", parent: 1, modIds: ["mod.003"] });
  });

  test("arrow keys and Enter on a row open it with the ring and tint", async ({ page }) => {
    await page.setViewportSize(WIDE);
    await openMods(page);
    const active = modsPane(page, "active");
    const first = active.getByTestId("mod-row-mod.003");
    const second = active.getByTestId("mod-row-mod.004");

    await first.focus();
    await page.keyboard.press("ArrowDown");
    await expect(second).toBeFocused();
    await expect(second).toHaveAttribute("aria-current", "true");
    await expect(second).toHaveClass(/(^|\s)bg-accent-soft/);
    await expect(second).toHaveClass(/inset-ring-2/);

    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Enter");
    await expect(first).toHaveAttribute("aria-current", "true");
    await expect(second).not.toHaveAttribute("aria-current", "true");
  });
});
