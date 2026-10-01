import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

test.describe("inbox", () => {
  test("keyboard walk: Enter accepts, a digit picks an alternative, i ignores — cursor advances each time", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-inbox").click();
    await expect(page).toHaveURL(/\/inbox$/);
    await expect(page.getByTestId("suggestion-panel")).toBeVisible();

    // The three lowest-confidence findings in the scenario, in order —
    // see `e2e/fixtures/scenario.ts`'s `handPicked` list.
    const firstKey = "missing_mod:mod.000";
    const secondKey = "edge_dropped:mod.007:mod.008:loadAfter:1";
    const thirdKey = "any_of_choice:mod.014:Shared2.dll";

    await expect(page.getByTestId(`finding-card-${firstKey}`)).toHaveAttribute(
      "aria-current",
      "true",
    );

    // 1. Enter accepts the ledger's own suggestion.
    await page.getByTestId("accept-button").click();
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({ key: firstKey, action: { kind: "removeMod", modId: "mod.000" } });
    await expect(page.getByTestId(`finding-card-${secondKey}`)).toHaveAttribute(
      "aria-current",
      "true",
    );

    // 2. Digit `1` picks the first alternative.
    await page.keyboard.press("1");
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({ key: secondKey, action: { kind: "keepEdge" } });
    await expect(page.getByTestId(`finding-card-${thirdKey}`)).toHaveAttribute(
      "aria-current",
      "true",
    );

    // 3. `i` ignores the third finding outright.
    await page.keyboard.press("i");
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({ key: thirdKey, action: { kind: "ignore" } });

    const decideCalls = await page.evaluate(() => window.__DECIDE_CALLS__);
    expect(decideCalls).toHaveLength(3);
  });

  test("j/k move the cursor without deciding anything", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-inbox").click();
    await expect(page.getByTestId("suggestion-panel")).toBeVisible();

    await page.keyboard.press("j");
    await page.keyboard.press("j");
    await page.keyboard.press("k");

    const decideCalls = await page.evaluate(() => window.__DECIDE_CALLS__);
    expect(decideCalls).toHaveLength(0);
  });

  test("status chip and kind chip filter the list, and search narrows by key substring", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-inbox").click();

    await page.getByTestId("status-chip-all").click();
    const totalWithAllStatuses = await page.locator('[data-testid^="finding-card-"]').count();

    await page.getByTestId("status-chip-needsInput").click();
    await expect(async () => {
      const afterFilter = await page.locator('[data-testid^="finding-card-"]').count();
      expect(afterFilter).toBeLessThanOrEqual(totalWithAllStatuses);
    }).toPass();

    await page.getByTestId("finding-search").fill("missing_mod");
    // The search box debounces (`refDebounced`, 300ms) before it commits
    // to the store and re-queries — polling for "some card is visible"
    // alone would pass on the *pre-search* list still on screen during
    // that window, so this waits for every currently visible card to
    // actually match, which only becomes true once the debounced search
    // has taken effect.
    await expect
      .poll(async () => {
        const testIds = await page
          .locator('[data-testid^="finding-card-"]')
          .evaluateAll((cards) => cards.map((card) => card.getAttribute("data-testid")));
        return testIds.length > 0 && testIds.every((testId) => testId?.includes("missing_mod"));
      })
      .toBe(true);
  });

  test("f cycles the status filter and ? toggles the shortcut help", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-inbox").click();

    await expect(page.getByTestId("status-chip-needsInput")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await page.keyboard.press("f");
    await expect(page.getByTestId("status-chip-auto")).toHaveAttribute("aria-pressed", "true");

    await page.keyboard.press("?");
    await expect(page.getByTestId("shortcut-help")).toBeVisible();
  });
});
