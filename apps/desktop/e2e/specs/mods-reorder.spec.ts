import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario, modsPane } from "./support";

/**
 * The Mods page's side-by-side lists: drag and drop between and within them, the Move menu and
 * Alt+Arrow, and what each of those sends over IPC. The mock backend mirrors `rim_session`'s
 * move/activate semantics (pinned by `mods-reorder-mock.spec.ts`), so the lists below are what
 * the real backend would show. `mod.000` is Core in the mock; `mod.901`.. are installed, inactive.
 */

/** The row ids rendered in one list, top to bottom (only the virtualized viewport is in the DOM). */
async function rowIds(page: Page, kind: "active" | "inactive"): Promise<string[]> {
  return modsPane(page, kind)
    .getByTestId("mod-table")
    .locator('[role="row"]')
    .evaluateAll((rows) =>
      rows.map((row) => (row.getAttribute("data-testid") ?? "").replace(/^mod-row-/, "")),
    );
}

async function moveCalls(page: Page) {
  return page.evaluate(() => window.__MOVE_MODS_CALLS__ ?? []);
}

async function openMods(page: Page): Promise<void> {
  await loadScenario(page);
  await page.getByTestId("nav-mods").click();
  await expect(page.getByTestId("mod-row-mod.003")).toBeVisible();
  await expect(page.getByTestId("mod-row-mod.901")).toBeVisible();
}

/** Drops onto the top quarter of `target`, i.e. into the gap just above that row. */
const ABOVE_ROW = { x: 80, y: 4 };

test.describe("side-by-side lists", () => {
  test("Inactive sits left of Active at wide widths and stacks above it below that", async ({
    page,
  }) => {
    await openMods(page);
    const inactive = await modsPane(page, "inactive").boundingBox();
    const active = await modsPane(page, "active").boundingBox();
    expect(inactive).not.toBeNull();
    expect(active).not.toBeNull();
    expect(inactive?.x ?? 0).toBeLessThan(active?.x ?? 0);
    expect(Math.abs((inactive?.y ?? 0) - (active?.y ?? 99))).toBeLessThan(2);

    await page.setViewportSize({ width: 900, height: 900 });

    const stackedInactive = await modsPane(page, "inactive").boundingBox();
    const stackedActive = await modsPane(page, "active").boundingBox();
    expect(stackedInactive?.y ?? 0).toBeLessThan(stackedActive?.y ?? 0);
    expect(Math.abs((stackedInactive?.x ?? 0) - (stackedActive?.x ?? 99))).toBeLessThan(2);
  });
});

test.describe("drag and drop", () => {
  test("reorders within Active: the drop names the row below the gap, and Apply is offered", async ({
    page,
  }) => {
    await openMods(page);

    await page
      .getByTestId("mod-row-mod.003")
      .dragTo(page.getByTestId("mod-row-mod.001"), { targetPosition: ABOVE_ROW });

    expect(await moveCalls(page)).toEqual([
      { ids: ["mod.003"], anchor: { kind: "before", modId: "mod.001" } },
    ]);
    await expect(async () => {
      expect((await rowIds(page, "active")).slice(0, 4)).toEqual([
        "mod.000",
        "mod.003",
        "mod.001",
        "mod.002",
      ]);
    }).toPass();
    await expect(page.getByTestId("order-not-in-file")).toBeVisible();
    await expect(page.getByTestId("mods-move-status")).toHaveText(
      /^Moved .+ to position 2 of \d+$/,
    );
  });

  test("moves the selected rows together", async ({ page }) => {
    await openMods(page);
    await page.getByTestId("mod-row-select-mod.004").locator('input[type="checkbox"]').check();
    await page.getByTestId("mod-row-select-mod.005").locator('input[type="checkbox"]').check();

    await page
      .getByTestId("mod-row-mod.005")
      .dragTo(page.getByTestId("mod-row-mod.001"), { targetPosition: ABOVE_ROW });

    expect(await moveCalls(page)).toEqual([
      { ids: ["mod.004", "mod.005"], anchor: { kind: "before", modId: "mod.001" } },
    ]);
    await expect(async () => {
      expect((await rowIds(page, "active")).slice(0, 4)).toEqual([
        "mod.000",
        "mod.004",
        "mod.005",
        "mod.001",
      ]);
    }).toPass();
  });

  test("activates a mod dragged from Inactive at the drop position, without a dialog", async ({
    page,
  }) => {
    await openMods(page);

    await page
      .getByTestId("mod-row-mod.901")
      .dragTo(page.getByTestId("mod-row-mod.002"), { targetPosition: ABOVE_ROW });

    await expect(modsPane(page, "active").getByTestId("mod-row-mod.901")).toBeVisible();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([
      { ids: ["mod.901"], withDependencies: false, anchor: { kind: "before", modId: "mod.002" } },
    ]);
    expect((await rowIds(page, "active")).slice(0, 4)).toEqual([
      "mod.000",
      "mod.001",
      "mod.901",
      "mod.002",
    ]);
    await expect(modsPane(page, "inactive").getByTestId("mod-row-mod.901")).toHaveCount(0);
  });

  test("asks first when the drop would also pull in a dependency, and keeps the position", async ({
    page,
  }) => {
    await openMods(page);
    await modsPane(page, "inactive")
      .getByTestId("activate-with-dependencies-checkbox")
      .locator("input")
      .check();

    await page
      .getByTestId("mod-row-mod.905")
      .dragTo(page.getByTestId("mod-row-mod.002"), { targetPosition: ABOVE_ROW });

    await expect(page.getByTestId("activate-confirm-dialog")).toBeVisible();
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([]);

    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([
      { ids: ["mod.905"], withDependencies: true, anchor: { kind: "before", modId: "mod.002" } },
    ]);
    const active = await rowIds(page, "active");
    expect(active.slice(0, 5)).toEqual(expect.arrayContaining(["mod.905", "mod.906"]));
    expect(active.indexOf("mod.906")).toBeLessThan(active.indexOf("mod.905"));
  });

  test("deactivates a row dropped on the Inactive list", async ({ page }) => {
    await openMods(page);

    await page
      .getByTestId("mod-row-mod.005")
      .dragTo(modsPane(page, "inactive").getByTestId("mod-table"));

    await expect(modsPane(page, "active").getByTestId("mod-row-mod.005")).toHaveCount(0);
    expect(await page.evaluate(() => window.__DEACTIVATE_CALLS__ ?? [])).toEqual([
      { ids: ["mod.005"] },
    ]);
  });

  test("refuses to drop Core on the Inactive list, with the reason shown", async ({ page }) => {
    await openMods(page);
    const core = page.getByTestId("mod-row-mod.000");
    const inactiveTable = modsPane(page, "inactive").getByTestId("mod-table");

    await core.hover();
    await page.mouse.down();
    const target = await inactiveTable.boundingBox();
    await page.mouse.move((target?.x ?? 0) + 100, (target?.y ?? 0) + 100, { steps: 6 });
    await expect(modsPane(page, "inactive").getByTestId("mod-table-refused-text")).toHaveText(
      "Core can't be deactivated.",
    );
    await page.mouse.up();

    expect(await page.evaluate(() => window.__DEACTIVATE_CALLS__ ?? [])).toEqual([]);
    await expect(modsPane(page, "active").getByTestId("mod-row-mod.000")).toBeVisible();
  });

  test("says the first reorder switched to your own order, and View suggested goes back", async ({
    page,
  }) => {
    await openMods(page);
    await page.getByTestId("order-source-suggested").click();
    await expect(page.getByTestId("order-source-suggested")).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await expect(page.getByTestId("mods-suggested-note")).toBeVisible();

    await page
      .getByTestId("mod-row-mod.003")
      .dragTo(page.getByTestId("mod-row-mod.001"), { targetPosition: ABOVE_ROW });

    await expect(page.getByTestId("order-switched-note")).toContainText("Now using your own order");
    await expect(page.getByTestId("order-source-current")).toHaveAttribute("aria-checked", "true");

    await page.getByTestId("order-switched-view-suggested").click();
    await expect(page.getByTestId("order-switched-note")).toBeHidden();
    await expect(page.getByTestId("order-source-suggested")).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });
});

test.describe("moves without dragging", () => {
  test("the Move menu: top, then up, then after another mod, each sending its anchor", async ({
    page,
  }) => {
    await openMods(page);

    // The popup finishes closing before the next one opens: a click on the button while it
    // still fades out would toggle it shut instead.
    const menu = page.getByTestId("mod-move-menu");
    await page.getByTestId("mod-row-menu-mod.003").click();
    await page.getByTestId("mod-move-top").click();
    await expect(menu).toBeHidden();
    await page.getByTestId("mod-row-menu-mod.003").click();
    await page.getByTestId("mod-move-down").click();
    await expect(menu).toBeHidden();
    await page.getByTestId("mod-row-menu-mod.003").click();
    await page.getByTestId("mod-move-after").click();
    await page.getByTestId("mod-position-search").fill("mod.010");
    await page.getByTestId("mod-position-after-mod.010").click();

    expect(await moveCalls(page)).toEqual([
      { ids: ["mod.003"], anchor: { kind: "start" } },
      { ids: ["mod.003"], anchor: { kind: "after", modId: "mod.000" } },
      { ids: ["mod.003"], anchor: { kind: "after", modId: "mod.010" } },
    ]);
    await expect(async () => {
      const ids = await rowIds(page, "active");
      expect(ids.indexOf("mod.003")).toBe(ids.indexOf("mod.010") + 1);
    }).toPass();
  });

  test("Activate at... on an inactive row places it at the start", async ({ page }) => {
    await openMods(page);

    await page.getByTestId("mod-row-menu-mod.901").click();
    await page.getByTestId("mod-position-start").click();

    await expect(modsPane(page, "active").getByTestId("mod-row-mod.901")).toBeVisible();
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([
      { ids: ["mod.901"], withDependencies: false, anchor: { kind: "start" } },
    ]);
    expect((await rowIds(page, "active"))[0]).toBe("mod.901");
  });

  test("Alt+ArrowUp and Alt+ArrowDown move the focused row and announce the position", async ({
    page,
  }) => {
    await openMods(page);

    await page.getByTestId("mod-row-mod.003").focus();
    await page.keyboard.press("Alt+ArrowUp");
    await expect(page.getByTestId("mods-move-status")).toHaveText(
      /^Moved .+ to position 3 of \d+$/,
    );
    await page.keyboard.press("Alt+ArrowDown");
    await expect(page.getByTestId("mods-move-status")).toHaveText(
      /^Moved .+ to position 4 of \d+$/,
    );

    expect(await moveCalls(page)).toEqual([
      { ids: ["mod.003"], anchor: { kind: "before", modId: "mod.002" } },
      { ids: ["mod.003"], anchor: { kind: "after", modId: "mod.002" } },
    ]);
    // Focus follows the row, so the next key press keeps moving it.
    await expect(page.getByTestId("mod-row-mod.003")).toBeFocused();
  });

  test("Alt+Arrow inside a search box leaves the list alone", async ({ page }) => {
    await openMods(page);

    await modsPane(page, "active").getByTestId("mods-search").focus();
    await page.keyboard.press("Alt+ArrowUp");

    expect(await moveCalls(page)).toEqual([]);
  });
});

test.describe("reordering is off while the Active list is sorted by something else", () => {
  test("drags, the menu and Alt+Arrow are refused with the reason visible", async ({ page }) => {
    await openMods(page);
    await modsPane(page, "active").getByTestId("mods-sort-name-ascending").click();
    await expect(page.getByTestId("mods-reorder-blocked")).toContainText(
      "isn't sorted by load order",
    );

    const [first, second] = await rowIds(page, "active");
    await page
      .getByTestId(`mod-row-${first}`)
      .dragTo(page.getByTestId(`mod-row-${second}`), { targetPosition: ABOVE_ROW });
    await page.getByTestId(`mod-row-${first}`).focus();
    await page.keyboard.press("Alt+ArrowUp");
    await expect(page.getByTestId("mods-move-status")).toContainText("isn't sorted by load order");
    await page.getByTestId(`mod-row-menu-${first}`).click();
    await expect(page.getByTestId("mod-move-top").locator("xpath=ancestor::li")).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    await expect(page.getByTestId("mod-move-menu-reason")).toBeVisible();

    expect(await moveCalls(page)).toEqual([]);
  });

  test("a mod dropped from Inactive is still activated, at its default place", async ({ page }) => {
    await openMods(page);
    await modsPane(page, "active").getByTestId("mods-sort-name-ascending").click();

    await page
      .getByTestId("mod-row-mod.901")
      .dragTo(modsPane(page, "active").getByTestId("mod-table"));

    await expect(modsPane(page, "active").getByTestId("mod-row-mod.901")).toBeVisible();
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([
      { ids: ["mod.901"], withDependencies: false, anchor: null },
    ]);
  });
});

test.describe("what a move leaves to apply", () => {
  test("Apply after a move writes the moved order", async ({ page }) => {
    await openMods(page);
    await page.getByTestId("mod-row-menu-mod.003").click();
    await page.getByTestId("mod-move-top").click();
    await expect(page.getByTestId("order-not-in-file")).toBeVisible();

    await page.getByTestId("order-not-in-file-apply").click();
    await expect(page.getByTestId("apply-dialog-order-source")).toHaveText("Current");
    await page.getByTestId("apply-dialog-submit").click();
    // The scenario's findings leave a hard problem in this order, so Apply asks first.
    await page.getByTestId("apply-confirm-apply-anyway").click();
    await expect(page.getByTestId("apply-dialog")).toBeHidden();

    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([
      { source: "current", writeModsConfig: true, writeMergeMod: false, force: false },
    ]);
    await expect(page.getByTestId("order-not-in-file")).toBeHidden();
    const fileIds = await page.evaluate(async () => {
      const exported = (await window.__TAURI_INTERNALS__.invoke("export_order_text", {})) as {
        text: string;
      };
      return [...exported.text.matchAll(/\[([^\]]+)\]$/gm)].map((match) => match[1] ?? "");
    });
    expect(fileIds.slice(0, 3)).toEqual((await rowIds(page, "active")).slice(0, 3));
    expect(fileIds[0]).toBe("mod.003");
  });

  test("Revert order to ModsConfig.xml undoes a move and hides the note", async ({ page }) => {
    await openMods(page);
    await page.getByTestId("mod-row-menu-mod.003").click();
    await page.getByTestId("mod-move-top").click();
    await expect(page.getByTestId("order-not-in-file-revert")).toBeVisible();

    await page.getByTestId("order-not-in-file-revert").click();

    await expect(page.getByTestId("order-not-in-file")).toBeHidden();
    expect(await page.evaluate(() => window.__REVERT_ORDER_CALLS__ ?? 0)).toBe(1);
    await expect(async () => {
      expect((await rowIds(page, "active")).slice(0, 4)).toEqual([
        "mod.000",
        "mod.001",
        "mod.002",
        "mod.003",
      ]);
    }).toPass();
  });

  test("Discard pending changes drops a pending activation without a rescan", async ({ page }) => {
    await openMods(page);
    await page.getByTestId("mod-row-menu-mod.901").click();
    await page.getByTestId("mod-position-end").click();
    await expect(page.getByTestId("pending-changes-banner")).toBeVisible();

    await page.getByTestId("pending-changes-discard-button").click();

    await expect(page.getByTestId("pending-changes-banner")).toBeHidden();
    await expect(modsPane(page, "active").getByTestId("mod-row-mod.901")).toHaveCount(0);
    await expect(modsPane(page, "inactive").getByTestId("mod-row-mod.901")).toBeVisible();
    expect(await page.evaluate(() => window.__DISCARD_PENDING_CALLS__ ?? 0)).toBe(1);
    expect(await page.evaluate(() => window.__RESCAN_CALLS__ ?? 0)).toBe(0);
  });
});

test.describe("the mod info panel", () => {
  test("floats over the lists as a drawer at side-by-side widths", async ({ page }) => {
    await openMods(page);

    await page.getByTestId("mod-row-mod.002").click();

    const panel = page.getByTestId("mod-info-panel");
    await expect(panel).toBeVisible();
    const panelBox = await panel.boundingBox();
    const activeBox = await modsPane(page, "active").boundingBox();
    const viewport = page.viewportSize();
    expect((panelBox?.x ?? 0) + (panelBox?.width ?? 0)).toBeCloseTo(viewport?.width ?? 0, 0);
    // It overlaps the Active list instead of narrowing it.
    expect(panelBox?.x ?? 0).toBeLessThan((activeBox?.x ?? 0) + (activeBox?.width ?? 0));
    // Focus moves into the drawer, and closing it hands focus back to the row that opened it.
    await expect(panel).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(panel).toBeHidden();
    await expect(page.getByTestId("mod-row-mod.002")).toBeFocused();
  });

  test("takes its own column on a very wide window", async ({ page }) => {
    await page.setViewportSize({ width: 1900, height: 900 });
    await openMods(page);

    await page.getByTestId("mod-row-mod.002").click();

    const panelBox = await page.getByTestId("mod-info-panel").boundingBox();
    const activeBox = await modsPane(page, "active").boundingBox();
    await expect(page.getByTestId("mod-info-panel")).toBeVisible();
    expect(panelBox?.x ?? 0).toBeGreaterThanOrEqual(
      (activeBox?.x ?? 0) + (activeBox?.width ?? 0) - 1,
    );
  });
});
