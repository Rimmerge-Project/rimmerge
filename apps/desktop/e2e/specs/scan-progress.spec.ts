import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { installScenario } from "../fixtures/scenario";

test.describe("scan progress", () => {
  // The mock holds `load_project` open after emitting each stage's
  // `project://progress` event, so the setup page's caption can be read
  // while the scan is still "running".
  test("the setup page captions each scan stage as its progress event arrives", async ({
    page,
  }) => {
    await page.addInitScript(installScenario);
    await page.goto("/");
    await page.evaluate(() => {
      window.__LOAD_PROJECT_PROGRESS__ = [
        { stage: "discovering", done: 0, total: 0 },
        { stage: "scanning", done: 12, total: 60 },
      ];
    });
    await page.getByTestId("load-project-button").click();

    await expect(page.getByTestId("base-progress-caption")).toContainText("Scanning mods");

    await page.waitForFunction(() => typeof window.__RELEASE_LOAD_PROJECT__ === "function");
    await page.evaluate(() => window.__RELEASE_LOAD_PROJECT__?.());
    await page.waitForURL(/\/$/);
  });

  test("a later stage replaces the earlier stage's caption", async ({ page }) => {
    await page.addInitScript(installScenario);
    await page.goto("/");
    await page.evaluate(() => {
      window.__LOAD_PROJECT_PROGRESS__ = [
        { stage: "scanning", done: 60, total: 60 },
        { stage: "analyzing", done: 1, total: 3 },
        { stage: "collectingTagEvidence", done: 2, total: 3 },
      ];
    });
    await page.getByTestId("load-project-button").click();

    await expect(page.getByTestId("base-progress-caption")).toContainText(
      "Collecting tag evidence",
    );
    await expect(page.getByTestId("base-progress-caption")).not.toContainText("Scanning mods");

    await page.waitForFunction(() => typeof window.__RELEASE_LOAD_PROJECT__ === "function");
    await page.evaluate(() => window.__RELEASE_LOAD_PROJECT__?.());
    await page.waitForURL(/\/$/);
  });
});
