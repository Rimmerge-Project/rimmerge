import type { Page } from "@playwright/test";
import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * The four dedicated
 * fixtures `e2e/fixtures/scenario.ts` adds for `runtimePatchCollision`
 * (regrouped per-target), `ruleOverruled`, `placementOverruled`,
 * and `placementQuestioned` — one spec per kind, each asserting the
 * card's title, the panel's evidence, and the exact `decide` payload an
 * alternative (or, where the placement rules leave none, `Accept`) sends.
 * Every fixture is `status: "auto"`, so each test switches to the "all"
 * status filter first, same as `clean-merge.spec.ts`'s own auto-status
 * case.
 */

async function openFindingInInbox(page: Page, needle: string): Promise<void> {
  await page.getByTestId("nav-inbox").click();
  await page.getByTestId("status-chip-all").click();
  await page.getByTestId("finding-search").fill(needle);
  await expect.poll(async () => page.locator('[data-testid^="finding-card-"]').count()).toBe(1);
  await page.locator('[data-testid^="finding-card-"]').first().click();
  await expect(page.getByTestId("suggestion-panel")).toBeVisible();
}

test.describe("runtime-patch, rule, and placement findings", () => {
  test("runtime patch collision: card title, owner order with the last patcher marked, and a per-owner Run-last payload", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "alpha.runtimepatch.patcher");

    const card = page.locator('[data-testid^="finding-card-"]');
    await expect(card.getByTestId("card-action-text")).toHaveText(
      "Verse.Pawn.Kill patched by 3 mods",
    );

    const payload = page.getByTestId("finding-payload");
    await expect(payload).toContainText("Verse.Pawn::Kill");
    await expect(payload.getByTestId("runtime-patch-last-patcher")).toBeVisible();
    await expect(
      payload.getByTestId("runtime-patch-run-last-gamma.runtimepatch.patcher"),
    ).toHaveCount(0);

    await payload.getByTestId("runtime-patch-run-last-alpha.runtimepatch.patcher").click();
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({
        key: "runtime_patch_collision:Verse.Pawn:Kill:[alpha.runtimepatch.patcher,beta.runtimepatch.patcher,gamma.runtimepatch.patcher]",
        action: {
          kind: "preferWinner",
          key: { defType: "runtime_target", defName: "Verse.Pawn::Kill" },
          winner: "alpha.runtimepatch.patcher",
        },
        note: null,
      });
  });

  test("rule overruled: card title, the winner's layer and detail, and the Reorder alternative's exact payload", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "cabin.furniture.mod");

    // The card title is built
    // backend-side straight from the typed `Finding` (`title`), which
    // names the winner too when one exists.
    const card = page.locator('[data-testid^="finding-card-"]');
    await expect(card.getByTestId("card-action-text")).toHaveText(
      "Rule cabin.core.mod before cabin.furniture.mod overruled by cabin.core.mod",
    );

    const payload = page.getByTestId("finding-payload");
    await expect(payload).toContainText("cabin.furniture.mod");
    await expect(payload).toContainText("cabin.core.mod");
    await expect(payload).toContainText("Declared");
    await expect(payload).toContainText("declares a modDependency on cabin.furniture.mod");

    // Reorder first, Promote second — `rule_overruled`'s own push order
    // (Reorder is gated on the winner's layer, Promote independently on
    // the rule's origin; both apply here).
    await page.getByTestId("alternative-1").click();
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({
        key: "rule_overruled:cabin.furniture.mod:cabin.core.mod:steam_db",
        action: { kind: "reorder", after: "cabin.furniture.mod", before: "cabin.core.mod" },
        note: null,
      });
  });

  test("placement overruled: card title, the winner and landed-at position, and no alternative can beat a Hard winner", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "basement.optimizer.mod");

    // The title text is built backend-side.
    const card = page.locator('[data-testid^="finding-card-"]');
    await expect(card.getByTestId("card-action-text")).toHaveText(
      "Bottom placement of basement.optimizer.mod overruled",
    );

    const payload = page.getByTestId("finding-payload");
    await expect(payload).toContainText("Bottom");
    await expect(payload).toContainText("RimSort community");
    await expect(payload).toContainText("ships a load-time AssemblyRef");
    await expect(payload).toContainText("Landed at #8");

    // "No alternative can beat a Hard winner" — `placement_overruled`
    // offers none here, so this asserts the Accept payload instead of an
    // alternative's.
    await expect(page.getByTestId("alternatives-section")).toHaveCount(0);
    await page.getByTestId("accept-button").click();
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({
        key: "placement_overruled:basement.optimizer.mod:bottom:rim_sort_community",
        action: { kind: "accept" },
        note: null,
      });
  });

  test("placement questioned: card title, the advisory relation, and the Reorder alternative's exact payload", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "priority.loader.mod");

    // The title text is built backend-side, and also names the relation's
    // other mod.
    const card = page.locator('[data-testid^="finding-card-"]');
    await expect(card.getByTestId("card-action-text")).toHaveText(
      "Top placement of priority.loader.mod questioned by type.reference.mod",
    );

    const payload = page.getByTestId("finding-payload");
    await expect(payload).toContainText("Top");
    await expect(payload).toContainText("Uses type");
    await expect(payload).toContainText(
      "type.reference.mod uses a type priority.loader.mod defines",
    );

    await page.getByTestId("alternative-1").click();
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({
        key: "placement_questioned:priority.loader.mod:top:uses_type",
        action: { kind: "reorder", after: "priority.loader.mod", before: "type.reference.mod" },
        note: null,
      });
  });

  test("deciding a promoteRule alternative from the inbox persists the promoted copy, shown on the rules page", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "cabin.furniture.mod");

    // Alternative 2 is `PromoteRule` (see the "rule overruled" spec
    // above for the push order).
    await page.getByTestId("alternative-2").click();
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({
        key: "rule_overruled:cabin.furniture.mod:cabin.core.mod:steam_db",
        action: {
          kind: "promoteRule",
          rule: { kind: "pair", after: "cabin.furniture.mod", before: "cabin.core.mod" },
        },
        note: null,
      });

    await page.getByTestId("nav-rules").click();
    const promoted = page.getByTestId("pair-rule-cabin.furniture.mod-cabin.core.mod-userDecision");
    await expect(promoted).toBeVisible();
    // The translated origin label, not the raw wire value — see
    // `PairRuleTable.vue`'s own `ruleOriginLabel` rendering.
    await expect(promoted).toContainText("User decision");
    await expect(promoted).toContainText("promoted from SteamDB");
  });

  test("undecodable texture: card title and evidence, Accept-only", async ({ page }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "example.giants.mod");

    const card = page.locator('[data-testid^="finding-card-"]');
    await expect(card.getByTestId("card-action-text")).toHaveText(
      "example.giants.mod's giants/bodies/colossus_east won't decode",
    );

    const payload = page.getByTestId("finding-payload");
    await expect(payload).toContainText("giants/bodies/colossus_east");
    await expect(payload).toContainText("130x130");
    await expect(payload).toContainText("DXT5");

    await expect(page.getByTestId("alternatives-section")).toHaveCount(0);
    await page.getByTestId("accept-button").click();
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({
        key: "undecodable_texture:example.giants.mod:giants/bodies/colossus_east",
        action: { kind: "accept" },
        note: null,
      });
  });

  test("broken inheritance: card title, the missing parent's affected defs, Accept-only", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "colony.additions.mod");

    const card = page.locator('[data-testid^="finding-card-"]');
    await expect(card.getByTestId("card-action-text")).toHaveText(
      "colony.additions.mod's inheritance from 'ExampleCreatureBase' is broken",
    );

    const payload = page.getByTestId("finding-payload");
    await expect(payload).toContainText("ExampleCreatureBase");
    await expect(payload).toContainText("no active mod defines it");
    const affected = page.getByTestId("broken-inheritance-affected-list");
    await expect(affected).toContainText("ExampleCreature_Alpha");
    await expect(affected).toContainText("ExampleCreature_Beta");
    await expect(affected).toContainText("ExampleCreature_Gamma");

    await expect(page.getByTestId("alternatives-section")).toHaveCount(0);
    await page.getByTestId("accept-button").click();
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({
        key: "broken_inheritance:colony.additions.mod:missing:ExampleCreatureBase",
        action: { kind: "accept" },
        note: null,
      });
  });

  test("near-miss mod reference: card title, the candidate and file, Accept-only", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "drift.compat.mod");

    const card = page.locator('[data-testid^="finding-card-"]');
    await expect(card.getByTestId("card-action-text")).toHaveText(
      "drift.compat.mod's reference to 'Example.CraftingFrameworkk' may be a typo",
    );

    const payload = page.getByTestId("finding-payload");
    await expect(payload).toContainText("MayRequire id");
    await expect(payload).toContainText("Example.CraftingFrameworkk");
    await expect(payload).toContainText("Example Crafting Framework");
    await expect(payload).toContainText("RecipeDefs_Misc.xml");

    await expect(page.getByTestId("alternatives-section")).toHaveCount(0);
    await page.getByTestId("accept-button").click();
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({
        key: "near_miss_mod_reference:drift.compat.mod:mayrequire:Example.CraftingFrameworkk",
        action: { kind: "accept" },
        note: null,
      });
  });

  test("discarded addition: card title, the deliberate-override rationale, Accept-only", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "example.biomecore.mod");

    const card = page.locator('[data-testid^="finding-card-"]');
    await expect(card.getByTestId("card-action-text")).toHaveText(
      "example.biomecore.mod deliberately discards example.plantsexpanded.mod's own addition",
    );

    const payload = page.getByTestId("finding-payload");
    await expect(payload).toContainText("example.biomecore.mod");
    await expect(payload).toContainText("example.plantsexpanded.mod");
    await expect(payload).toContainText("ThingDef/ExampleShrubland/wildPlants");
    await expect(payload).toContainText("discarded on purpose");

    await expect(page.getByTestId("alternatives-section")).toHaveCount(0);
    await page.getByTestId("accept-button").click();
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({
        key: "discarded_addition:example.biomecore.mod:example.plantsexpanded.mod:ThingDef/ExampleShrubland:wildPlants",
        action: { kind: "accept" },
        note: null,
      });
  });

  test("dangling def reference: card title, the cause and referrer, Accept-only", async ({
    page,
  }) => {
    await loadScenario(page);
    // Searched by the dangling name, not a mod id — this finding's own
    // key (`dangling_def_reference:ExampleGhostResearch`) names no mod at
    // all (see `FindingKey::DanglingDefReference`'s own doc comment), so
    // `openFindingInInbox`'s key-substring search needs the name instead.
    await openFindingInInbox(page, "ExampleGhostResearch");

    const card = page.locator('[data-testid^="finding-card-"]');
    await expect(card.getByTestId("card-action-text")).toHaveText(
      "'ExampleGhostResearch' resolves to no active def",
    );

    const payload = page.getByTestId("finding-payload");
    await expect(payload).toContainText("ExampleGhostResearch");
    await expect(payload).toContainText("1.6NotOdyssey");
    await expect(payload).toContainText("example.techtree.mod");
    await expect(payload).toContainText("researchPrerequisites/li");

    await expect(page.getByTestId("alternatives-section")).toHaveCount(0);
    await page.getByTestId("accept-button").click();
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_CALLS__?.at(-1)))
      .toMatchObject({
        key: "dangling_def_reference:ExampleGhostResearch",
        action: { kind: "accept" },
        note: null,
      });
  });
});
