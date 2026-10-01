import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * Merge-first suggestions for clean previews: a clean, non-zero-op
 * `patchCollision` preview lists as an auto-accepted `Merge` card, a
 * zero-op one never does, and the findings list stays consistent across
 * a remount.
 *
 * The "auto Merge card" case runs against `TemperateForest` (a
 * `patchCollision`), not `CleanWall` (a `defOverride`): an undecided
 * `defOverride`'s action can never be `Merge`, only a
 * `patchCollision`'s can (mirroring RimWorld's own sequential patch
 * composition; a field-merged def override is a copy no author shipped,
 * so it enters the merge mod only by an explicit decision). `CleanWall`
 * gets its own test asserting that contract instead — see
 * `e2e/fixtures/scenario.ts`'s own `CleanWall` doc comment.
 */
test.describe("merge-first suggestions", () => {
  test("a clean patch collision with a real op lists as an auto Merge card with the state pill", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-inbox").click();
    // `TemperateForest` is `status: "auto"`, not the inbox's default
    // `needsInput` filter — see `e2e/fixtures/scenario.ts`'s own
    // `status-chip-all` note on why a real ledger's own confidence rules
    // can put a fixture finding above the default threshold.
    await page.getByTestId("status-chip-all").click();
    await page.getByTestId("finding-search").fill("TemperateForest");

    const card = page.getByTestId(
      "finding-card-patch_collision:BiomeDef/TemperateForest:defName:plantDensity:[examplebiomes.biomes,example.flora.core]",
    );
    await expect(card).toBeVisible();
    await expect(card.getByTestId("card-action-text")).toHaveText("Merge BiomeDef/TemperateForest");
    await expect(card.getByTestId("status-pill-auto")).toBeVisible();
    await expect(card.getByTestId("merge-state-pill")).toHaveText("merged");
  });

  // The exact same shape as
  // above — a clean, non-zero-op preview — but for a `defOverride`
  // (`CleanWall`) reads entirely differently: the ledger's own original
  // suggestion (`Accept` 60, "no strong signal") is untouched, and
  // `Merge` only ever leads the alternatives — never the action, never
  // an auto status, never a merge-state pill, however clean the preview.
  test("a clean def override with a real op still needs an explicit decision — Merge only leads the alternatives", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-inbox").click();
    await page.getByTestId("finding-search").fill("CleanWall");

    const card = page.getByTestId(
      "finding-card-def_override:ThingDef/CleanWall:[wall.core.mod,wall.addon.mod]",
    );
    await expect(card).toBeVisible();
    await expect(card.getByTestId("card-action-text")).toHaveText("Accept");
    await expect(card.getByTestId("status-pill-needsInput")).toBeVisible();
    await expect(card.getByTestId("merge-state-pill")).toHaveCount(0);

    await card.click();
    await expect(page.getByTestId("suggestion-panel")).toContainText(
      "The mods change different fields; merging would combine both.",
    );
  });

  test("a zero-op clean override (BionicHeart) never auto-promotes and stays Accept", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-inbox").click();
    await page.getByTestId("finding-search").fill("BionicHeart");

    const card = page.locator('[data-testid^="finding-card-def_override:HediffDef/BionicHeart"]');
    await expect(card).toBeVisible();
    await expect(card.getByTestId("card-action-text")).toHaveText("Accept");
    await expect(card.getByTestId("status-pill-needsInput")).toBeVisible();
    await expect(card.getByTestId("merge-state-pill")).toHaveCount(0);
  });

  test("loading the findings list twice, navigating away and back, yields the same card order and statuses", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-inbox").click();
    await page.getByTestId("status-chip-all").click();

    const snapshot = () =>
      page.locator('[data-testid^="finding-card-"]').evaluateAll((cards) =>
        cards.map((card) => ({
          key: card.getAttribute("data-testid"),
          status: card.querySelector('[data-testid^="status-pill-"]')?.getAttribute("data-testid"),
        })),
      );

    const first = await snapshot();
    expect(first.length).toBeGreaterThan(0);

    await page.getByTestId("nav-dashboard").click();
    await page.getByTestId("nav-inbox").click();
    await page.getByTestId("status-chip-all").click();
    const second = await snapshot();

    expect(second).toEqual(first);
  });
});
