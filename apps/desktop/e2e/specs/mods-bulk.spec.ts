import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { createGroup, expectNothingMoved, groupMenu, node, openMods } from "./organization-support";
import { modsPane } from "./support";

/**
 * Bulk organization actions on a list's selection (#40 batch D): add, remove and move group
 * membership, bulk colour, bulk tags, and group-level activate and deactivate. Organization and
 * tag actions never change the selection, the working set, the order or the pending changes
 * (`expectNothingMoved`); group-level enable and disable only open the existing plan dialogs, and
 * nothing is applied until their confirm.
 */

type Pane = "active" | "inactive";

/** Checks the rows of `ids` in a list's selection column. */
async function selectRows(page: Page, pane: Pane, ids: readonly string[]): Promise<void> {
  for (const id of ids) {
    await modsPane(page, pane).getByTestId(`mod-row-select-${id}`).locator("input").check();
  }
}

/** Adds mods to a group straight through the command, as a setup step (recorded like any edit). */
async function seedMembers(page: Page, groupId: number, modIds: readonly string[]): Promise<void> {
  await page.evaluate(
    ([id, ids]) =>
      window.__TAURI_INTERNALS__.invoke("add_group_members", {
        request: { id, modIds: ids },
      }),
    [groupId, [...modIds]] as const,
  );
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

/** Waits for any closing dialog's mask, so a toolbar click is not retried under a fading mask. */
async function settled(page: Page): Promise<void> {
  await expect(page.locator(".p-overlay-mask")).toHaveCount(0);
}

test.describe("mods bulk organization", () => {
  test("adds the selection to a group with one call and keeps the selection and order", async ({
    page,
  }) => {
    const before = await openMods(page);
    await createGroup(page, "Weapons");
    await seedMembers(page, 1, ["mod.003"]);
    const active = modsPane(page, "active");
    await selectRows(page, "active", ["mod.003", "mod.004", "mod.005"]);
    await expect(active.getByTestId("mods-selected-count")).toContainText("3 selected");

    await active.getByTestId("bulk-add-to-group-active").click();
    await expect(page.getByTestId("bulk-groups-selection")).toHaveText("3 selected");
    await page.getByTestId("bulk-target-1").check();
    await page.getByTestId("bulk-groups-submit").click();
    await expect(page.getByTestId("bulk-groups-dialog")).toBeHidden();

    await expect(page.getByText("Added 2 mods to Weapons")).toBeVisible();
    await expect(page.getByText("1 was already in the group")).toBeVisible();
    await expect(active.getByTestId("mod-row-mod.004")).toContainText("Weapons");
    await expect(active.getByTestId("mod-row-mod.005")).toContainText("Weapons");
    // The selection is exactly what it was, and one call carried the three ids.
    await expect(active.getByTestId("mods-selected-count")).toContainText("3 selected");
    const adds = await organizationRequests(page, "add_group_members");
    expect(adds.at(-1)).toEqual({ id: 1, modIds: ["mod.003", "mod.004", "mod.005"] });
    await expectNothingMoved(page, before);
  });

  test("acts on selected rows the filter hides and says how many are not shown", async ({
    page,
  }) => {
    const before = await openMods(page);
    await createGroup(page, "Weapons");
    const active = modsPane(page, "active");
    await selectRows(page, "active", ["mod.003", "mod.004"]);
    await active.getByTestId("mods-search").fill("mod.003");
    await expect(active.getByTestId("mod-row-mod.004")).toHaveCount(0);

    await active.getByTestId("bulk-add-to-group-active").click();
    await expect(page.getByTestId("bulk-groups-selection")).toHaveText("2 selected (1 not shown)");
    await page.getByTestId("bulk-target-1").check();
    await page.getByTestId("bulk-groups-submit").click();
    await expect(page.getByTestId("bulk-groups-dialog")).toBeHidden();

    const adds = await organizationRequests(page, "add_group_members");
    expect(adds.at(-1)).toEqual({ id: 1, modIds: ["mod.003", "mod.004"] });
    await expectNothingMoved(page, before);
  });

  test("lists only the groups that hold a selected mod for remove, and removes from the chosen one", async ({
    page,
  }) => {
    const before = await openMods(page);
    await createGroup(page, "Weapons");
    await createGroup(page, "Armor");
    await createGroup(page, "Empty");
    await seedMembers(page, 1, ["mod.003", "mod.004"]);
    await seedMembers(page, 2, ["mod.005"]);
    const active = modsPane(page, "active");
    await selectRows(page, "active", ["mod.003", "mod.004", "mod.006"]);

    await active.getByTestId("bulk-remove-from-group-active").click();
    await expect(page.getByTestId("bulk-target-1")).toBeVisible();
    await expect(page.getByTestId("bulk-target-count-1")).toHaveText("2 selected mods are in it");
    await expect(page.getByTestId("bulk-target-2")).toHaveCount(0);
    await expect(page.getByTestId("bulk-target-3")).toHaveCount(0);
    await page.getByTestId("bulk-target-1").check();
    await page.getByTestId("bulk-groups-submit").click();
    await expect(page.getByTestId("bulk-groups-dialog")).toBeHidden();

    await expect(page.getByText("Removed 2 mods from Weapons")).toBeVisible();
    await expect(active.getByTestId("mod-row-mod.003")).not.toContainText("Weapons");
    await expect(active.getByTestId("mod-row-mod.005")).toContainText("Armor");
    await expectNothingMoved(page, before);
  });

  test("moves from a chosen source, leaving other memberships and mods outside it alone", async ({
    page,
  }) => {
    const before = await openMods(page);
    await createGroup(page, "Weapons");
    await createGroup(page, "Armor");
    await createGroup(page, "Utility");
    await seedMembers(page, 1, ["mod.003", "mod.004"]);
    await seedMembers(page, 3, ["mod.003"]);
    const active = modsPane(page, "active");
    await selectRows(page, "active", ["mod.003", "mod.004", "mod.005"]);

    await active.getByTestId("bulk-move-to-group-active").click();
    await page.getByTestId("bulk-source-1").check();
    // The source cannot be its own target, and says why.
    await expect(page.getByTestId("bulk-target-1")).toBeDisabled();
    await expect(page.getByTestId("bulk-target-reason-1")).toHaveText("Same as the source");
    await page.getByTestId("bulk-target-2").check();
    await page.getByTestId("bulk-groups-submit").click();
    await expect(page.getByTestId("bulk-groups-dialog")).toBeHidden();

    await expect(page.getByText("Moved 2 mods from Weapons to Armor")).toBeVisible();
    await expect(page.getByText("1 was not in Weapons, so it did not move")).toBeVisible();
    await expect(active.getByTestId("mod-row-mod.003")).toContainText("Armor");
    await expect(active.getByTestId("mod-row-mod.003")).toContainText("Utility");
    await expect(active.getByTestId("mod-row-mod.003")).not.toContainText("Weapons");
    await expect(active.getByTestId("mod-row-mod.005")).not.toContainText("Armor");
    const moves = await organizationRequests(page, "move_group_members");
    expect(moves.at(-1)).toEqual({
      from: 1,
      to: 2,
      modIds: ["mod.003", "mod.004", "mod.005"],
    });
    await expectNothingMoved(page, before);
  });

  test("sets and resets the colour of the whole selection, named in text", async ({ page }) => {
    const before = await openMods(page);
    const active = modsPane(page, "active");
    await selectRows(page, "active", ["mod.003", "mod.004"]);

    await active.getByTestId("bulk-color-active").click();
    await expect(page.getByTestId("color-scope")).toHaveText("2 selected");
    // Nothing is picked at first, so one click cannot clear the colour of every selected mod.
    await expect(page.getByTestId("color-apply")).toBeDisabled();
    await page.getByTestId("color-option-teal").check();
    await page.getByTestId("color-apply").click();
    await expect(page.getByTestId("color-dialog")).toBeHidden();

    await expect(page.getByText("Colour set for 2 mods")).toBeVisible();
    for (const id of ["mod.003", "mod.004"]) {
      await expect(
        active.getByTestId(`mod-row-${id}`).getByTestId("mod-row-color"),
      ).toHaveAttribute("data-color", "teal");
      await expect(active.getByTestId(`mod-row-${id}`)).toContainText("Colour: Teal");
    }
    await expect(active.getByTestId("mods-selected-count")).toContainText("2 selected");

    await settled(page);
    await active.getByTestId("bulk-color-active").click();
    await page.getByTestId("color-option-none").check();
    await page.getByTestId("color-apply").click();
    await expect(page.getByText("Colour cleared for 2 mods")).toBeVisible();
    await expect(active.getByTestId("mod-row-mod.003").getByTestId("mod-row-color")).toHaveCount(0);
    await expectNothingMoved(page, before);
  });

  test("tags inactive mods in one call and shows the tag on their rows at once", async ({
    page,
  }) => {
    const before = await openMods(page);
    const inactive = modsPane(page, "inactive");
    await selectRows(page, "inactive", ["mod.901", "mod.902"]);

    await inactive.getByTestId("bulk-tags-inactive").click();
    // Only a valid slug can be sent; what was typed is never rewritten.
    await page.getByTestId("bulk-tags-input").fill("Not A Slug");
    await expect(page.getByTestId("bulk-tags-add")).toBeDisabled();
    await page.getByTestId("bulk-tags-input").fill("framework");
    await page.getByTestId("bulk-tags-add").click();
    await expect(page.getByTestId("bulk-tags-dialog")).toBeHidden();

    await expect(page.getByText("Tag framework added to 2 mods")).toBeVisible();
    await expect(inactive.getByTestId("mod-row-mod.901")).toContainText("framework");
    await expect(inactive.getByTestId("mod-row-mod.902")).toContainText("framework");
    await expect(inactive.getByTestId("mods-selected-count")).toContainText("2 selected");
    const calls = await page.evaluate(() => window.__SET_MANUAL_TAGS_CALLS__ ?? []);
    expect(calls).toEqual([{ modIds: ["mod.901", "mod.902"], tag: "framework", mode: "add" }]);

    await settled(page);
    await inactive.getByTestId("bulk-tags-inactive").click();
    await page.getByTestId("bulk-tags-input").fill("framework");
    await page.getByTestId("bulk-tags-remove").click();
    await expect(page.getByText("Tag framework removed from 2 mods")).toBeVisible();
    await expect(inactive.getByTestId("mod-row-mod.901")).not.toContainText("framework");
    // Tags are rules data, not organization: only the tag recorder saw them.
    await expectNothingMoved(page, before, { editsOrganization: false });
  });

  test("removes a tag only from the selected mods that show it and counts the rest as unchanged", async ({
    page,
  }) => {
    const before = await openMods(page);
    const inactive = modsPane(page, "inactive");
    await selectRows(page, "inactive", ["mod.901"]);
    await inactive.getByTestId("bulk-tags-inactive").click();
    await page.getByTestId("bulk-tags-input").fill("framework");
    await page.getByTestId("bulk-tags-add").click();
    await expect(page.getByTestId("bulk-tags-dialog")).toBeHidden();
    await expect(inactive.getByTestId("mod-row-mod.901")).toContainText("framework");
    await settled(page);

    // Two more mods join the selection; only 901 shows the tag.
    await selectRows(page, "inactive", ["mod.902", "mod.903"]);
    await inactive.getByTestId("bulk-tags-inactive").click();
    await page.getByTestId("bulk-tags-input").fill("framework");
    await page.getByTestId("bulk-tags-remove").click();

    await expect(page.getByText("Tag framework removed from 1 mod")).toBeVisible();
    await expect(page.getByText("2 did not change")).toBeVisible();
    await expect(inactive.getByTestId("mod-row-mod.901")).not.toContainText("framework");
    const calls = await page.evaluate(() => window.__SET_MANUAL_TAGS_CALLS__ ?? []);
    expect(calls.at(-1)).toEqual({
      modIds: ["mod.901", "mod.902", "mod.903"],
      tag: "framework",
      mode: "remove",
    });
    await expectNothingMoved(page, before, { editsOrganization: false });
  });

  test("pins a tag a mod only shows by inference and counts it changed, once", async ({ page }) => {
    const before = await openMods(page);
    const active = modsPane(page, "active");
    // mod.009 already shows `framework` by inference in this scenario.
    await selectRows(page, "active", ["mod.009"]);
    await expect(active.getByTestId("mod-row-mod.009")).toContainText("framework");

    await active.getByTestId("bulk-tags-active").click();
    await page.getByTestId("bulk-tags-input").fill("framework");
    await page.getByTestId("bulk-tags-add").click();
    await expect(page.getByText("Tag framework added to 1 mod")).toBeVisible();
    await settled(page);

    await active.getByTestId("bulk-tags-active").click();
    await page.getByTestId("bulk-tags-input").fill("framework");
    await page.getByTestId("bulk-tags-add").click();
    await expect(page.getByText("Tag framework added to 0 mods")).toBeVisible();
    await expect(page.getByText("1 did not change")).toBeVisible();
    await expectNothingMoved(page, before, { editsOrganization: false });
  });

  test("hides the group and colour actions when organization is unusable, but keeps tags", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      window.__ORGANIZATION_STATE__ = "notLoaded";
    });
    await openMods(page);
    await selectRows(page, "active", ["mod.003"]);

    const active = modsPane(page, "active");
    await expect(active.getByTestId("bulk-tags-active")).toBeVisible();
    await expect(active.getByTestId("bulk-add-to-group-active")).toHaveCount(0);
    await expect(active.getByTestId("bulk-color-active")).toHaveCount(0);
  });

  test("shows no bulk actions without a selection", async ({ page }) => {
    await openMods(page);

    await expect(modsPane(page, "active").getByTestId("mods-bulk-bar-active")).toHaveCount(0);
  });
});

test.describe("mods group-level activate and deactivate", () => {
  test("plans a group's inactive mods through the Activate dialog with the scope line, applying nothing until confirm", async ({
    page,
  }) => {
    await openMods(page);
    await createGroup(page, "Weapons");
    await groupMenu(page, "Weapons", "new-subgroup");
    await page.getByTestId("group-name-input").fill("Melee");
    await page.getByTestId("group-name-submit").click();
    await expect(node(page, "Melee")).toBeVisible();
    // 901 is in both groups (counted once), 902 only in the subgroup, 003 is already active.
    await seedMembers(page, 1, ["mod.901", "mod.003"]);
    await seedMembers(page, 2, ["mod.901", "mod.902"]);
    await settled(page);

    await node(page, "Weapons").click({ button: "right" });
    const item = page.getByTestId("group-menu-activate-all");
    await expect(item).toHaveText("Activate 2 mods…");
    await item.click();

    const dialog = page.getByTestId("activate-confirm-dialog");
    await expect(dialog).toBeVisible();
    await expect(page.getByTestId("activate-scope")).toContainText(
      "Activate 2 mods, including subgroups",
    );
    await expect(page.getByTestId("activate-plan-to-add")).toHaveCount(2);
    // Planned, not applied: no activation and no apply yet.
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([]);

    await page.getByTestId("activate-confirm-submit").click();
    await expect(dialog).toBeHidden();
    const calls = await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? []);
    expect(calls).toHaveLength(1);
    expect([...(calls[0]?.ids ?? [])].sort()).toEqual(["mod.901", "mod.902"]);
    await expect(page.getByTestId("pending-changes-banner")).toBeVisible();
    // Still a pending change: nothing was written.
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([]);
  });

  test("Deactivate all refuses Core in the existing dialog and discloses mods the filter hides", async ({
    page,
  }) => {
    await openMods(page);
    await createGroup(page, "Weapons");
    await seedMembers(page, 1, ["ludeon.rimworld", "mod.003", "mod.004"]);
    const active = modsPane(page, "active");
    await active.getByTestId("mods-search").fill("mod.003");
    await expect(active.getByTestId("mod-row-mod.004")).toHaveCount(0);
    await settled(page);

    await node(page, "Weapons").click({ button: "right" });
    const item = page.getByTestId("group-menu-deactivate-all");
    await expect(item).toHaveText("Deactivate 3 mods…");
    await item.click();

    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeVisible();
    await expect(page.getByTestId("deactivate-scope")).toContainText(
      "Deactivate 3 mods in this group",
    );
    await expect(page.getByTestId("deactivate-scope")).toContainText(
      "2 are not shown by the current filter",
    );
    await expect(page.getByTestId("deactivate-plan-refused")).toHaveCount(1);
    await page.getByTestId("deactivate-confirm-cancel").click();
    expect(await page.evaluate(() => window.__DEACTIVATE_CALLS__ ?? [])).toEqual([]);
  });

  test("offers nothing to activate for a group whose mods are all active", async ({ page }) => {
    await openMods(page);
    await createGroup(page, "Weapons");
    await seedMembers(page, 1, ["mod.003"]);
    await settled(page);

    await node(page, "Weapons").click({ button: "right" });

    const item = page.getByTestId("group-menu-activate-all");
    await expect(item).toHaveText("No mods to activate");
    await expect(item.locator("xpath=ancestor::li[1]")).toHaveAttribute("data-p-disabled", "true");
  });

  test("a committed group activation leaves selected mods outside the group selected", async ({
    page,
  }) => {
    await openMods(page);
    await createGroup(page, "Weapons");
    await seedMembers(page, 1, ["mod.901", "mod.902"]);
    await settled(page);
    const inactive = modsPane(page, "inactive");
    // 902 is in the group; 903 is not, and 901 (in the group) was never selected.
    await selectRows(page, "inactive", ["mod.902", "mod.903"]);

    await node(page, "Weapons").click({ button: "right" });
    await page.getByTestId("group-menu-activate-all").click();
    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();

    await expect(inactive.getByTestId("mod-row-mod.902")).toHaveCount(0);
    await expect(inactive.getByTestId("mods-selected-count")).toContainText("1 selected");
    await expect(inactive.getByTestId("mod-row-select-mod.903").locator("input")).toBeChecked();
  });

  test("a selection button opened after a group's dialog carries no scope line", async ({
    page,
  }) => {
    await openMods(page);
    await createGroup(page, "Weapons");
    await seedMembers(page, 1, ["mod.901"]);
    await settled(page);
    await node(page, "Weapons").click({ button: "right" });
    await page.getByTestId("group-menu-activate-all").click();
    await expect(page.getByTestId("activate-scope")).toBeVisible();
    await page.getByTestId("activate-confirm-cancel").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();

    await selectRows(page, "inactive", ["mod.902"]);
    await page.getByTestId("activate-selected-button").click();

    await expect(page.getByTestId("activate-confirm-dialog")).toBeVisible();
    await expect(page.getByTestId("activate-scope")).toHaveCount(0);
  });
});
