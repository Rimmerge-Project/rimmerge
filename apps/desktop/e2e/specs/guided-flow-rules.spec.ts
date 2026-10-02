import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

// Step 1 of the Dashboard strip, "Get the recommended rules". The scenario starts with
// community rules cached and imported, the Steam Workshop database never downloaded, and the
// first-run notice unanswered; the mock derives every state and outcome from that, the way the
// backend does (see `recommendedRulesStepFor` in `fixtures/scenario.ts`).

test.describe("guided flow: get the recommended rules", () => {
  test("downloads and imports, shows progress, and the suggested order really changes", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("welcome-card-keep").click();
    // On Current, step 2 words how many mods the suggested order moves.
    await page.getByTestId("order-source-current").click();

    const rules = page.getByTestId("guide-step-rules");
    await expect(rules).toHaveAttribute("data-status", "current");
    await expect(rules).toHaveAttribute("aria-current", "step");
    await expect(page.getByTestId("guide-rules-source-steam")).toHaveText(
      "Steam Workshop: not downloaded yet.",
    );
    await expect(page.getByTestId("guide-rules-button")).toHaveText(
      "Download and import (about 49 MB)",
    );
    await expect(page.getByTestId("guide-step-choose")).toContainText("moves 3 mods");

    await page.evaluate(() => {
      window.__HOLD_RECOMMENDED_RULES__ = true;
    });
    await page.getByTestId("guide-rules-button").click();

    await expect(page.getByTestId("guide-rules-progress")).toHaveText(
      "Downloading the Steam Workshop database…",
    );
    await expect(page.getByTestId("guide-rules-slow-note")).toBeVisible();
    await expect(page.getByTestId("guide-rules-live-region")).toHaveText(
      "Downloading the Steam Workshop database…",
    );
    await expect(page.getByTestId("guide-rules-button")).toBeDisabled();
    await expect(page.getByTestId("guide-rules-skip")).toBeHidden();

    await page.evaluate(() => window.__RELEASE_RECOMMENDED_RULES__?.());

    await expect(rules).toHaveAttribute("data-status", "done");
    await expect(page.getByTestId("guide-rules-done-text")).toHaveText(
      "Imported. The suggested order uses them.",
    );
    await expect(page.getByText("Recommended rules imported.")).toBeVisible();
    // The import moved two more mods in the suggested order, and the Dashboard refetched it.
    await expect(page.getByTestId("moved-mods")).toHaveText("5");
    await expect(page.getByTestId("guide-step-choose")).toContainText("moves 5 mods");
    await expect(page.getByTestId("guide-choose-button")).toBeFocused();

    expect(await page.evaluate(() => window.__GET_RECOMMENDED_RULES_CALLS__)).toBe(1);
    expect(await page.evaluate(() => window.__RECOMMENDED_RULES_PROGRESS__)).toEqual([
      { kind: "downloading", database: "steam" },
      { kind: "importing", databases: ["steam"] },
    ]);
  });

  test("a failed download leaves the step offered as Try again, with the reason", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("welcome-card-keep").click();
    await page.evaluate(() => {
      window.__RECOMMENDED_RULES_FAILING_SOURCES__ = ["steam"];
    });

    await page.getByTestId("guide-rules-button").click();

    await expect(page.getByText("The rule databases couldn't be downloaded.")).toBeVisible();
    await expect(page.getByTestId("guide-step-rules")).toHaveAttribute("data-status", "current");
    await expect(page.getByTestId("guide-rules-button")).toHaveText("Try again (about 49 MB)");
    await expect(page.getByTestId("guide-rules-source-steam")).toContainText(
      "the last download failed",
    );
    // Nothing was imported, so the suggested order did not move.
    await expect(page.getByTestId("moved-mods")).toHaveText("3");
  });

  test("Skip is remembered across a reload, and Get them now still works afterwards", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("welcome-card-keep").click();

    await page.getByTestId("guide-rules-skip").click();

    await expect(page.getByTestId("guide-step-rules")).toHaveAttribute("data-status", "skipped");
    await expect(page.getByTestId("guide-step-apply")).toHaveAttribute("data-status", "current");
    await expect(page.getByTestId("dashboard-apply-button")).toBeFocused();
    expect(await page.evaluate(() => window.__SKIP_RECOMMENDED_RULES_CALLS__)).toBe(1);

    await page.reload();
    await page.getByTestId("load-project-button").click();
    await page.waitForURL(/\/$/);

    await expect(page.getByTestId("guide-step-rules")).toHaveAttribute("data-status", "skipped");
    await expect(page.getByTestId("guide-rules-skipped-text")).toContainText("Skipped.");

    // The scenario forgot the first-run answer on reload; give it again, then get the rules.
    await page.getByTestId("welcome-card-keep").click();
    await page.getByTestId("guide-rules-get-now").click();

    await expect(page.getByTestId("guide-step-rules")).toHaveAttribute("data-status", "done");
    expect(await page.evaluate(() => window.__GET_RECOMMENDED_RULES_CALLS__)).toBe(1);
  });

  test("after Skip with Steam turned off, the row still lists the sources and the download size", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("welcome-card-keep").click();
    await page.getByTestId("nav-rules").click();
    const steamToggle = page.getByTestId("rule-database-toggle-steam").locator("input");
    await steamToggle.uncheck({ force: true });
    await expect(steamToggle).not.toBeChecked();
    await page.getByTestId("nav-dashboard").click();

    await page.getByTestId("guide-rules-skip").click();

    await expect(page.getByTestId("guide-step-rules")).toHaveAttribute("data-status", "skipped");
    await expect(page.getByTestId("guide-rules-source-steam")).toHaveText(
      "Steam Workshop: off. It will be turned on and downloaded.",
    );
    await expect(page.getByTestId("guide-rules-get-now")).toHaveText("Get them now (about 49 MB)");
  });

  test("a click refused for the unanswered first-run notice says so and changes nothing", async ({
    page,
  }) => {
    await loadScenario(page);
    // Skip works while the step is unavailable; Get them now then reaches the closed gate.
    await page.getByTestId("guide-rules-skip").click();
    await expect(page.getByTestId("guide-step-rules")).toHaveAttribute("data-status", "skipped");

    await page.getByTestId("guide-rules-get-now").click();

    await expect(
      page.getByText("Answer the Welcome notice on the Dashboard first, then try again."),
    ).toBeVisible();
    await expect(page.getByTestId("guide-step-rules")).toHaveAttribute("data-status", "skipped");
    await expect(page.getByTestId("moved-mods")).toHaveText("3");
  });

  test("with internet access off the step is unavailable: no button, a Settings link, flow moves on", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("welcome-card-turn-off").click();

    await expect(page.getByTestId("guide-step-rules")).toHaveAttribute(
      "data-status",
      "unavailable",
    );
    await expect(page.getByTestId("guide-rules-unavailable")).toHaveText(
      "Internet access is off, so nothing can be downloaded.",
    );
    await expect(page.getByTestId("guide-rules-button")).toBeHidden();
    await expect(page.getByTestId("guide-rules-skip")).toBeVisible();
    await expect(page.getByTestId("guide-step-apply")).toHaveAttribute("data-status", "current");
    expect(await page.evaluate(() => window.__GET_RECOMMENDED_RULES_CALLS__)).toBe(0);

    await page.getByTestId("guide-rules-settings-link").click();
    await expect(page).toHaveURL(/\/settings$/);
  });

  test("before the first-run notice is answered the step asks for it, with no button", async ({
    page,
  }) => {
    await loadScenario(page);

    await expect(page.getByTestId("guide-step-rules")).toHaveAttribute(
      "data-status",
      "unavailable",
    );
    await expect(page.getByTestId("guide-rules-unavailable")).toHaveText(
      "Answer the Welcome notice above first.",
    );
    await expect(page.getByTestId("guide-rules-button")).toBeHidden();
  });

  test("the recommended-sources notice steps aside while the step is offered, and returns after Skip", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("welcome-card-keep").click();

    await page.getByTestId("notification-bell").click();
    await expect(page.getByTestId("notification-item-recommendedSourcesIncomplete")).toBeHidden();
    await page.keyboard.press("Escape");

    await page.getByTestId("guide-rules-skip").click();
    await expect(page.getByTestId("guide-step-rules")).toHaveAttribute("data-status", "skipped");

    await page.getByTestId("notification-bell").click();
    await expect(page.getByTestId("notification-sources-recommendedSourcesIncomplete")).toHaveText(
      /Steam Workshop: on, not downloaded yet/,
    );
  });
});
