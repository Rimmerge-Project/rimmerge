import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

const SIDEBAR_LAUNCH = "shell-launch-button";

test.describe("launch RimWorld", () => {
  test("a matching ModsConfig.xml launches once, shows the toast and reads Starting", async ({
    page,
  }) => {
    await loadScenario(page);
    // The file holds the current order, so selecting it makes the status ready.
    await page.getByTestId("order-source-current").click();
    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toBeEnabled();

    await page.getByTestId(SIDEBAR_LAUNCH).click();

    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toHaveText("Starting RimWorld…");
    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toBeDisabled();
    await expect(page.getByText("Asked Steam to start RimWorld.").first()).toBeVisible();
    expect(await page.evaluate(() => window.__LAUNCH_GAME_CALLS__ ?? [])).toEqual([
      { ifNotApplied: "refuse" },
    ]);
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([]);
  });

  test("the strip's done line launches the same way once the order is applied", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();
    await expect(page.getByTestId("guide-done")).toBeVisible();

    await page.getByTestId("guide-launch-button").click();

    await expect(page.getByTestId("guide-launch-button")).toHaveText("Starting RimWorld…");
    expect(await page.evaluate(() => window.__LAUNCH_GAME_CALLS__ ?? [])).toEqual([
      { ifNotApplied: "refuse" },
    ]);
  });

  test("an unapplied order asks first; Cancel is focused and launches nothing", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId(SIDEBAR_LAUNCH).click();

    await expect(page.getByTestId("launch-apply-first")).toBeVisible();
    await expect(page.getByTestId("launch-apply-first-sentence")).toContainText(
      "ModsConfig.xml doesn't hold the suggested order",
    );
    await expect(page.getByTestId("launch-apply-first-cancel")).toBeFocused();
    await page.getByTestId("launch-apply-first-cancel").click();
    await expect(page.getByTestId("launch-apply-first")).toBeHidden();
    expect(await page.evaluate(() => window.__LAUNCH_GAME_CALLS__ ?? [])).toEqual([]);
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([]);
  });

  test("Apply first applies through the Apply dialog, then launches once", async ({ page }) => {
    await loadScenario(page);

    await page.getByTestId(SIDEBAR_LAUNCH).click();
    await page.getByTestId("launch-apply-first-apply").click();
    await expect(page.getByTestId("apply-dialog")).toBeVisible();
    expect(await page.evaluate(() => window.__LAUNCH_GAME_CALLS__ ?? [])).toEqual([]);
    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();

    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toHaveText("Starting RimWorld…");
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([
      { source: "suggested", writeModsConfig: true, writeMergeMod: false, force: false },
    ]);
    expect(await page.evaluate(() => window.__LAUNCH_GAME_CALLS__ ?? [])).toEqual([
      { ifNotApplied: "refuse" },
    ]);
  });

  test("Apply first saves only, when ModsConfig.xml is unchecked, and does not launch", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId(SIDEBAR_LAUNCH).click();
    await page.getByTestId("launch-apply-first-apply").click();
    await page.getByTestId("apply-dialog-write-modsconfig-checkbox").click();
    await page.getByTestId("apply-dialog-submit").click();

    await expect(page.getByTestId("apply-dialog")).toBeHidden();
    await expect(
      page.getByText("ModsConfig.xml wasn't written, so RimWorld wasn't started."),
    ).toBeVisible();
    expect(await page.evaluate(() => window.__LAUNCH_GAME_CALLS__ ?? [])).toEqual([]);
  });

  test("Launch anyway launches with launchAnyway and applies nothing", async ({ page }) => {
    await loadScenario(page);

    await page.getByTestId(SIDEBAR_LAUNCH).click();
    await page.getByTestId("launch-apply-first-anyway").click();

    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toHaveText("Starting RimWorld…");
    expect(await page.evaluate(() => window.__LAUNCH_GAME_CALLS__ ?? [])).toEqual([
      { ifNotApplied: "launchAnyway" },
    ]);
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([]);
  });

  test("a running game disables the button, and one poll later it is enabled again", async ({
    page,
  }) => {
    await page.clock.install();
    await page.addInitScript(() => {
      window.__GAME_RUNNING__ = true;
    });
    await loadScenario(page);

    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toHaveText("RimWorld is running");
    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toBeDisabled();
    expect(await page.evaluate(() => window.__LAUNCH_GAME_CALLS__ ?? [])).toEqual([]);

    await page.evaluate(() => {
      window.__GAME_RUNNING__ = false;
    });
    await page.clock.runFor(5000);

    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toHaveText("Launch RimWorld");
    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toBeEnabled();
  });

  test("a game that starts while Rimmerge is open shows as running within one poll", async ({
    page,
  }) => {
    await page.clock.install();
    await loadScenario(page);
    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toBeEnabled();

    await page.evaluate(() => {
      window.__GAME_RUNNING__ = true;
    });
    await page.clock.runFor(5000);

    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toHaveText("RimWorld is running");
    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toBeDisabled();
  });

  test("an install without RimWorldWin64.exe says so and cannot be launched", async ({ page }) => {
    await page.addInitScript(() => {
      window.__LAUNCH_ROUTE__ = "unavailable";
    });
    await loadScenario(page);

    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toBeDisabled();
    await expect(page.getByTestId("launch-game-hint")).toHaveText(
      "RimWorldWin64.exe isn't in the install folder.",
    );
  });

  test("a status call held behind a long command is never stacked", async ({ page }) => {
    await page.clock.install();
    await loadScenario(page);
    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toBeEnabled();
    const statusCalls = () => page.evaluate(() => window.__GAME_LAUNCH_STATUS_CALLS__ ?? 0);
    const base = await statusCalls();
    await page.evaluate(() => {
      window.__HOLD_GAME_LAUNCH_STATUS__ = true;
    });

    await page.clock.runFor(5000);
    await expect.poll(statusCalls).toBe(base + 1);
    await page.clock.runFor(20000);
    // Four more periods passed with the first call still pending: nothing was queued behind it.
    expect(await statusCalls()).toBe(base + 1);

    await page.evaluate(() => {
      window.__HOLD_GAME_LAUNCH_STATUS__ = false;
      window.__GAME_RUNNING__ = true;
      window.__RELEASE_GAME_LAUNCH_STATUS__?.();
    });
    await expect(page.getByTestId(SIDEBAR_LAUNCH)).toHaveText("RimWorld is running");
    await page.clock.runFor(5000);
    await expect.poll(statusCalls).toBeGreaterThan(base + 1);
  });

  test("staged activation changes ask first, with their own sentence", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-tab-inactive").click();
    await page.getByTestId("mod-row-select-mod.905").locator('input[type="checkbox"]').check();
    await page.getByTestId("activate-selected-button").click();
    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("pending-changes-banner")).toBeVisible();

    await page.getByTestId(SIDEBAR_LAUNCH).click();

    await expect(page.getByTestId("launch-apply-first-sentence")).toContainText(
      "Your pending activation changes aren't in ModsConfig.xml yet.",
    );
    expect(await page.evaluate(() => window.__LAUNCH_GAME_CALLS__ ?? [])).toEqual([]);
  });
});
