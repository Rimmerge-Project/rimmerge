import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * Reaches the def page in-app via the mods page's search box — never
 * `page.goto` (a full page load resets the client-only `session.loaded`
 * state and bounces the router guard back to `/setup`, per `mods.spec.ts`'s
 * own note).
 */
async function gotoDef(page: Page, needle: string, defRef: string): Promise<void> {
  await page.getByTestId("nav-mods").click();
  await page.getByTestId("def-search-input").fill(needle);
  const result = page.getByTestId(`def-search-result-${defRef}`);
  await expect(result).toBeVisible();
  await result.click();
  await expect(page.getByTestId("def-page-ref")).toHaveText(defRef);
}

test.describe("def inspector", () => {
  test("a mod's changes row opens the def page, which links back to its finding", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-search").fill("RimWorld");
    await page.getByTestId("mod-row-ludeon.rimworld").click();
    await expect(page).toHaveURL(/\/mods\/ludeon\.rimworld$/);
    // The mod info panel's own "All details" link, not the panel itself
    // — `mod-changes-row-*` lives on the full detail page.
    await page.getByTestId("mod-info-all-details-link").click();
    await expect(page).toHaveURL(/\/mods\/ludeon\.rimworld\/details$/);

    const changesRow = page.getByTestId("mod-changes-row-HediffDef/BionicHeart");
    await expect(changesRow).toBeVisible();
    await changesRow.getByRole("link", { name: "HediffDef/BionicHeart" }).click();

    await expect(page.getByTestId("def-page-ref")).toHaveText("HediffDef/BionicHeart");
    // `useModLabel` defaults to name mode, so owners render as their
    // display names, not their bare ids.
    await expect(page.getByTestId("def-page-owners")).toContainText("RimWorld");
    await expect(page.getByTestId("def-page-owners")).toContainText("Example Bionics Fork");
    await expect(page.getByTestId("def-page-fields")).toContainText("synthetic heart");

    const findingPill = page.getByTestId("def-page-findings").getByRole("link").first();
    await expect(findingPill).toBeVisible();
    await findingPill.click();
    await expect(page).toHaveURL(/\/inbox\?search=/);
  });

  test("switching the order source updates the def page's source badge", async ({ page }) => {
    await loadScenario(page);
    await gotoDef(page, "BionicHeart", "HediffDef/BionicHeart");
    await expect(page.getByTestId("def-page-source")).toHaveText("Suggested");

    await page.getByTestId("order-source-current").click();
    await expect(page.getByTestId("def-page-source")).toHaveText("Current");
  });

  test("expanding a patcher shows its operation", async ({ page }) => {
    await loadScenario(page);
    await gotoDef(page, "TemperateForest", "BiomeDef/TemperateForest");

    await expect(page.getByTestId("def-page-owners")).toContainText("RimWorld");
    const patcherToggle = page.getByTestId("def-page-patcher-toggle-example.flora.core");
    await expect(patcherToggle).toBeVisible();
    await patcherToggle.click();
    await expect(page.getByTestId("def-page-patcher-ops-example.flora.core")).toContainText(
      "PatchOperationReplace",
    );
  });

  test("the only-patched toggle hides owner-attributed rows", async ({ page }) => {
    await loadScenario(page);
    await gotoDef(page, "BionicHeart", "HediffDef/BionicHeart");

    const fields = page.getByTestId("def-page-fields");
    await expect(fields).toContainText("synthetic heart");

    await page.getByTestId("def-page-only-patched").check();
    // Every `BionicHeart` field is `Owner`-attributed in this fixture (a
    // `defOverride` with no patch ops modeled) — toggling "only patched"
    // hides all of them.
    await expect(fields).not.toContainText("synthetic heart");
  });

  test("the def search box shows the owner count and jumps straight to a def page", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("def-search-input").fill("BionicHeart");
    const result = page.getByTestId("def-search-result-HediffDef/BionicHeart");
    await expect(result).toContainText("(2)");

    await result.click();
    await expect(page.getByTestId("def-page-ref")).toHaveText("HediffDef/BionicHeart");
  });

  test("the def search results stay inside the box, so the Mods page does not scroll sideways", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1024, height: 800 });
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("def-search-input").fill("Extraordinarily");
    const results = page.getByTestId("def-search-results");
    await expect(results).toBeVisible();

    // Measure the shell's `<main>`: it is the scroll container, so the
    // document's own scroll width stays equal even when the list overhangs.
    const overflow = await page.locator("main").evaluate((main) => ({
      scrollWidth: main.scrollWidth,
      clientWidth: main.clientWidth,
    }));
    expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.clientWidth);

    const box = await page.getByTestId("def-search-box").boundingBox();
    const list = await results.boundingBox();
    if (box === null || list === null) {
      throw new Error("the def search box and its result list must both have a layout box");
    }
    expect(list.x + list.width).toBeLessThanOrEqual(box.x + box.width + 0.5);
  });

  test("a def name with a space in it survives the search-box round trip unmangled", async ({
    page,
  }) => {
    await loadScenario(page);
    // `vue-router`'s own param encoder percent-encodes the embedded space
    // going out and decodes it back on the way in — the def page must
    // still display the literal, unmangled name, not `%20` or `+`.
    await gotoDef(page, "Tribal", "RaidStrategyDef/Tribal Siege");

    await expect(page).toHaveURL(/\/defs\/RaidStrategyDef%2FTribal%20Siege$/);
    await expect(page.getByTestId("def-page-owners")).toContainText("Raid Core");
    await expect(page.getByTestId("def-page-owners")).toContainText("Raid Expanded");
  });
});
