import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * Searches the inbox down to exactly the finding named `needle`, selects
 * it, and waits for its detail to load. Mirrors `summary.spec.ts`'s own
 * helper of the same name.
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
  test("a list merged from two mods shows each entry with its own source, alongside the label conflict", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "widget.mod.b");

    const view = page.getByTestId("def-conflict-view");
    await expect(view.getByTestId("def-conflict-view-def-ref")).toHaveText("ThingDef/Widget");

    // The mod-label-mode default is "name" — `list_mod_names` maps these
    // ids to "Widget Mod A"/"Widget Mod B", so that's what renders, not
    // the raw ids (see `labels.spec.ts` for the `t` toggle itself).
    const conflict = view.locator('[data-path="label"]');
    await expect(conflict).toContainText("Widget Mod A");
    await expect(conflict).toContainText("Widget Mod B");
    await expect(conflict.getByTestId("def-conflict-preference")).toContainText(
      "Widget Mod B wins (load order)",
    );

    const alpha = view.locator('[data-path="comps/li[@Class=CompProperties_Alpha]"]');
    const beta = view.locator('[data-path="comps/li[@Class=CompProperties_Beta]"]');
    await expect(alpha).toContainText("Widget Mod A");
    await expect(beta).toContainText("Widget Mod B");

    // Both entries land under one "comps" group, in the final list's own
    // order (alpha before beta) — not load order, not path order (see
    // `rim_session`'s own position-based `ListEntry` sort).
    const rows = view.getByTestId("def-conflict-field-row");
    const alphaIndex = await alpha.evaluate((el) =>
      Array.from(el.parentElement?.children ?? []).indexOf(el),
    );
    const betaIndex = await beta.evaluate((el) =>
      Array.from(el.parentElement?.children ?? []).indexOf(el),
    );
    expect(alphaIndex).toBeLessThan(betaIndex);
    await expect(rows).toHaveCount(3);
  });

  test("a stored merge choice's after-merge value differs from what's in-game today", async ({
    page,
  }) => {
    // Pins a real, distinguishing
    // `inGame`/`afterMerge` pair end to end (a component bug swapping the
    // two chip bindings would pass every other spec here, since they all
    // happen to have the two equal). Before any choice is stored, `label`
    // is unresolved (`afterMerge` is `null` — `NeedsFieldInput`, the mock
    // mirrors `rim_resolve::domain::merge_status`); picking `widget.mod.a`
    // in the merge editor — *not* the load-order winner `widget.mod.b` —
    // resolves it, so `afterMerge` names `widget.mod.a` while `inGame`
    // keeps naming `widget.mod.b`, the real winner today.
    await loadScenario(page);
    await openFindingInInbox(page, "widget.mod.b");
    // Resolving this collision's one conflict field below also *decides*
    // it (`set_merge_choices` records a `Merge` decision — the same
    // reason `merge.spec.ts`'s own Esc test deliberately uses a finding
    // with a conflict left over, not `BionicHeart`): switch to the "all
    // statuses" chip first so the now-decided finding stays visible and
    // selected after Escape returns, instead of dropping out of the
    // default `needsInput` filter mid-test.
    await page.getByTestId("status-chip-all").click();

    const label = page.getByTestId("def-conflict-view").locator('[data-path="label"]');
    await expect(label.getByTestId("def-conflict-in-game")).toContainText("Widget Mod B");
    await expect(label.getByTestId("def-conflict-after-merge")).toHaveCount(0);

    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);
    // Wait for the row cursor's own target to actually be on screen
    // before sending a digit — pressing it too early (before the field
    // page loads) would land on nothing.
    await expect(page.getByTestId("merge-field-row").first()).toBeVisible();
    // Patch-collision owners columns are the colliding mods only (no base
    // column) — `widget.mod.a` is position 0, so digit "1" picks it.
    await page.keyboard.press("1");
    await expect(page.getByTestId("merge-header-state")).toHaveText("merged");

    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(/\/inbox$/);

    const refreshed = page.getByTestId("def-conflict-view").locator('[data-path="label"]');
    await expect(refreshed.getByTestId("def-conflict-in-game")).toContainText("Widget Mod B");
    await expect(refreshed.getByTestId("def-conflict-after-merge")).toContainText("Widget Mod A");
  });

  test("the kind filter narrows the fields table to just the list entries", async ({ page }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "widget.mod.b");

    const view = page.getByTestId("def-conflict-view");
    await view.getByTestId("def-conflict-view-filter").selectOption("listEntry");

    const rows = view.getByTestId("def-conflict-field-row");
    await expect(rows).toHaveCount(2);
    await expect(view.locator('[data-path="label"]')).toHaveCount(0);
  });

  test("an unsupported op stops the replay: earlier rows survive, the problem names the op, and the later toucher is not reached", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "wall.d.mod");

    const view = page.getByTestId("def-conflict-view");
    await expect(view.getByTestId("def-conflict-view-completeness")).toHaveText("Partial");

    // "Wall C Mod"/"Wall D Mod" are mapped display names (both are real
    // `patchCollision.mods` owners); `wall.x.mod` — the stopper — isn't a
    // party to this collision and so has no mapped name, rendering as its
    // raw id.
    const description = view.locator('[data-path="description"]');
    await expect(description).toContainText("Wall C Mod");
    await expect(description.getByTestId("def-conflict-in-game")).toContainText("Wall C Mod");

    const problem = view.getByTestId("def-conflict-problem-row");
    await expect(problem).toContainText("wall.x.mod");
    await expect(problem).toContainText("SomeThirdParty.WeirdOperation");

    const dModRow = view.locator('[data-mod-id="wall.d.mod"]');
    await expect(dModRow.getByTestId("def-conflict-toucher-reached")).toHaveText("not reached");
    const cModRow = view.locator('[data-mod-id="wall.c.mod"]');
    await expect(cModRow.getByTestId("def-conflict-toucher-reached")).toHaveText("reached");
  });

  test("two mods adding the exact same list item collapse into one agreed-on entry", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "dup.mod.b");

    const view = page.getByTestId("def-conflict-view");
    await expect(view.getByTestId("def-conflict-view-def-ref")).toHaveText("ThingDef/Sprocket");

    const row = view.locator('[data-path="comps/li[#0]"]');
    await expect(row).toHaveCount(1);
    await expect(row).toContainText("Dup Mod A");
    await expect(row.getByTestId("def-conflict-agreed-by")).toContainText("Dup Mod B");

    // Never a second, indistinguishable-looking row for the agreeing mod.
    await expect(view.getByTestId("def-conflict-field-row")).toHaveCount(1);
  });

  test("a tag-keyed map's own uncontested keys group under one container header, its agreed-on key names both contributors, and its real conflicts stay ungrouped", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "wildlife.mod.c");

    const view = page.getByTestId("def-conflict-view");
    await expect(view.getByTestId("def-conflict-view-def-ref")).toHaveText(
      "BiomeDef/AridShrubland",
    );

    // Six changed rows by default (`onlyChanged: true`): the two real
    // conflicts (`Raptor`, `Boar`) plus the four `wildAnimals` map entries
    // that already auto-resolved — the four `Unchanged` animals stay
    // hidden. The four map entries join `FieldRowsTable`'s grouping, so
    // they render as grouped rows.
    await expect(view.getByTestId("def-conflict-field-row")).toHaveCount(6);

    const groupHeader = view.getByTestId("def-conflict-group-header");
    await expect(groupHeader).toContainText("wildAnimals");
    await expect(groupHeader).toContainText("4 entries");

    const cobraRow = view.locator('[data-path="wildAnimals/Cobra"]');
    await expect(cobraRow).toContainText("Wildlife Mod A");
    await expect(cobraRow.getByTestId("def-conflict-agreed-by")).toContainText("Wildlife Mod B");

    const raptorRow = view.locator('[data-path="wildAnimals/Raptor"]');
    await expect(raptorRow.getByTestId("def-conflict-preference")).toContainText(
      "Wildlife Mod C wins (load order)",
    );

    await view.getByTestId("def-conflict-view-filter").selectOption("mapEntry");
    const mapRows = view.getByTestId("def-conflict-field-row");
    await expect(mapRows).toHaveCount(4);
    await expect(view.locator('[data-path="wildAnimals/Raptor"]')).toHaveCount(0);
  });

  test("shows a finding's fields again after switching away and back within the cache's staleTime", async ({
    page,
  }) => {
    // `SuggestionPanel` reuses
    // this view across findings instead of remounting it, so switching
    // Widget → Sprocket → Widget within Pinia Colada's default `staleTime`
    // must not leave the view stuck on "No fields match this filter" once
    // back on Widget — `usePagedRows`'s reset watcher must not wipe the page
    // its own `data` watcher just landed from cache, with nothing left to
    // refetch and refill it.
    await loadScenario(page);
    await openFindingInInbox(page, "widget.mod.b");

    const view = page.getByTestId("def-conflict-view");
    await expect(view.getByTestId("def-conflict-view-def-ref")).toHaveText("ThingDef/Widget");
    await expect(view.getByTestId("def-conflict-field-row").first()).toBeVisible();

    await openFindingInInbox(page, "dup.mod.b");
    await expect(view.getByTestId("def-conflict-view-def-ref")).toHaveText("ThingDef/Sprocket");

    await openFindingInInbox(page, "widget.mod.b");
    await expect(view.getByTestId("def-conflict-view-def-ref")).toHaveText("ThingDef/Widget");
    await expect(view.getByTestId("def-conflict-field-row").first()).toBeVisible();
    await expect(view.getByTestId("def-conflict-fields-empty")).toHaveCount(0);
  });

  test("the def-ref link navigates to the def page", async ({ page }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "widget.mod.b");

    await page.getByTestId("def-conflict-view-def-ref").click();
    await expect(page).toHaveURL(/\/defs\//);
    await expect(page.getByTestId("def-page-ref")).toHaveText("ThingDef/Widget");
  });
});
