import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

test.describe("active mod set", () => {
  test("activate with dependencies, rescan, and apply — the pending-change lifecycle", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-tab-inactive").click();

    // `mod.905` (`e2e/fixtures/scenario.ts`'s own declared-dependency
    // fixture) declares `mod.906` — checking "also activate
    // dependencies" pulls both in from one selection.
    await page.getByTestId("mod-row-select-mod.905").locator('input[type="checkbox"]').check();
    await page.getByTestId("activate-with-dependencies-checkbox").locator("input").check();
    await page.getByTestId("activate-selected-button").click();

    await expect(page.getByTestId("activate-confirm-dialog")).toBeVisible();
    // Names, not raw ids: `mod.905`/`mod.906` resolve to their
    // `inactiveMods` fixture names through `useModLabel`, since the mock's
    // own `list_mod_names` covers the inactive pool too (see `mods.spec.ts`
    // for the direct check).
    await expect(page.getByTestId("activate-confirm-dialog")).toContainText("Inactive Mod 5");
    await expect(page.getByTestId("activate-confirm-dialog")).toContainText("Inactive Mod 6");
    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();

    const activateCalls = await page.evaluate(() => window.__ACTIVATE_CALLS__ ?? []);
    expect(activateCalls).toEqual([{ ids: ["mod.905"], withDependencies: true }]);

    await expect(page.getByTestId("pending-changes-banner")).toBeVisible();
    await expect(page.getByTestId("pending-changes-unscanned-text")).toContainText("2");

    await page.getByTestId("pending-changes-rescan-button").click();
    await expect(page.getByTestId("pending-changes-banner")).toBeHidden();

    const rescanCalls = await page.evaluate(() => window.__RESCAN_CALLS__ ?? 0);
    expect(rescanCalls).toBe(1);

    // The rescan swaps in a fresh backend session; the selection survives it.
    await expect(page.getByTestId("order-source-suggested")).toHaveAttribute(
      "aria-checked",
      "true",
    );

    await page.getByTestId("shell-apply-button").click();
    await expect(page.getByTestId("apply-dialog")).toBeVisible();
    await expect(page.getByTestId("apply-dialog-order-source")).toHaveText("Suggested");
    await expect(page.getByTestId("apply-dialog-unapplied-active-changes")).toContainText(
      "Inactive Mod 5",
    );
    await expect(page.getByTestId("apply-dialog-unapplied-active-changes")).toContainText(
      "Inactive Mod 6",
    );

    // The scenario carries unanswered hard problems (missing mods), so a
    // write asks first; skipping ModsConfig.xml never would.
    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();

    const applyCalls = await page.evaluate(() => window.__APPLY_CALLS__ ?? []);
    expect(applyCalls.length).toBeGreaterThan(0);
    expect(applyCalls.at(-1)).toMatchObject({ source: "suggested", writeModsConfig: true });
  });

  test("opening Apply while stale shows the forced-off, disabled checkbox; its own Rescan unblocks Apply", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-tab-inactive").click();

    await page.getByTestId("mod-row-select-mod.905").locator('input[type="checkbox"]').check();
    await page.getByTestId("activate-with-dependencies-checkbox").locator("input").check();
    await page.getByTestId("activate-selected-button").click();
    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("pending-changes-banner")).toBeVisible();

    // Open Apply directly, without using the banner's own Rescan first —
    // the dialog must say the working set is stale on its own.
    await page.getByTestId("shell-apply-button").click();
    await expect(page.getByTestId("apply-dialog")).toBeVisible();
    await expect(page.getByTestId("apply-dialog-stale-active-set")).toBeVisible();
    const checkbox = page.getByTestId("apply-dialog-write-modsconfig-checkbox").locator("input");
    await expect(checkbox).not.toBeChecked();
    await expect(checkbox).toBeDisabled();

    await page.getByTestId("apply-dialog-rescan-button").click();
    await expect(page.getByTestId("apply-dialog-stale-active-set")).toBeHidden();
    await expect(checkbox).toBeChecked();
    await expect(checkbox).toBeEnabled();

    // The scenario carries unanswered hard problems (missing mods), so a
    // write asks first; skipping ModsConfig.xml never would.
    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();

    const applyCalls = await page.evaluate(() => window.__APPLY_CALLS__ ?? []);
    expect(applyCalls.length).toBeGreaterThan(0);
    expect(applyCalls.at(-1)).toMatchObject({ writeModsConfig: true });
  });

  test("deactivating a mod with dependents names them in the confirm dialog", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-tab-inactive").click();

    await page.getByTestId("mod-row-select-mod.905").locator('input[type="checkbox"]').check();
    await page.getByTestId("activate-with-dependencies-checkbox").locator("input").check();
    await page.getByTestId("activate-selected-button").click();
    await page.getByTestId("activate-confirm-submit").click();

    await page.getByTestId("pending-changes-rescan-button").click();
    await expect(page.getByTestId("pending-changes-banner")).toBeHidden();

    await page.getByTestId("mods-tab-active").click();
    await page.getByTestId("mods-search").fill("mod.906");
    await page.getByTestId("mod-row-select-mod.906").locator('input[type="checkbox"]').check();
    await page.getByTestId("deactivate-selected-button").click();

    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeVisible();
    // Names, not raw ids — same inactive-pool label lookup as above.
    await expect(page.getByTestId("deactivate-plan-dependents")).toContainText("Inactive Mod 5");
    await expect(page.getByTestId("deactivate-plan-dependents")).toContainText("Inactive Mod 6");

    await page.getByTestId("deactivate-confirm-submit").click();
    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeHidden();

    const deactivateCalls = await page.evaluate(() => window.__DEACTIVATE_CALLS__ ?? []);
    expect(deactivateCalls).toEqual([{ ids: ["mod.906"] }]);

    // A mod activated, then rescanned, then deactivated (exactly this
    // sequence — `mod.906` above) is a real pending change: the mock
    // compares resulting sets, not deltas, so the banner must show the real
    // `unscanned.removed` entry.
    await expect(page.getByTestId("pending-changes-banner")).toBeVisible();
    await expect(page.getByTestId("pending-changes-unscanned-text")).toContainText("1");
  });
});
