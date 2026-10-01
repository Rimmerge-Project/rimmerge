import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * The merge editor's Drop/decide
 * behaviour on a `PatchCollision` finding — `patch_collision:ThingDef/
 * Widget:...` in `e2e/fixtures/scenario.ts`'s own mock backend (a real,
 * two-owner `Conflict` field: `label`). Covers Drop being hidden and a
 * decided field rendering its real value (not "absent"/"needs input")
 * end to end; the caching/invalidation half is covered at
 * the unit level (`src/queries/merge.test.ts`,
 * `src/composables/useSessionEvents.test.ts`), since a mock-IPC Playwright
 * run has no real `rim_session::use_cases::PlanMerge` replay to measure.
 */
async function openFindingInInbox(
  page: import("@playwright/test").Page,
  needle: string,
): Promise<void> {
  await page.getByTestId("nav-inbox").click();
  await page.getByTestId("finding-search").fill(needle);
  await expect.poll(async () => page.locator('[data-testid^="finding-card-"]').count()).toBe(1);
  await page.locator('[data-testid^="finding-card-"]').first().click();
  await expect(page.getByTestId("suggestion-panel")).toBeVisible();
}

const WIDGET_KEY = "patch_collision:ThingDef/Widget:def_name:label:[widget.mod.a,widget.mod.b]";

test.describe("merge editor — patch collision Drop/decide", () => {
  test("the Drop button is replaced by a disabled, tooltipped placeholder, and the d key is inert", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "Widget");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    const row = page.getByTestId("merge-field-row").first();
    await expect(row).toContainText("label");
    await expect(row.getByTestId("merge-drop-button")).toHaveCount(0);
    const placeholder = row.getByTestId("merge-drop-unavailable");
    await expect(placeholder).toBeVisible();
    await expect(placeholder).toBeDisabled();
    await expect(placeholder).toHaveAttribute("title", /patch collision/);

    // `d` (toggle drop) must be a no-op here — no `set_merge_choices` call
    // at all, not even an empty-choices one.
    await page.keyboard.press("d");
    await page.waitForTimeout(250); // past the 150ms debounce, if it fired
    const calls = await page.evaluate(() => window.__SET_MERGE_CHOICES_CALLS__ ?? []);
    expect(calls).toEqual([]);
  });

  test("choosing an owner's value on a conflicted collision field sends the payload and renders the real value, never absent/needs-input", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "Widget");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    const row = page.getByTestId("merge-field-row").first();
    await expect(row).toContainText("label");
    await expect(row.getByTestId("merge-result-needs-input")).toBeVisible();

    // Column 2 = the second owner (`widget.mod.b`) — 1 is the base.
    await page.keyboard.press("2");

    await expect
      .poll(() => page.evaluate(() => window.__SET_MERGE_CHOICES_CALLS__ ?? []))
      .toEqual([
        {
          key: WIDGET_KEY,
          choices: { label: { choice: "from", modId: "widget.mod.b" } },
          patchId: null,
        },
      ]);

    await expect(row.getByTestId("merge-result-needs-input")).toHaveCount(0);
    await expect(row).toContainText("beta widget");
  });
});
