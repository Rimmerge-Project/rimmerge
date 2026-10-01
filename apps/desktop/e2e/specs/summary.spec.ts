import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * Searches the inbox down to exactly the finding named `needle`, selects
 * it, and waits for its detail to load. Mirrors `merge.spec.ts`'s own
 * helper of the same name (kept file-local rather than shared — the two
 * specs' needs haven't diverged enough yet to justify a shared module,
 * and duplicating four lines is cheaper than a premature abstraction).
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

test.describe("def conflict view", () => {
  // `DefConflictView` is the panel
  // for `defOverride`/`patchCollision`/
  // `duplicateTemplateName` findings in the profile inbox — `BionicHeart`/
  // `HeadNormal`/`plantDensity` are three fixtures asserting its own
  // markup. `e2e/specs/def-conflict.spec.ts` covers the two
  // fixtures added specifically for this view (the list merge and
  // the unsupported-op stopper).
  test("a def override with no conflicts shows every changed field, grouped, and Complete", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "BionicHeart");

    const view = page.getByTestId("def-conflict-view");
    await expect(view.getByTestId("def-conflict-view-def-ref")).toHaveText("HediffDef/BionicHeart");
    await expect(view.getByTestId("def-conflict-view-completeness")).toHaveText("Complete");
    // Onlychanged hides `defName` and the 40 synthetic unchanged fields —
    // 4 `CleanMerge` fields plus the one inherited `li` (`ListEntry`).
    await expect(view.getByTestId("def-conflict-field-row")).toHaveCount(5);
    await expect(view.getByTestId("def-conflict-view-problems")).toHaveCount(0);
  });

  test("a def override with two conflicting fields shows both, each with its winner", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "HeadNormal");

    const view = page.getByTestId("def-conflict-view");
    const rows = view.getByTestId("def-conflict-field-row");
    // `skinColorOverride` and `eyeSize` conflict; `label` is a clean
    // one-sided merge; `hitInfo` (unchanged) stays hidden by default.
    await expect(rows).toHaveCount(3);

    // Mod-label-mode defaults to "name"; `list_mod_names` maps these ids
    // to their display names, so that's what renders.
    const skinColor = view.locator('[data-path="skinColorOverride"]');
    await expect(skinColor).toContainText("Example Animation");
    await expect(skinColor).toContainText("Example Bionics Fork");
    await expect(skinColor.getByTestId("def-conflict-preference")).toContainText(
      "Example Animation wins (load order)",
    );

    const eyeSize = view.locator('[data-path="eyeSize"]');
    await expect(eyeSize.getByTestId("def-conflict-preference")).toContainText(
      "Example Animation wins (load order)",
    );
  });

  test("a patch collision shows the contested field, both sources, and the base value", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-inbox").click();
    // `TemperateForest` is `status: "auto"` (the "auto Merge card" case,
    // `clean-merge.spec.ts`) — not the inbox's default `needsInput`
    // filter.
    await page.getByTestId("status-chip-all").click();
    await page.getByTestId("finding-search").fill("plantDensity");
    await expect.poll(async () => page.locator('[data-testid^="finding-card-"]').count()).toBe(1);
    await page.locator('[data-testid^="finding-card-"]').first().click();
    await expect(page.getByTestId("suggestion-panel")).toBeVisible();

    const view = page.getByTestId("def-conflict-view");
    await expect(view.getByTestId("def-conflict-view-kind")).toContainText("plantDensity");
    const row = view.locator('[data-path="plantDensity"]');
    await expect(row).toBeVisible();
    await expect(row).toContainText("Flora: Core");
    await expect(row).toContainText("Example Biomes");
    await expect(row).toContainText("0.9");
  });

  test("a texture override shows both owners' images, badges the winner, and 'Use this one' records the exact decide payload", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "BionicWall");

    const pair = page.getByTestId("texture-pair");
    await expect(pair.getByTestId("texture-image")).toHaveCount(2);

    // The scenario loads on the Suggested order, where `mod.010` loads last.
    const winnerTile = pair.getByTestId("texture-tile-mod.010");
    await expect(winnerTile.getByTestId("texture-winner-badge")).toBeVisible();
    const otherTile = pair.getByTestId("texture-tile-mod.015");
    await expect(otherTile.getByTestId("texture-winner-badge")).toHaveCount(0);

    await otherTile.getByTestId("texture-use-this-one").click();

    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__ ?? []))
      .toEqual([
        {
          key: "texture_override:Things/BionicWall:[mod.010,mod.015]",
          action: {
            kind: "preferWinner",
            key: { defType: "texture", defName: "Things/BionicWall" },
            winner: "mod.015",
          },
          note: null,
        },
      ]);
  });

  test("a texture override's winner badge follows the selected order, not the owner list", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "BionicWall");

    const pair = page.getByTestId("texture-pair");
    const first = pair.getByTestId("texture-tile-mod.010");
    const second = pair.getByTestId("texture-tile-mod.015");

    // Suggested swaps this pair, so the owner list's last entry (`mod.015`)
    // is the Current winner only.
    await expect(first.getByTestId("texture-winner-badge")).toBeVisible();
    await expect(second.getByTestId("texture-winner-badge")).toHaveCount(0);

    await page.getByTestId("order-source-current").click();
    await expect(second.getByTestId("texture-winner-badge")).toBeVisible();
    await expect(first.getByTestId("texture-winner-badge")).toHaveCount(0);

    await page.getByTestId("order-source-suggested").click();
    await expect(first.getByTestId("texture-winner-badge")).toBeVisible();
    await expect(second.getByTestId("texture-winner-badge")).toHaveCount(0);
  });
});
