import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

test.describe("rules", () => {
  test("the import dialog prefills from the default RimSort paths", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-rules").click();
    await expect(page).toHaveURL(/\/rules$/);

    await page.getByTestId("open-import-dialog").click();
    await expect(page.getByTestId("import-rimsort-dialog")).toBeVisible();
    await expect(page.getByTestId("import-user-rules-path")).toHaveValue(
      "C:/RimSort/dbs/userRules.json",
    );
    await expect(page.getByTestId("import-community-rules-path")).toHaveValue(
      "C:/RimSort/dbs/Community-Rules-Database/communityRules.json",
    );

    await page.getByTestId("import-rimsort-submit").click();
    await expect(page.getByTestId("import-rimsort-dialog")).not.toBeVisible();

    // Confirms the submitted paths are exactly what was shown, not just
    // that *some* import call happened.
    const calls = await page.evaluate(() => window.__IMPORT_RIMSORT_CALLS__ ?? []);
    expect(calls).toEqual([
      {
        userRules: "C:/RimSort/dbs/userRules.json",
        communityRules: "C:/RimSort/dbs/Community-Rules-Database/communityRules.json",
        steamDb: "C:/RimSort/dbs/Steam-Workshop-Database/steamDB.json",
      },
    ]);
  });

  // Pair rules render through the virtualized `PairRuleTable`; this pins
  // both that the scenario's pair row is actually mounted and that its
  // Delete button still emits the exact key *and* the row's own origin
  // (`DeleteRuleRequestDto.origin`, which the origin-aware `DeleteRule`
  // needs).
  test("deletes a pair rule", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-rules").click();

    const row = page.getByTestId("pair-rule-mod.003-mod.004-rimSortUser");
    await expect(row).toBeVisible();
    await row.getByRole("button", { name: "Delete" }).click();
    await expect(row).not.toBeVisible();

    const deletes = await page.evaluate(() => window.__DELETE_RULE_CALLS__ ?? []);
    expect(deletes).toEqual([
      { key: { kind: "pair", after: "mod.003", before: "mod.004" }, origin: "rimSortUser" },
    ]);
  });

  // The scenario's own default settings turn `useImportedPairs` off and
  // `useImportedPlacements` on — this pins that the rules page reflects
  // both toggles' state (greying the imported pair row, leaving the
  // imported placement row un-greyed) rather than treating every imported
  // row the same way.
  test("greys the imported pair row while its toggle is off, and offers to promote it", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-rules").click();

    await expect(page.getByTestId("rules-import-toggle-state")).toHaveText(
      "Imported pairs: off · Imported placements: on",
    );

    // Greying is scoped to the data cells,
    // not the row (which would also fade the Promote/Delete buttons below
    // WCAG's 4.5:1 for interactive elements) — the row itself carries a
    // `title` explaining why instead.
    const pairRow = page.getByTestId("pair-rule-mod.003-mod.004-rimSortUser");
    await expect(pairRow).toHaveAttribute("title", "Not in effect: imported pairs are off");
    await expect(pairRow.locator('[role="cell"]').first()).toHaveClass(/opacity-50/);
    const placementRow = page.getByTestId("placement-rule-mod.000-rimSortCommunity");
    await expect(placementRow).not.toHaveAttribute("title");

    await page.getByTestId("pair-rule-mod.003-mod.004-rimSortUser-promote").click();

    const promotes = await page.evaluate(() => window.__PROMOTE_RULE_CALLS__ ?? []);
    expect(promotes).toEqual([{ kind: "pair", after: "mod.003", before: "mod.004" }]);

    // The promoted copy appears as its own row, keyed by its own
    // `userDecision` origin (it shares the imported row's `after`/
    // `before`, so origin is what disambiguates them), and names its
    // imported source.
    await expect(
      page.getByTestId("pair-rule-mod.003-mod.004-userDecision-promoted-from"),
    ).toBeVisible();
    // The imported row's action now reads "Promoted" and is disabled —
    // promoting again would only duplicate the copy.
    await expect(page.getByTestId("pair-rule-mod.003-mod.004-rimSortUser-promote")).toBeDisabled();
  });

  test("adds and removes a manual tag", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-rules").click();

    await page.getByTestId("manual-tag-mod-id").fill("mod.020");
    await page.getByTestId("manual-tag-tag").fill("custom");
    await page.getByTestId("manual-tag-submit").click();

    await expect(page.getByTestId("tag-assignment-mod.020-custom")).toBeVisible();

    // The mode select defaults back to "Add" on every submit (the form
    // never resets it) — switch it to "Remove" before resubmitting the
    // same mod/tag pair to actually exercise the remove path, not just
    // add it a second time.
    await page.getByTestId("manual-tag-mod-id").fill("mod.020");
    await page.getByTestId("manual-tag-tag").fill("custom");
    await page.getByTestId("manual-tag-mode").click();
    await page.getByText("Remove", { exact: true }).click();
    await page.getByTestId("manual-tag-submit").click();

    await expect(page.getByTestId("tag-assignment-mod.020-custom")).not.toBeVisible();

    const calls = await page.evaluate(() => window.__SET_MANUAL_TAG_CALLS__ ?? []);
    expect(calls).toEqual([
      { modId: "mod.020", tag: "custom", mode: "add" },
      { modId: "mod.020", tag: "custom", mode: "remove" },
    ]);
  });

  // The rules page can create a placement rule, not only
  // view/promote/delete existing ones. `mod.006` carries no pin of
  // its own anywhere else in the fixture, so this is a genuine "pin a
  // mod that was never imported" flow, matching `rule set-placement`'s
  // own real motivating case, not an edit of something already pinned.
  test("adds a placement rule and shows it in the table", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-rules").click();

    await page.getByTestId("open-add-rule-dialog").click();
    await expect(page.getByTestId("add-rule-dialog")).toBeVisible();

    await page.getByTestId("add-rule-mod-search").fill("Mod 6");
    await expect(page.getByTestId("add-rule-mod-add-mod.006")).toBeVisible();
    await page.getByTestId("add-rule-mod-add-mod.006").click();

    await page.getByTestId("add-rule-placement").click();
    // Scoped to the open dropdown's own listbox — `RuleTable.vue`'s
    // placement column now renders the identical, translated "Bottom"
    // label too, so a page-wide `getByText` is ambiguous once the table
    // already has rows.
    await page.getByRole("option", { name: "Bottom", exact: true }).click();
    await page.getByTestId("add-rule-comment").fill("keep it last");

    await page.getByTestId("add-rule-submit").click();
    await expect(page.getByTestId("add-rule-dialog")).not.toBeVisible();

    // Confirms the exact IPC payload, not just that some upsert happened
    // — `origin` must be `userDecision` regardless of what the picker's
    // own mod list otherwise carries, matching `rule set-placement`'s
    // own hard-coded origin exactly.
    const calls = await page.evaluate(() => window.__UPSERT_RULE_CALLS__ ?? []);
    expect(calls).toEqual([
      {
        kind: "placement",
        modId: "mod.006",
        placement: "bottom",
        origin: "userDecision",
        comment: "keep it last",
        promotedFrom: null,
        alreadyPromoted: false,
      },
    ]);

    await expect(page.getByTestId("placement-rule-mod.006-userDecision")).toBeVisible();
  });

  // The rule-level promotes-dependents companion:
  // `crowded.framework.mod`'s own `Bottom` pin promotes two other mods
  // past its own tier boundary (the dedicated
  // `placement_promotes_dependents` fixture in `scenario.ts`) — the
  // signal the user has no way to see anywhere else. `basement.optimizer.mod`'s
  // own `Bottom` pin, in the same table, carries no such finding and must
  // show nothing.
  test("shows the promoted-dependent count on a pin that has one, and nothing on one that doesn't", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-rules").click();

    await expect(
      page.getByTestId("placement-rule-crowded.framework.mod-userDecision-promotes"),
    ).toHaveText("2 other mods");
    await expect(
      page.getByTestId("placement-rule-basement.optimizer.mod-rimSortCommunity-promotes"),
    ).not.toBeAttached();
  });

  // The Databases card's own
  // refresh flow end to end, including a failing source — the "never
  // silently stale" requirement means a failed refresh must leave the
  // cached copy's own facts on screen untouched, not blank them out.
  test("refreshing updates an enabled source, leaves a failing one's cached copy in use, and skips a disabled one", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-rules").click();

    // Community starts already cached (the scenario's own default fixture).
    await expect(page.getByTestId("rule-database-status-community")).toContainText("b00fa1529656");

    // Steam is on by default (every source is recommended); turn it off so
    // this test still covers a disabled source being skipped.
    const steamToggle = page.getByTestId("rule-database-toggle-steam").locator("input");
    await expect(steamToggle).toBeChecked();
    await steamToggle.uncheck({ force: true });
    await expect(steamToggle).not.toBeChecked();

    await page.evaluate(() => {
      window.__RULE_DATABASE_REFRESH_FORCE_FAIL__ = true;
    });
    await page.getByTestId("rule-databases-refresh").click();

    await expect(page.getByTestId("rule-database-refresh-result-community")).toContainText(
      "failed: Couldn't reach GitHub",
    );
    await expect(page.getByTestId("rule-database-refresh-result-detail-community")).toHaveText(
      "Technical details: connection timed out",
    );
    await expect(page.getByTestId("rule-database-refresh-result-steam")).toContainText(
      "skipped (source disabled)",
    );
    // The third source is a full row on this
    // card — enabled by default, refreshed by the same button, and its
    // own result reported alongside the other two.
    await expect(page.getByTestId("rule-database-row-rimmerge")).toContainText("Rimmerge rules");
    await expect(page.getByTestId("rule-database-refresh-result-rimmerge")).toContainText(
      "updated",
    );
    // The cached copy is still in use: the same bytes/sha from before the
    // failed refresh, plus the "still in use" banner — never blanked out.
    await expect(page.getByTestId("rule-database-status-community")).toContainText("b00fa1529656");
    await expect(page.getByTestId("rule-database-status-community")).toContainText(
      "Last refresh failed: Couldn't reach GitHub",
    );
    await expect(page.getByTestId("rule-database-failure-detail-community")).toHaveText(
      "Technical details: connection timed out",
    );
    await expect(page.getByTestId("rule-databases-failure-banner")).toContainText("still in use");
  });

  test("a successful refresh updates the cached facts and clears a previous failure", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-rules").click();

    // Create a real failure first — asserting "clears a previous failure"
    // against a fixture that starts with `lastFailure: null` would pass
    // whether or not clearing actually worked.
    await page.evaluate(() => {
      window.__RULE_DATABASE_REFRESH_FORCE_FAIL__ = true;
    });
    await page.getByTestId("rule-databases-refresh").click();
    await expect(page.getByTestId("rule-databases-failure-banner")).toBeVisible();
    await expect(page.getByTestId("rule-database-status-community")).toContainText(
      "Last refresh failed",
    );

    await page.evaluate(() => {
      window.__RULE_DATABASE_REFRESH_FORCE_FAIL__ = false;
    });
    await page.getByTestId("rule-databases-refresh").click();

    await expect(page.getByTestId("rule-database-refresh-result-community")).toContainText(
      "updated",
    );
    await expect(page.getByTestId("rule-database-status-community")).not.toContainText(
      "b00fa1529656",
    );
    await expect(page.getByTestId("rule-database-status-community")).not.toContainText(
      "Last refresh failed",
    );
    await expect(page.getByTestId("rule-databases-failure-banner")).not.toBeAttached();
  });

  // The re-import badge's other half: it must actually
  // *appear* once a refresh produces new bytes — every other spec in
  // this file only exercises the refresh flow itself or the badge's
  // absence, never the moment it turns on.
  test("the re-import badge appears next to the import button once a refresh finds new bytes", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-rules").click();

    await expect(page.getByTestId("rule-databases-reimport-hint")).not.toBeAttached();

    await page.getByTestId("rule-databases-refresh").click();
    await expect(page.getByTestId("rule-database-refresh-result-community")).toContainText(
      "updated",
    );

    await expect(page.getByTestId("rule-databases-reimport-hint")).toBeVisible();
    await expect(page.getByTestId("rule-databases-reimport-hint")).toContainText(
      "Cached databases have changed since this profile last imported them",
    );
    // Badge only — no dialog opens on its own.
    await expect(page.locator('[role="dialog"]')).not.toBeVisible();
  });

  test("network refresh off disables the Refresh button and keeps per-source toggles and cached status visible", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();
    // Saves immediately — this toggle lives in its own app-global Internet access
    // section, not the profile-settings form the page's own Save button
    // submits.
    await page.getByTestId("settings-allow-network").click();

    await page.getByTestId("nav-rules").click();

    await expect(page.getByTestId("rule-databases-refresh")).toBeDisabled();
    await expect(page.getByTestId("rule-databases-network-disabled-notice")).toContainText(
      "Internet access is off, so refresh is disabled",
    );
    await expect(page.getByTestId("rule-database-toggle-community")).toBeEnabled();
    await expect(page.getByTestId("rule-database-status-community")).toContainText("b00fa1529656");
  });
});
