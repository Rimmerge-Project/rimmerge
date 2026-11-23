import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario, modsPane } from "./support";

/**
 * #47: after a committed activation or deactivation, the list's selection loses exactly the mods
 * the backend says it moved. A dependency the activation pulled in, which a filter hides, must not
 * stay selected behind the filter, and a refused Core must stay selected.
 */

async function openMods(page: Page): Promise<void> {
  await loadScenario(page);
  await page.getByTestId("nav-mods").click();
  await expect(modsPane(page, "active").getByTestId("mod-table")).toBeVisible();
}

async function check(page: Page, kind: "active" | "inactive", modId: string): Promise<void> {
  await modsPane(page, kind)
    .getByTestId(`mod-row-select-${modId}`)
    .locator('input[type="checkbox"]')
    .check();
}

test.describe("mods selection after a commit", () => {
  test("a dependency pulled in under a filter that hides it leaves the Inactive selection", async ({
    page,
  }) => {
    await openMods(page);
    const inactive = modsPane(page, "inactive");
    // mod.906 is mod.905's declared dependency; mod.901 is unrelated and stays selected.
    await check(page, "inactive", "mod.901");
    await check(page, "inactive", "mod.906");
    await inactive.getByTestId("mods-search").fill("Inactive Mod 5");
    await expect(inactive.getByTestId("mods-selected-count")).toHaveText(
      "2 selected (2 not shown)",
    );
    await page.getByTestId("activate-with-dependencies-checkbox").locator("input").check();

    await inactive.getByTestId("mod-row-mod.905").dblclick();
    await expect(page.getByTestId("activate-plan-to-add")).toHaveCount(2);
    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();

    // The backend moved mod.906 and mod.905; only the unrelated mod.901 is still selected, and
    // the count of hidden ones is right (#47-AC3).
    await expect(inactive.getByTestId("mods-selected-count")).toHaveText(
      "1 selected (1 not shown)",
    );
    expect(await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? [])).toEqual([
      { ids: ["mod.905"], withDependencies: true, anchor: null },
    ]);
  });

  test("a deactivation drops what was removed from the Active selection and keeps Core", async ({
    page,
  }) => {
    await openMods(page);
    const active = modsPane(page, "active");
    // Core sits far down the working list, so a filter brings its row into view.
    await active.getByTestId("mods-search").fill("ludeon.rimworld");
    await check(page, "active", "ludeon.rimworld");
    await active.getByTestId("mods-search").fill("mod.003");
    await check(page, "active", "mod.003");
    await expect(active.getByTestId("mods-selected-count")).toHaveText("2 selected (1 not shown)");

    await page.getByTestId("deactivate-selected-button").click();
    await expect(page.getByTestId("deactivate-plan-refused")).toHaveCount(1);
    await page.getByTestId("deactivate-confirm-submit").click();
    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeHidden();

    // mod.003 was removed; Core was refused, is still active and still selected (#47-AC2/AC3).
    await expect(active.getByTestId("mods-selected-count")).toHaveText("1 selected (1 not shown)");
    await active.getByTestId("mods-search").fill("ludeon.rimworld");
    await expect(
      active.getByTestId("mod-row-select-ludeon.rimworld").locator("input"),
    ).toBeChecked();
    expect(await page.evaluate(() => window.__DEACTIVATE_CALLS__ ?? [])).toEqual([
      { ids: ["ludeon.rimworld", "mod.003"] },
    ]);
  });

  test("the mock's commit reports what was already active or not active, as the backend does", async ({
    page,
  }) => {
    await openMods(page);

    const result = await page.evaluate(async () => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const activated = await invoke("activate_mods", {
        request: { ids: ["mod.901", "mod.003", "mod.901"], withDependencies: false, anchor: null },
      });
      const deactivated = await invoke("deactivate_mods", {
        request: { ids: ["mod.902", "mod.901", "ludeon.rimworld"] },
      });
      return { activated, deactivated };
    });

    expect(result.activated).toMatchObject({
      activated: ["mod.901"],
      alreadyActive: ["mod.003", "mod.901"],
    });
    // mod.902 was never active; mod.901 was just activated; Core is refused, in neither list.
    expect(result.deactivated).toMatchObject({ removed: ["mod.901"], notActive: ["mod.902"] });
  });

  test("the mock walks dependencies transitively and reports a missing one as unresolvable", async ({
    page,
  }) => {
    await openMods(page);

    const result = await page.evaluate(async () => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const request = (ids: string[]) => ({ ids, withDependencies: true, anchor: null });
      const chainPlan = await invoke("plan_activate_mods", { request: request(["mod.907"]) });
      const missingPlan = await invoke("plan_activate_mods", { request: request(["mod.910"]) });
      const chain = await invoke("activate_mods", { request: request(["mod.907"]) });
      const missing = await invoke("activate_mods", { request: request(["mod.910"]) });
      return { chainPlan, missingPlan, chain, missing };
    });

    // mod.907 needs mod.908, which needs mod.909: dependencies come first, deepest first.
    expect(result.chainPlan).toMatchObject({ toAdd: ["mod.909", "mod.908", "mod.907"] });
    expect(result.chain).toMatchObject({
      activated: ["mod.909", "mod.908", "mod.907"],
      alreadyActive: [],
    });
    // mod.910 needs a mod installed nowhere: planned and activated alone, the gap is reported.
    expect(result.missingPlan).toMatchObject({
      toAdd: ["mod.910"],
      unresolvableDependencies: { "mod.910": ["missing.dependency"] },
    });
    expect(result.missing).toMatchObject({ activated: ["mod.910"] });
  });
});
