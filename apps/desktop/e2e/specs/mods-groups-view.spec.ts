import { expect, type Locator, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { groupMenu, node, openMods, WIDE } from "./organization-support";
import { loadScenario, modsPane } from "./support";

/**
 * The Groups view of the Mods page (#54): each pane's `List | Groups` switch, the folders as a
 * treegrid (nesting, order, "also in"), folds that persist, the keyboard model, selection in the
 * view, dropping mods on a folder header, the folder menu, and the notes around it. The library
 * is seeded through the organization commands as a setup step:
 *
 *   Weapons (1)             mods 003 004 | inactive 901
 *     Melee (2)             mods 004 005
 *       Blades (3)          mod 006
 *   Armor (4)               mod 003 | inactive 902
 *   Spare (5)               empty
 *
 * Folders list as Armor, Spare, Weapons (name order), then "Not in a group" last.
 */

type Pane = "active" | "inactive";

/** Creates the library above, in order, so the ids are 1 to 5. */
async function seedLibrary(): Promise<void> {
  const create = (name: string, parent: number | null, modIds: string[]) =>
    window.__TAURI_INTERNALS__.invoke("create_group", { request: { name, parent, modIds } });
  await create("Weapons", null, ["mod.003", "mod.004", "mod.901"]);
  await create("Melee", 1, ["mod.004", "mod.005"]);
  await create("Blades", 2, ["mod.006"]);
  await create("Armor", null, ["mod.003", "mod.902"]);
  await create("Spare", null, []);
}

/** The keys of the rows rendered in a pane's Groups view, in screen order. */
function entryKeys(page: Page, pane: Pane): Promise<string[]> {
  return modsPane(page, pane)
    .locator("[data-entry-key]")
    .evaluateAll((rows) => rows.map((row) => (row as HTMLElement).dataset["entryKey"] ?? ""));
}

/** The first rows of the Active pane, as they must read with every folder open. */
const ACTIVE_OPEN_HEAD = [
  "f:4",
  "m:4:mod.003",
  "f:5",
  "f:1",
  "f:2",
  "f:3",
  "m:3:mod.006",
  "m:2:mod.004",
  "m:2:mod.005",
  "m:1:mod.003",
  "m:1:mod.004",
  "f:u",
];

/** Seeds the library, opens the Mods page and switches `pane` to Groups. */
async function openGroupsView(page: Page, pane: Pane = "active"): Promise<void> {
  await openMods(page);
  await page.evaluate(seedLibrary);
  await showGroups(page, pane);
}

async function showGroups(page: Page, pane: Pane): Promise<void> {
  await modsPane(page, pane).getByTestId("mods-view-groups").click();
  await expect(modsPane(page, pane).getByTestId("mod-groups-table")).toBeVisible();
}

function folder(page: Page, pane: Pane, id: number | "ungrouped"): Locator {
  return modsPane(page, pane).getByTestId(`mods-folder-${id}`);
}

function entry(page: Page, pane: Pane, folderId: number | "ungrouped", modId: string): Locator {
  return modsPane(page, pane).getByTestId(`mod-entry-${folderId}-${modId}`);
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

/** The mock's recorders for the commands a drop on a folder must never send. */
async function positionCommands(page: Page): Promise<{ move: unknown[]; activate: unknown[] }> {
  return page.evaluate(() => ({
    move: window.__MOVE_MODS_CALLS__ ?? [],
    activate: window.__ACTIVATE_CALLS__ ?? [],
  }));
}

test.describe("mods groups view", () => {
  test.use({ viewport: WIDE });

  test("switches a pane to Groups and back, and asks for exactly the queries of its view", async ({
    page,
  }) => {
    await openMods(page);
    await page.evaluate(seedLibrary);
    const active = modsPane(page, "active");
    await expect(active.getByTestId("mod-table")).toBeVisible();
    await expect(active.getByTestId("mod-groups-table")).toHaveCount(0);
    await expect(active.getByTestId("mods-view-list")).toHaveAttribute("aria-checked", "true");

    await showGroups(page, "active");
    await expect(active.getByTestId("mod-table")).toHaveCount(0);
    await expect(active.getByTestId("mods-view-groups")).toHaveAttribute("aria-checked", "true");
    // The other pane did not follow, and asked nothing of the grouped command.
    await expect(modsPane(page, "inactive").getByTestId("mod-table")).toBeVisible();
    const calls = await page.evaluate(() => ({
      working: window.__LIST_WORKING_MODS_GROUPED_CALLS__ ?? [],
      inactive: window.__LIST_INACTIVE_MODS_GROUPED_CALLS__ ?? [],
    }));
    expect(calls.inactive).toEqual([]);
    expect(calls.working.length).toBeGreaterThan(0);
    expect(calls.working.every((call) => call.collapsed.length === 0)).toBe(true);
    expect(calls.working[0]?.list).toMatchObject({ search: null, tag: null, source: null });

    await active.getByTestId("mods-view-list").click();
    await expect(active.getByTestId("mod-table")).toBeVisible();
    await expect(active.getByTestId("mod-groups-table")).toHaveCount(0);
    // The List view keeps its own row ids.
    await expect(active.getByTestId("mod-row-mod.003")).toBeVisible();
  });

  test("nests folders as a treegrid, subfolders before mods, ungrouped last, load order shown", async ({
    page,
  }) => {
    await openGroupsView(page);
    const active = modsPane(page, "active");

    await expect(active.getByTestId("mod-groups-table")).toHaveAttribute("role", "treegrid");
    await expect
      .poll(async () => (await entryKeys(page, "active")).slice(0, ACTIVE_OPEN_HEAD.length))
      .toEqual(ACTIVE_OPEN_HEAD);

    // Levels: a header is its depth, a mod one below its folder.
    const levels = await active
      .locator("[data-entry-key]")
      .evaluateAll((rows) =>
        rows
          .slice(0, 12)
          .map(
            (row) =>
              `${(row as HTMLElement).dataset["entryKey"]}=${row.getAttribute("aria-level")}`,
          ),
      );
    expect(levels).toEqual([
      "f:4=1",
      "m:4:mod.003=2",
      "f:5=1",
      "f:1=1",
      "f:2=2",
      "f:3=3",
      "m:3:mod.006=4",
      "m:2:mod.004=3",
      "m:2:mod.005=3",
      "m:1:mod.003=2",
      "m:1:mod.004=2",
      "f:u=1",
    ]);

    // aria-expanded is on headers only, aria-selected on mod rows only.
    await expect(active.locator('[role="row"][aria-expanded]')).toHaveCount(
      await active.locator('[data-testid^="mods-folder-"]').count(),
    );
    await expect(active.locator('[data-entry-key^="m:"][aria-expanded]')).toHaveCount(0);
    await expect(active.locator('[data-entry-key^="f:"][aria-selected]')).toHaveCount(0);
    await expect(active.locator('[data-entry-key^="m:"]:not([aria-selected])')).toHaveCount(0);

    // The header says what it holds; the count is distinct mods of its whole subtree.
    await expect(folder(page, "active", 1)).toHaveAttribute("aria-label", "Weapons, 4 mods, open");
    await expect(folder(page, "active", 5)).toHaveAttribute("aria-label", "Spare, 0 mods, open");
    // D4b: a load-order number on every Active row.
    await expect(
      entry(page, "active", 4, "mod.003").getByTestId("mod-row-position-mod.003"),
    ).toBeVisible();
    // "Not in a group" holds the active mods no group has.
    await expect(folder(page, "active", "ungrouped")).toContainText("Not in a group");
  });

  test("a mod in two groups shows under each with an also-in mark, and is one mod to select", async ({
    page,
  }) => {
    await openGroupsView(page);

    const inWeapons = entry(page, "active", 1, "mod.004");
    const inMelee = entry(page, "active", 2, "mod.004");
    await expect(inWeapons.getByTestId("mod-also-in")).toContainText("also in 1");
    await expect(inWeapons.getByTestId("mod-also-in")).toHaveAttribute("title", "Also in: Melee");
    await expect(inMelee.getByTestId("mod-also-in")).toHaveAttribute("title", "Also in: Weapons");
    // A mod in a single group carries no mark.
    await expect(entry(page, "active", 3, "mod.006").getByTestId("mod-also-in")).toHaveCount(0);

    await inWeapons.getByTestId("mod-row-select-mod.004").locator("input").check();
    await expect(inMelee).toHaveAttribute("aria-selected", "true");
    await expect(modsPane(page, "active").getByTestId("mods-selected-count")).toContainText(
      "1 selected",
    );
  });

  test("folds persist across a reload for the same profile and are not borrowed from another", async ({
    page,
  }) => {
    await openGroupsView(page);
    // Another profile's folds sit in storage and must not apply here.
    await page.evaluate(() =>
      localStorage.setItem(
        "rimmerge.groupsView.collapsed.v1:another-profile",
        JSON.stringify({ active: [4], inactive: [] }),
      ),
    );

    await folder(page, "active", 1).click();
    await expect(folder(page, "active", 1)).toHaveAttribute("aria-expanded", "false");
    await expect(folder(page, "active", 2)).toHaveCount(0);
    // The fold reached the query as a key, and nothing else was folded.
    await expect
      .poll(async () => {
        const calls = await page.evaluate(() => window.__LIST_WORKING_MODS_GROUPED_CALLS__ ?? []);
        return calls.at(-1)?.collapsed;
      })
      .toEqual([{ kind: "group", groupId: 1 }]);
    // The folded folder keeps its count.
    await expect(folder(page, "active", 1)).toContainText("4 mods");

    // The reload keeps `localStorage` but loses the mock's in-memory library, so it is seeded again
    // once the project is loaded (loading a project starts from an empty library).
    await page.reload();
    await page.getByTestId("load-project-button").click();
    await page.waitForURL(/\/$/);
    await page.evaluate(seedLibrary);
    await page.getByTestId("nav-mods").click();
    await showGroups(page, "active");

    await expect(folder(page, "active", 1)).toHaveAttribute("aria-expanded", "false");
    await expect(folder(page, "active", 2)).toHaveCount(0);
    // Armor is folded only in the other profile's entry.
    await expect(folder(page, "active", 4)).toHaveAttribute("aria-expanded", "true");
    await expect(entry(page, "active", 4, "mod.003")).toBeVisible();
    // The Inactive pane has folds of its own: nothing is folded there.
    await showGroups(page, "inactive");
    await expect(folder(page, "inactive", 1)).toHaveAttribute("aria-expanded", "true");
  });

  test("Collapse all folds every folder, Expand all opens them again", async ({ page }) => {
    await openGroupsView(page);
    const active = modsPane(page, "active");

    await active.getByTestId("mods-groups-collapse-all").click();
    await expect.poll(() => entryKeys(page, "active")).toEqual(["f:4", "f:5", "f:1", "f:u"]);
    await expect(folder(page, "active", 4)).toHaveAttribute("aria-expanded", "false");
    await expect
      .poll(async () => {
        const calls = await page.evaluate(() => window.__LIST_WORKING_MODS_GROUPED_CALLS__ ?? []);
        return calls.at(-1)?.collapsed;
      })
      .toEqual([
        { kind: "group", groupId: 1 },
        { kind: "group", groupId: 2 },
        { kind: "group", groupId: 3 },
        { kind: "group", groupId: 4 },
        { kind: "group", groupId: 5 },
        { kind: "ungrouped" },
      ]);

    await active.getByTestId("mods-groups-expand-all").click();
    await expect
      .poll(async () => (await entryKeys(page, "active")).slice(0, ACTIVE_OPEN_HEAD.length))
      .toEqual(ACTIVE_OPEN_HEAD);
  });

  test("Left and Right close and open a folder, and walk to the parent or first child", async ({
    page,
  }) => {
    await openGroupsView(page);

    await folder(page, "active", 1).focus();
    await page.keyboard.press("ArrowLeft");
    await expect(folder(page, "active", 1)).toHaveAttribute("aria-expanded", "false");
    await expect(folder(page, "active", 2)).toHaveCount(0);
    await page.keyboard.press("ArrowRight");
    await expect(folder(page, "active", 1)).toHaveAttribute("aria-expanded", "true");
    // Right on an open folder enters it: the first child, here the subfolder.
    await page.keyboard.press("ArrowRight");
    await expect(folder(page, "active", 2)).toBeFocused();
    // Left on an open folder closes it; on a closed one it goes to the parent.
    await page.keyboard.press("ArrowLeft");
    await expect(folder(page, "active", 2)).toHaveAttribute("aria-expanded", "false");
    await page.keyboard.press("ArrowLeft");
    await expect(folder(page, "active", 1)).toBeFocused();
    // A mod row: Left goes to the folder it sits in.
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await expect(entry(page, "active", 1, "mod.003")).toBeFocused();
    await page.keyboard.press("ArrowLeft");
    await expect(folder(page, "active", 1)).toBeFocused();
  });

  test("Enter and Space on a header toggle it, while on a mod row they open and select", async ({
    page,
  }) => {
    await openGroupsView(page);

    await folder(page, "active", 4).focus();
    await page.keyboard.press("Enter");
    await expect(folder(page, "active", 4)).toHaveAttribute("aria-expanded", "false");
    await page.keyboard.press("Space");
    await expect(folder(page, "active", 4)).toHaveAttribute("aria-expanded", "true");

    await entry(page, "active", 4, "mod.003").focus();
    await page.keyboard.press("Space");
    await expect(entry(page, "active", 4, "mod.003")).toHaveAttribute("aria-selected", "true");
    await expect(entry(page, "active", 1, "mod.003")).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/\/mods\/mod\.003$/);
  });

  test("Select all takes the mods inside folded folders too, and counts what List view counts", async ({
    page,
  }) => {
    await openGroupsView(page);
    const active = modsPane(page, "active");
    const total = Number.parseInt(
      (await active.getByTestId("mods-total").innerText()).replace(/\D+.*$/s, ""),
      10,
    );

    await folder(page, "active", 1).click();
    await expect(folder(page, "active", 2)).toHaveCount(0);
    await entry(page, "active", 4, "mod.003").focus();
    await page.keyboard.press("Control+a");
    await expect(active.getByTestId("mods-selected-count")).toContainText(`${total} selected`);

    // mod.006 sat only inside the folded Weapons tree, and is selected all the same.
    await active.getByTestId("mods-view-list").click();
    await expect(active.getByTestId("mod-row-select-mod.006").locator("input")).toBeChecked();
    await expect(active.getByTestId("mods-selected-count")).toContainText(`${total} selected`);
  });

  test("a Shift range runs over the rows on screen, headers add nothing, and a mod counts once", async ({
    page,
  }) => {
    await openGroupsView(page);
    const active = modsPane(page, "active");

    await entry(page, "active", 4, "mod.003").getByTestId("mod-row-select-mod.003").click();
    await entry(page, "active", 1, "mod.004")
      .getByTestId("mod-row-select-mod.004")
      .click({ modifiers: ["Shift"] });

    // The range holds 003 and 004 twice each (Armor and Weapons copies) plus 005 and 006: four
    // distinct mods, not eight rows' worth.
    await expect(active.getByTestId("mods-selected-count")).toContainText("4 selected");
    await expect(entry(page, "active", 2, "mod.005")).toHaveAttribute("aria-selected", "true");
    await expect(entry(page, "active", 3, "mod.006")).toHaveAttribute("aria-selected", "true");
    await expect(active.locator('[data-entry-key^="f:"][aria-selected]')).toHaveCount(0);
  });

  test("dropping a mod on a folder header adds it to that group and moves nothing", async ({
    page,
  }) => {
    await openGroupsView(page);
    // Weapons folded keeps "Not in a group" near the top, where its first mod is on screen.
    await folder(page, "active", 1).click();
    const before = await organizationRequests(page, "add_group_members");

    await entry(page, "active", "ungrouped", "mod.000").dragTo(folder(page, "active", 5));

    await expect
      .poll(async () =>
        (await organizationRequests(page, "add_group_members")).slice(before.length),
      )
      .toEqual([{ id: 5, modIds: ["mod.000"] }]);
    // It now sits in Spare, and has left "Not in a group".
    await expect(entry(page, "active", 5, "mod.000")).toBeVisible();
    await expect(entry(page, "active", "ungrouped", "mod.000")).toHaveCount(0);
    expect(await positionCommands(page)).toEqual({ move: [], activate: [] });
  });

  test("an inactive mod dropped on an Active folder joins the group and is not activated", async ({
    page,
  }) => {
    await openGroupsView(page);
    const inactive = modsPane(page, "inactive");
    const before = await organizationRequests(page, "add_group_members");

    await inactive.getByTestId("mod-row-mod.903").dragTo(folder(page, "active", 5));

    await expect
      .poll(async () =>
        (await organizationRequests(page, "add_group_members")).slice(before.length),
      )
      .toEqual([{ id: 5, modIds: ["mod.903"] }]);
    expect(await positionCommands(page)).toEqual({ move: [], activate: [] });
    // Still inactive.
    await expect(inactive.getByTestId("mod-row-mod.903")).toBeVisible();
  });

  test("dragging a multi-selection onto a folder header adds each mod once, whatever copy was picked up", async ({
    page,
  }) => {
    await openGroupsView(page);
    const before = await organizationRequests(page, "add_group_members");

    // 003 is in Armor and Weapons, 004 in Weapons and Melee: selected as mods, not as rows.
    await entry(page, "active", 4, "mod.003").getByTestId("mod-row-select-mod.003").click();
    await entry(page, "active", 2, "mod.004")
      .getByTestId("mod-row-select-mod.004")
      .click({
        modifiers: ["Control"],
      });
    await expect(modsPane(page, "active").getByTestId("mods-selected-count")).toContainText(
      "2 selected",
    );

    // Picked up by its Weapons copy, the second copy of a selected mod.
    await entry(page, "active", 1, "mod.003").dragTo(folder(page, "active", 5));

    await expect
      .poll(async () =>
        (await organizationRequests(page, "add_group_members")).slice(before.length),
      )
      .toEqual([{ id: 5, modIds: ["mod.003", "mod.004"] }]);
    expect(await positionCommands(page)).toEqual({ move: [], activate: [] });
  });

  test("a group deleted from the rail drops out of Groups view without an error pane", async ({
    page,
  }) => {
    await openGroupsView(page);
    await folder(page, "active", 5).click();
    await expect(folder(page, "active", 5)).toHaveAttribute("aria-expanded", "false");
    await expect
      .poll(async () => {
        const calls = await page.evaluate(() => window.__LIST_WORKING_MODS_GROUPED_CALLS__ ?? []);
        return calls.at(-1)?.collapsed;
      })
      .toEqual([{ kind: "group", groupId: 5 }]);

    await groupMenu(page, "Spare", "delete");
    await page.getByTestId("group-delete-submit").click();
    await expect(page.getByTestId("group-delete-dialog")).toBeHidden();

    await expect(folder(page, "active", 5)).toHaveCount(0);
    await expect(modsPane(page, "active").getByTestId("mods-error")).toHaveCount(0);
    await expect(modsPane(page, "active").getByTestId("mod-groups-table")).toBeVisible();
    // The fold left the request and the stored folds with the group.
    await expect
      .poll(async () => {
        const calls = await page.evaluate(() => window.__LIST_WORKING_MODS_GROUPED_CALLS__ ?? []);
        return calls.at(-1)?.collapsed;
      })
      .toEqual([]);
    const stored = await page.evaluate(() =>
      Object.entries(localStorage)
        .filter(([key]) => key.startsWith("rimmerge.groupsView.collapsed.v1:"))
        .map(([, value]) => JSON.parse(value) as { active: number[]; inactive: number[] }),
    );
    expect(stored.flatMap((folds) => [...folds.active, ...folds.inactive])).not.toContain(5);
  });

  test("Not in a group refuses a drop, with the reason on screen and no command", async ({
    page,
  }) => {
    await openGroupsView(page);
    await folder(page, "active", 1).click();
    const before = await organizationRequests(page, "add_group_members");

    const source = await entry(page, "active", 4, "mod.003").boundingBox();
    const target = await folder(page, "active", "ungrouped").boundingBox();
    if (source === null || target === null) {
      throw new Error("a row has no box");
    }
    await page.mouse.move(source.x + source.width / 2, source.y + source.height / 2);
    await page.mouse.down();
    const x = target.x + target.width / 2;
    const y = target.y + target.height / 2;
    await page.mouse.move(x, y, { steps: 8 });
    await page.mouse.move(x + 2, y, { steps: 2 });
    await expect(
      modsPane(page, "active").getByTestId("mods-groups-ungrouped-refused"),
    ).toBeVisible();
    await page.mouse.up();
    await expect(modsPane(page, "active").getByTestId("mods-groups-ungrouped-refused")).toHaveCount(
      0,
    );

    expect((await organizationRequests(page, "add_group_members")).length).toBe(before.length);
    expect(await positionCommands(page)).toEqual({ move: [], activate: [] });
  });

  test("a folder menu selects every mod in it, folded or not, and opens the activate and deactivate plans", async ({
    page,
  }) => {
    await openGroupsView(page);
    const active = modsPane(page, "active");

    // Weapons, folded: its subtree is mods 003 004 005 006 in this pane.
    await folder(page, "active", 1).click();
    await expect(folder(page, "active", 2)).toHaveCount(0);
    await folder(page, "active", 1).getByTestId("folder-menu-button").click();
    await page.getByTestId("folder-menu-select").click();
    await expect(active.getByTestId("mods-selected-count")).toContainText("4 selected");
    await expect(page.getByTestId("mods-move-status")).toContainText("Selected 4 mods in Weapons");
    // The folder's menu asked the List query with that folder as the group filter.
    await active.getByTestId("mods-view-list").click();
    await expect(active.getByTestId("mod-row-select-mod.006").locator("input")).toBeChecked();
    await active.getByTestId("mods-view-groups").click();

    await folder(page, "active", 1).getByTestId("folder-menu-button").click();
    await page.getByTestId("folder-menu-activate").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeVisible();
    await expect(page.getByTestId("activate-plan-to-add")).toContainText("Inactive Mod 1");
    await page.getByTestId("activate-confirm-cancel").click();
    await expect(page.locator(".p-overlay-mask")).toHaveCount(0);

    await folder(page, "active", 1).getByTestId("folder-menu-button").click();
    await page.getByTestId("folder-menu-deactivate").click();
    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeVisible();
    await page.getByTestId("deactivate-confirm-cancel").click();
    await expect(page.locator(".p-overlay-mask")).toHaveCount(0);
    expect((await positionCommands(page)).activate).toEqual([]);

    // "Not in a group" offers only the selection.
    await folder(page, "active", "ungrouped").getByTestId("folder-menu-button").click();
    await expect(page.getByTestId("folder-menu-select")).toBeVisible();
    await expect(page.getByTestId("folder-menu-activate")).toHaveCount(0);
    await page.keyboard.press("Escape");
  });

  test("moving is off in Groups view: the note says so, a drag from Active is refused, the menu is disabled", async ({
    page,
  }) => {
    await openGroupsView(page);
    const active = modsPane(page, "active");

    await expect(page.getByTestId("mods-reorder-blocked")).toContainText(
      "Moving is off in Groups view",
    );
    await entry(page, "active", 4, "mod.003").getByTestId("mod-row-menu-mod.003").click();
    await expect(page.getByTestId("mod-move-up").locator("xpath=ancestor::li[1]")).toHaveAttribute(
      "data-p-disabled",
      "true",
    );
    await expect(page.getByTestId("mod-move-menu-reason")).toContainText(
      "Moving is off in Groups view",
    );
    await page.keyboard.press("Escape");

    // Alt+Arrow moves nothing.
    await entry(page, "active", 4, "mod.003").focus();
    await page.keyboard.press("Alt+ArrowDown");
    expect(await positionCommands(page)).toEqual({ move: [], activate: [] });

    // Back in List view the note reads the sort again, and moving is on.
    await active.getByTestId("mods-view-list").click();
    await expect(page.getByTestId("mods-reorder-blocked")).toHaveCount(0);
  });

  test("says how many lines are not shown when the backend cut the view", async ({ page }) => {
    await openMods(page);
    await page.evaluate(seedLibrary);
    // A cap of 10 cuts the answer in the mock's own slicing: the open library has more lines.
    await page.evaluate(() => {
      window.__GROUPED_ENTRY_CAP__ = 10;
    });
    await showGroups(page, "active");

    await expect(modsPane(page, "active").getByTestId("mods-groups-cut-note")).toContainText(
      /\d+ lines are not shown/,
    );
    await expect(modsPane(page, "active").locator("[data-entry-key]")).toHaveCount(10);
    // The List view has no such note.
    await modsPane(page, "active").getByTestId("mods-view-list").click();
    await expect(modsPane(page, "active").getByTestId("mods-groups-cut-note")).toHaveCount(0);
  });

  test("empty folders show with no filter and go while searching; a search with no match says so", async ({
    page,
  }) => {
    await openGroupsView(page);
    const active = modsPane(page, "active");
    await expect(folder(page, "active", 5)).toBeVisible();

    await active.getByTestId("mods-search").fill("mod.003");
    await expect(folder(page, "active", 5)).toHaveCount(0);
    await expect(folder(page, "active", 4)).toBeVisible();
    await expect(entry(page, "active", 1, "mod.003")).toBeVisible();

    await active.getByTestId("mods-search").fill("no-such-mod-anywhere");
    await expect(active.getByTestId("mods-groups-empty")).toContainText(
      "No mods match the current search, tag, source or group.",
    );
    await expect(active.locator("[data-entry-key]")).toHaveCount(0);

    await active.getByTestId("mods-search").fill("");
    await expect(active.getByTestId("mods-groups-empty")).toHaveCount(0);
    await expect(folder(page, "active", 5)).toBeVisible();
  });

  test("Not in a group is left out under a group scope, which holds only that group's mods", async ({
    page,
  }) => {
    await openGroupsView(page, "inactive");
    await expect(folder(page, "inactive", "ungrouped")).toBeVisible();
    await node(page, "Weapons").click();
    await expect(folder(page, "inactive", 1)).toBeVisible();
    await expect(folder(page, "inactive", "ungrouped")).toHaveCount(0);
  });

  test("the switch is disabled with its reason while the groups file is unusable", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      window.__ORGANIZATION_STATE__ = "recovered";
    });
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    const active = modsPane(page, "active");
    await expect(active.getByTestId("mod-table")).toBeVisible();
    await expect(active.getByTestId("mods-view-groups")).toHaveAttribute("aria-disabled", "true");
    await expect(active.getByTestId("mods-view-unavailable")).toBeVisible();
    // The choice is refused, not just styled: the pane stays in List view.
    await active.getByTestId("mods-view-groups").click({ force: true });
    await expect(active.getByTestId("mod-table")).toBeVisible();
    await expect(active.getByTestId("mod-groups-table")).toHaveCount(0);
  });
});
