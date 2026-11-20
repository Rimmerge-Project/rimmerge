import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import {
  addToGroup,
  createGroup,
  expectNothingMoved,
  groupMenu,
  node,
  openMods,
  openRowMenu,
  WIDE,
  workingIds,
} from "./organization-support";
import { loadScenario, modsPane } from "./support";

/**
 * The group tree on the Mods page (#40 batch C): create, nest, rename, colour, delete, scope both
 * lists, chips and the colour stripe on rows, the unusable-file notice. Organization never moves
 * a mod, so every spec ends with `expectNothingMoved`: the activation, deactivation, move, apply,
 * discard, revert and import recorders are empty, the working ids are exactly what they were when
 * the page opened, and (where the spec edits organization) only organization commands were sent.
 */

test.describe("mods groups", () => {
  test("creates, nests and renames groups, refuses a move into a subgroup, and never moves a mod", async ({
    page,
  }) => {
    const before = await openMods(page);

    await createGroup(page, "Weapons");
    await groupMenu(page, "Weapons", "new-subgroup");
    await expect(page.getByTestId("group-name-dialog")).toContainText("Inside Weapons");
    await page.getByTestId("group-name-input").fill("Melee");
    await page.getByTestId("group-name-submit").click();
    await expect(node(page, "Melee")).toBeVisible();

    // A duplicate sibling name is refused with its reason, inside the dialog.
    await groupMenu(page, "Weapons", "new-subgroup");
    await page.getByTestId("group-name-input").fill("melee");
    await page.getByTestId("group-name-submit").click();
    await expect(page.getByTestId("organization-error")).toContainText(
      "That group name is already used",
    );
    await page.getByTestId("group-name-cancel").click();

    await groupMenu(page, "Melee", "rename");
    await page.getByTestId("group-name-input").fill("Blades");
    await page.getByTestId("group-name-submit").click();
    await expect(node(page, "Blades")).toBeVisible();

    // Weapons cannot go inside its own subgroup: listed, disabled, with the reason.
    await groupMenu(page, "Weapons", "move");
    const dialog = page.getByTestId("group-move-dialog");
    await expect(dialog.locator('[data-testid^="group-move-target-"]').first()).toBeDisabled();
    await expect(dialog.locator('[data-testid^="group-move-reason-"]').first()).toContainText(
      "can't go inside itself",
    );
    await page.getByTestId("group-move-cancel").click();

    await expectNothingMoved(page, before);
  });

  test("scopes both lists to a group with its subgroups, keeps load order, and searches inside the scope", async ({
    page,
  }) => {
    const before = await openMods(page);
    await createGroup(page, "Weapons");
    await groupMenu(page, "Weapons", "new-subgroup");
    await page.getByTestId("group-name-input").fill("Melee");
    await page.getByTestId("group-name-submit").click();
    await expect(node(page, "Melee")).toBeVisible();

    await addToGroup(page, "active", "mod.003", "Weapons");
    await addToGroup(page, "active", "mod.004", "Melee");
    await addToGroup(page, "inactive", "mod.901", "Melee");
    await expect(modsPane(page, "active").getByTestId("mod-row-mod.003")).toContainText("Weapons");

    await node(page, "Weapons").click();

    await expect(page.getByTestId("group-scope-text")).toContainText("Weapons");
    await expect(page.getByTestId("group-scope-text")).toContainText("3 mods, including subgroups");
    const active = modsPane(page, "active");
    await expect(active.getByTestId("mods-total")).toContainText("2 mods");
    await expect(modsPane(page, "inactive").getByTestId("mods-total")).toContainText("1 mod");
    // Still the load order: reorder stays on, the rows keep their relative order.
    await expect(page.getByTestId("mods-reorder-blocked")).toHaveCount(0);
    const rowIds = await active
      .locator('[data-testid^="mod-row-mod."]')
      .evaluateAll((rows) =>
        rows
          .map((row) => row.getAttribute("data-testid"))
          .filter((id) => /mod-row-mod\.\d+$/.test(id ?? "")),
      );
    expect(rowIds).toEqual(["mod-row-mod.003", "mod-row-mod.004"]);

    // Search runs inside the scope: another group's mod is not found, and chips stay visible.
    await active.getByTestId("mods-search").fill("mod.004");
    await expect(active.getByTestId("mod-row-mod.004")).toBeVisible();
    await expect(active.getByTestId("mod-row-mod.003")).toHaveCount(0);
    await expect(active.getByTestId("mod-row-mod.004")).toContainText("Melee");
    await active.getByTestId("mods-search").fill("mod.010");
    await expect(active.getByTestId("mods-total")).toContainText("0 mods");

    await page.getByTestId("group-scope-clear").click();
    await active.getByTestId("mods-search").fill("");
    await expect(page.getByTestId("group-scope-bar")).toHaveCount(0);

    await node(page, "Ungrouped").click();
    await expect(active.getByTestId("mod-row-mod.003")).toHaveCount(0);
    await expect(active.getByTestId("mod-row-mod.005")).toBeVisible();

    await expectNothingMoved(page, before);
  });

  test("deleting a group keeps its mods in both lists and asks about subgroups", async ({
    page,
  }) => {
    const before = await openMods(page);
    await createGroup(page, "Weapons");
    await groupMenu(page, "Weapons", "new-subgroup");
    await page.getByTestId("group-name-input").fill("Melee");
    await page.getByTestId("group-name-submit").click();
    await addToGroup(page, "active", "mod.003", "Weapons");
    await addToGroup(page, "inactive", "mod.901", "Melee");

    await groupMenu(page, "Weapons", "delete");
    const dialog = page.getByTestId("group-delete-dialog");
    await expect(dialog).toContainText("Delete Weapons?");
    await expect(page.getByTestId("group-delete-safety")).toHaveText(
      "No mod is deleted or uninstalled. Mods stay in any other group they belong to.",
    );
    await expect(page.getByTestId("group-delete-lift")).toBeChecked();
    await page.getByTestId("group-delete-submit").click();
    await expect(dialog).toBeHidden();

    // The subgroup moved up a level; both mods are still listed.
    await expect(node(page, "Weapons")).toHaveCount(0);
    await expect(node(page, "Melee")).toBeVisible();
    await expect(modsPane(page, "active").getByTestId("mod-row-mod.003")).toBeVisible();
    await expect(modsPane(page, "inactive").getByTestId("mod-row-mod.901")).toBeVisible();
    await expect(modsPane(page, "active").getByTestId("mod-row-mod.003")).not.toContainText(
      "Weapons",
    );

    await groupMenu(page, "Melee", "delete");
    await expect(page.getByTestId("group-delete-lift")).toHaveCount(0);
    await page.getByTestId("group-delete-submit").click();
    await expect(node(page, "Melee")).toHaveCount(0);
    await expect(modsPane(page, "inactive").getByTestId("mod-row-mod.901")).toBeVisible();

    await expectNothingMoved(page, before);
  });

  test("deleting the scoped group drops the scope back to every mod", async ({ page }) => {
    const before = await openMods(page);
    await createGroup(page, "Weapons");
    await addToGroup(page, "active", "mod.003", "Weapons");
    await node(page, "Weapons").click();
    await expect(page.getByTestId("group-scope-bar")).toBeVisible();

    await groupMenu(page, "Weapons", "delete");
    await page.getByTestId("group-delete-submit").click();

    await expect(page.getByTestId("group-scope-bar")).toHaveCount(0);
    await expect(modsPane(page, "active").getByTestId("mod-row-mod.005")).toBeVisible();
    await expectNothingMoved(page, before);
  });

  test("after a group is deleted from the keyboard, focus lands on its next sibling", async ({
    page,
  }) => {
    const before = await openMods(page);
    await createGroup(page, "Alpha");
    await createGroup(page, "Beta");

    await node(page, "Alpha").focus();
    await page.keyboard.press("Delete");
    await page.getByTestId("group-delete-submit").click();
    await expect(page.getByTestId("group-delete-dialog")).toBeHidden();

    await expect(node(page, "Alpha")).toHaveCount(0);
    await expect(node(page, "Beta")).toBeFocused();
    await expectNothingMoved(page, before);
  });

  test("an inactive row's menu opened from the keyboard has its first item ready for Enter", async ({
    page,
  }) => {
    const before = await openMods(page);
    const row = modsPane(page, "inactive").getByTestId("mod-row-mod.901");

    await row.focus();
    await page.keyboard.press("Shift+F10");
    await expect(page.getByTestId("mod-activate-at")).toBeVisible();
    await page.keyboard.press("Enter");

    await expect(page.getByTestId("mod-position-dialog")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("mod-position-dialog")).toBeHidden();
    // Nothing organization-related was sent: this only opened the Activate at dialog.
    await expectNothingMoved(page, before, { editsOrganization: false });
  });

  test("sets and resets a mod colour from its row menu, with the colour named in text", async ({
    page,
  }) => {
    const before = await openMods(page);
    const row = modsPane(page, "active").getByTestId("mod-row-mod.003");

    await openRowMenu(page, "active", "mod.003");
    await page.getByTestId("mod-menu-color").click();
    await page.getByTestId("color-option-teal").check();
    await page.getByTestId("color-apply").click();
    await expect(page.getByTestId("color-dialog")).toBeHidden();

    await expect(row.getByTestId("mod-row-color")).toHaveAttribute("data-color", "teal");
    await expect(row).toContainText("Colour: Teal");

    await openRowMenu(page, "active", "mod.003");
    await page.getByTestId("mod-menu-color").click();
    await expect(page.getByTestId("color-option-teal")).toBeChecked();
    await page.getByTestId("color-option-none").check();
    await page.getByTestId("color-apply").click();
    await expect(row.getByTestId("mod-row-color")).toHaveCount(0);

    await expectNothingMoved(page, before);
  });

  test("removes the mods that are not installed from a group in one step", async ({ page }) => {
    const before = await openMods(page);
    await createGroup(page, "Wishlist");
    await addToGroup(page, "active", "mod.003", "Wishlist");
    // A mod that is nowhere on disk: added straight through the mock's own command.
    await page.evaluate(() =>
      window.__TAURI_INTERNALS__.invoke("add_group_members", {
        request: { id: 1, modIds: ["gone.mod"] },
      }),
    );
    await node(page, "Wishlist").click();
    await expect(page.getByTestId("group-scope-not-installed")).toContainText(
      "1 mod isn't installed",
    );

    await groupMenu(page, "Wishlist", "remove-uninstalled");
    const dialog = page.getByTestId("remove-uninstalled-dialog");
    await expect(dialog).toContainText("As of the last scan, 1 mod in Wishlist");
    await expect(dialog.getByTestId("remove-uninstalled-list")).toContainText("gone.mod");
    await page.getByTestId("remove-uninstalled-confirm").click();

    await expect(page.getByTestId("group-scope-not-installed")).toHaveCount(0);
    await expect(modsPane(page, "active").getByTestId("mod-row-mod.003")).toBeVisible();
    await expectNothingMoved(page, before);
  });

  test("groups, chips and colours are unchanged after a rescan", async ({ page }) => {
    const before = await openMods(page);
    await createGroup(page, "Weapons");
    await addToGroup(page, "active", "mod.003", "Weapons");
    await node(page, "Weapons").click();

    await page.getByTestId("mods-rescan-button").click();

    await expect(node(page, "Weapons")).toBeVisible();
    await expect(page.getByTestId("group-scope-bar")).toBeVisible();
    await expect(modsPane(page, "active").getByTestId("mod-row-mod.003")).toContainText("Weapons");
    await expectNothingMoved(page, before);
  });

  test("a keyboard user can scope and open the group menu from the tree", async ({ page }) => {
    const before = await openMods(page);
    await createGroup(page, "Weapons");

    await node(page, "Weapons").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("group-scope-bar")).toBeVisible();
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("group-scope-bar")).toBeVisible();
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("group-scope-bar")).toHaveCount(0);

    await node(page, "Weapons").focus();
    await page.keyboard.press("F2");
    await expect(page.getByTestId("group-name-dialog")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("group-name-dialog")).toBeHidden();
    await node(page, "Weapons").focus();
    await page.keyboard.press("Shift+F10");
    await expect(page.getByTestId("group-menu-rename")).toBeVisible();
    // Opened from the keyboard, the first item is ready: Enter opens the New subgroup dialog.
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("group-name-dialog")).toContainText("Inside Weapons");
    await expectNothingMoved(page, before);
  });

  test("below very wide windows the groups open as a left drawer from a Groups button", async ({
    page,
  }) => {
    const before = await openMods(page, { width: 1280, height: 800 });

    await expect(page.getByTestId("group-rail")).toHaveCount(0);
    await page.getByTestId("groups-open").click();
    await expect(page.getByTestId("group-rail")).toBeVisible();
    await createGroup(page, "Weapons");
    await node(page, "Weapons").click();

    // Picking a scope answers the question the drawer was opened for, so it closes.
    await expect(page.getByTestId("group-rail")).toHaveCount(0);
    await expect(page.getByTestId("group-scope-bar")).toBeVisible();
    // Closing hands focus back to the button that opened it, never to the page body.
    await expect(page.getByTestId("groups-open")).toBeFocused();
    await page.getByTestId("groups-open").click();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("group-rail")).toHaveCount(0);
    await expect(page.getByTestId("groups-open")).toBeFocused();

    // Escape inside the group menu closes the menu, not the drawer under it.
    await page.getByTestId("groups-open").click();
    await node(page, "Weapons").click({ button: "right" });
    await expect(page.getByTestId("group-menu-rename")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("group-menu-rename")).toBeHidden();
    await expect(page.getByTestId("group-rail")).toBeVisible();
    await expectNothingMoved(page, before);
  });

  test("deleting the scoped group from the drawer leaves focus on the Groups button", async ({
    page,
  }) => {
    const before = await openMods(page, { width: 1280, height: 800 });
    await page.getByTestId("groups-open").click();
    await createGroup(page, "Weapons");
    await node(page, "Weapons").click();
    await expect(page.getByTestId("group-rail")).toHaveCount(0);
    await expect(page.getByTestId("group-scope-bar")).toBeVisible();

    await page.getByTestId("groups-open").click();
    await groupMenu(page, "Weapons", "delete");
    await page.getByTestId("group-delete-submit").click();
    await expect(page.getByTestId("group-delete-dialog")).toBeHidden();

    await expect(page.getByTestId("group-scope-bar")).toHaveCount(0);
    await expect(page.getByTestId("group-rail")).toHaveCount(0);
    await expect(page.getByTestId("groups-open")).toBeFocused();
    await expectNothingMoved(page, before);
  });

  test("a mod carrying a manual tag shows it on the Inactive list and the tag filter finds it", async ({
    page,
  }) => {
    await page.setViewportSize(WIDE);
    await loadScenario(page);
    const before = await workingIds(page);
    await page.evaluate(() =>
      window.__TAURI_INTERNALS__.invoke("set_manual_tag", {
        request: { modId: "mod.901", tag: "favourite", mode: "add" },
      }),
    );
    await page.getByTestId("nav-mods").click();

    const inactive = modsPane(page, "inactive");
    await expect(inactive.getByTestId("mod-row-mod.901")).toContainText("favourite");
    await inactive.getByTestId("mods-tag-filter").fill("favourite");
    await expect(inactive.getByTestId("mods-total")).toContainText("1 mod");
    await expectNothingMoved(page, before, { editsOrganization: false });
  });
});

test.describe("mods groups when the file is unusable", () => {
  test("shows the notice, offers Start over for a damaged file, and nothing else changes", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      window.__ORGANIZATION_STATE__ = "recovered";
      window.__ORGANIZATION_CALLS__ = [];
    });
    await page.setViewportSize(WIDE);
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    const before = await workingIds(page);

    await expect(page.getByTestId("organization-notice")).toContainText("damaged");
    await expect(page.getByTestId("group-rail")).toHaveCount(0);
    await expect(page.getByTestId("organization-try-again")).toHaveCount(0);

    await page.getByTestId("organization-start-over").click();
    await expect(page.getByTestId("start-over-dialog")).toContainText("keeps a copy");
    await page.getByTestId("start-over-confirm").click();

    await expect(page.getByTestId("organization-notice")).toHaveCount(0);
    await expect(page.getByTestId("group-rail")).toBeVisible();
    const commands = await page.evaluate(() =>
      (window.__ORGANIZATION_CALLS__ ?? []).map((call) => call.command),
    );
    expect(commands).toEqual(["reset_organization"]);
    await expectNothingMoved(page, before);
  });
});

test.describe("mods groups on a narrow row", () => {
  test("a row with a colour stripe, source, chips and the Added mark stays readable at 1024px in German", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1024, height: 800 });
    await page.addInitScript(() => {
      window.__ORGANIZATION_CALLS__ = [];
    });
    await loadScenario(page);
    const before = await workingIds(page);
    await page.getByTestId("welcome-card-keep").click();
    await page.getByTestId("nav-settings").click();
    await page.getByTestId("language-picker-select").click();
    await page.getByRole("option", { name: "Deutsch (preview)", exact: true }).click();
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await page.getByTestId("nav-mods").click();

    // The drawer is how groups are made at this width.
    await page.getByTestId("groups-open").click();
    await createGroup(page, "Waffen");
    await page.keyboard.press("Escape");
    await addToGroup(page, "active", "mod.004", "Waffen");
    await openRowMenu(page, "active", "mod.004");
    await page.getByTestId("mod-menu-color").click();
    await page.getByTestId("color-option-pink").check();
    await page.getByTestId("color-apply").click();

    const pane = modsPane(page, "active");
    await pane.getByTestId("mods-search").fill("mod.004");
    const row = pane.getByTestId("mod-row-mod.004");
    await expect(row.getByTestId("mod-row-color")).toBeVisible();
    const rowBox = await row.boundingBox();
    const nameBox = await pane.getByTestId("mod-row-name-mod.004").boundingBox();
    const menuBox = await pane.getByTestId("mod-row-menu-mod.004").boundingBox();
    if (rowBox === null || nameBox === null || menuBox === null) {
      throw new Error("the row is missing its name or menu button");
    }
    expect(nameBox.width).toBeGreaterThanOrEqual(40);
    expect(menuBox.x + menuBox.width).toBeLessThanOrEqual(rowBox.x + rowBox.width);
    // The chips are hidden at this width, so the groups must still be there as text.
    await expect(row.getByTestId("mod-row-groups-text")).toHaveText(/Waffen/);
    await expectNothingMoved(page, before);
  });
});
