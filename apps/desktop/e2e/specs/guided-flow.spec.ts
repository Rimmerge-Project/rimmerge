import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

test.describe("guided flow", () => {
  test("a fresh load lands on Suggested with Apply as the current step", async ({ page }) => {
    await loadScenario(page);

    await expect(page.getByTestId("guide-step-choose")).toHaveAttribute("data-status", "done");
    await expect(page.getByTestId("guide-step-apply")).toHaveAttribute("data-status", "current");
    await expect(page.getByTestId("guide-step-apply")).toHaveAttribute("aria-current", "step");
    await expect(page.getByTestId("guide-step-confirm")).toHaveAttribute("data-status", "upcoming");
    await expect(page.getByTestId("guide-done")).toBeHidden();
  });

  test("Apply, confirm the problem, and the strip reads done", async ({ page }) => {
    await loadScenario(page);

    await page.getByTestId("dashboard-apply-button").click();
    await expect(page.getByTestId("guide-step-confirm")).toHaveAttribute("data-status", "current");
    await page.getByTestId("apply-dialog-submit").click();

    await expect(page.getByTestId("apply-confirm")).toBeVisible();
    await page.getByTestId("apply-confirm-apply-anyway").click();
    await expect(page.getByTestId("apply-dialog")).toBeHidden();

    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([
      { source: "suggested", writeModsConfig: true, writeMergeMod: false, force: false },
    ]);
    await expect(page.getByTestId("guide-done")).toContainText(
      "ModsConfig.xml matches the suggested order",
    );
    await expect(page.getByTestId("guide-step-apply")).toHaveAttribute("data-status", "done");
    await expect(page.getByTestId("guide-live-region")).toContainText("Applied.");
    await expect(page.getByTestId("dashboard-apply-button")).toBeHidden();
  });

  test("the Apply dialog opens from the strip with Write ModsConfig.xml checked", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("dashboard-apply-button").click();

    const checkbox = page.getByTestId("apply-dialog-write-modsconfig-checkbox").locator("input");
    await expect(checkbox).toBeChecked();
    await expect(page.getByTestId("apply-dialog-submit")).toHaveText("Apply");
    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();

    await expect(page.getByTestId("guide-done")).toBeVisible();
  });

  test("unchecking Write ModsConfig.xml saves only, and the strip stays on Apply", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-write-modsconfig-checkbox").click();
    await page.getByTestId("apply-dialog-submit").click();
    await expect(page.getByTestId("apply-dialog")).toBeHidden();

    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([
      { source: "suggested", writeModsConfig: false, writeMergeMod: false, force: false },
    ]);
    await expect(page.getByTestId("guide-done")).toBeHidden();
    await expect(page.getByTestId("guide-step-apply")).toHaveAttribute("data-status", "current");
  });

  test("on Current, step one offers the suggested order and focus moves to Apply", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("order-source-current").click();

    await expect(page.getByTestId("guide-step-choose")).toHaveAttribute("data-status", "current");
    await page.getByTestId("guide-choose-button").click();

    await expect(page.getByTestId("guide-step-apply")).toHaveAttribute("data-status", "current");
    await expect(page.getByTestId("dashboard-apply-button")).toBeFocused();
  });
});
