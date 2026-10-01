import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

test.describe("mod label mode", () => {
  test("t toggles the first finding card between the mod's name and its id, live", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-inbox").click();

    // `mod.000`'s scenario name is "Mod 0" — the lowest-confidence
    // finding (`missing_mod:mod.000`, see `handPicked` in
    // `e2e/fixtures/scenario.ts`) is the first card by default. Scoped to
    // the action-text span, not the whole card: the card's key subtitle
    // line (`missing_mod:mod.000`) always shows the raw id by design,
    // regardless of label mode, so asserting against the whole card
    // would spuriously fail the "not" side of these checks.
    const firstCard = page.getByTestId("finding-card-missing_mod:mod.000");
    const actionText = firstCard.getByTestId("card-action-text");
    await expect(actionText).toHaveText("Remove Mod 0 from the active list");
    await expect(page.getByTestId("mod-label-mode-name")).toHaveAttribute("aria-checked", "true");

    await page.keyboard.press("t");

    await expect(actionText).toHaveText("Remove mod.000 from the active list");
    await expect(page.getByTestId("mod-label-mode-id")).toHaveAttribute("aria-checked", "true");

    // Flips back — no reload needed either direction.
    await page.keyboard.press("t");
    await expect(actionText).toHaveText("Remove Mod 0 from the active list");
    await expect(page.getByTestId("mod-label-mode-name")).toHaveAttribute("aria-checked", "true");
  });

  test("clicking the segmented control's Ids segment also switches the mode", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-inbox").click();

    const firstCard = page.getByTestId("finding-card-missing_mod:mod.000");
    await expect(firstCard).toContainText("Remove Mod 0");

    await page.getByTestId("mod-label-mode-id").click();

    await expect(firstCard).toContainText("Remove mod.000");
    await expect(page.getByTestId("mod-label-mode-id")).toHaveAttribute("aria-checked", "true");
  });
});
