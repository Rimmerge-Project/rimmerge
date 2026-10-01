import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

test.describe("apply dialog", () => {
  test("happy path: writes ModsConfig.xml and shows the backup path in a toast", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("dashboard-apply-button").click();
    await expect(page.getByTestId("apply-dialog")).toBeVisible();
    await expect(page.getByTestId("apply-dialog-order-source")).toHaveText("Suggested");

    // The scenario carries unanswered hard problems (missing mods), so
    // Apply asks first; the confirmation click is below.
    // `TemperateForest` (`e2e/fixtures/scenario.ts`'s own
    // always-registered "auto-promoted patchCollision Merge" fixture —
    // the "auto Merge card" case in `clean-merge.spec.ts`) is a real,
    // complete merge candidate from page load onward, yet "Write merge
    // mod" still starts unchecked: this test is about
    // `ModsConfig.xml`/the backup toast, and asserts the payload below
    // carries `writeMergeMod: false` with no click.
    await expect(page.getByTestId("apply-dialog-write-merge-mod-checkbox")).toHaveAttribute(
      "data-p-checked",
      "false",
    );
    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();

    await expect(page.getByTestId("apply-dialog")).toBeHidden();
    await expect(page.getByText("Applied", { exact: true })).toBeVisible();
    await expect(page.getByText(/ModsConfig\.xml\.bak-/)).toBeVisible();

    const calls = await page.evaluate(() => window.__APPLY_CALLS__ ?? []);
    expect(calls).toEqual([
      { source: "suggested", writeModsConfig: true, writeMergeMod: false, force: false },
    ]);
  });

  test("an unanswered hard problem asks first; Go back writes nothing, Apply anyway writes once", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("dashboard-apply-button").click();
    await expect(page.getByTestId("apply-dialog-problems-summary")).toContainText("to confirm");
    await page.getByTestId("apply-dialog-submit").click();

    await expect(page.getByTestId("apply-confirm")).toBeVisible();
    await expect(page.getByTestId("apply-confirm-heading")).toHaveText("Apply anyway?");
    await expect(page.getByTestId("apply-confirm-go-back")).toBeFocused();
    await expect(page.getByTestId("apply-confirm-problems")).toContainText("is not installed");
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([]);

    await page.getByTestId("apply-confirm-go-back").click();
    await expect(page.getByTestId("apply-confirm")).toBeHidden();
    await expect(page.getByTestId("apply-dialog-submit")).toBeVisible();
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([]);

    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();
    await expect(page.getByTestId("apply-dialog")).toBeHidden();
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([
      { source: "suggested", writeModsConfig: true, writeMergeMod: false, force: false },
    ]);
  });

  test("game-running path: warns, then retries with force on confirmation", async ({ page }) => {
    await loadScenario(page);
    await page.evaluate(() => {
      window.__APPLY_FORCE_GAME_RUNNING__ = true;
    });

    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();
    // The hard-problem confirmation comes first; the running-game prompt is
    // a separate, later step with its own button.
    await page.getByTestId("apply-confirm-apply-anyway").click();

    await expect(page.getByTestId("apply-dialog-force-warning")).toBeVisible();
    await expect(page.getByTestId("apply-dialog")).toBeVisible();

    await page.getByTestId("apply-dialog-write-anyway").click();

    await expect(page.getByTestId("apply-dialog")).toBeHidden();
    await expect(page.getByText("Applied", { exact: true })).toBeVisible();

    const calls = await page.evaluate(() => window.__APPLY_CALLS__ ?? []);
    expect(calls).toEqual([
      { source: "suggested", writeModsConfig: true, writeMergeMod: false, force: false },
      { source: "suggested", writeModsConfig: true, writeMergeMod: false, force: true },
    ]);
  });

  test("unchecking Write ModsConfig.xml saves decisions without writing it", async ({ page }) => {
    await loadScenario(page);

    await page.getByTestId("dashboard-apply-button").click();
    const checkbox = page.getByTestId("apply-dialog-write-modsconfig-checkbox").locator("input");
    await expect(checkbox).toBeChecked();
    await page.getByTestId("apply-dialog-write-modsconfig-checkbox").click();
    await expect(page.getByTestId("apply-dialog-submit")).toHaveText("Save decisions and rules");
    // Nothing is written to ModsConfig.xml, so it skips the confirmation.
    await page.getByTestId("apply-dialog-submit").click();

    await expect(page.getByTestId("apply-dialog")).toBeHidden();
    await expect(page.getByText("Saved")).toBeVisible();

    const calls = await page.evaluate(() => window.__APPLY_CALLS__ ?? []);
    expect(calls).toEqual([
      { source: "suggested", writeModsConfig: false, writeMergeMod: false, force: false },
    ]);
  });

  test("shell's Apply button opens the same dialog from any page", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-order").click();

    await page.getByTestId("shell-apply-button").click();

    await expect(page.getByTestId("apply-dialog")).toBeVisible();
  });

  test("Verify order runs the on-demand pass and shows the predicted-failure summary", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("dashboard-apply-button").click();
    await expect(page.getByTestId("apply-dialog")).toBeVisible();

    await page.getByTestId("apply-dialog-verify-button").click();

    await expect(page.getByTestId("apply-dialog-verify-report")).toBeVisible();
    await expect(page.getByTestId("apply-dialog-verify-summary")).toContainText("Checked 42 defs");
    await expect(page.getByTestId("apply-dialog-verify-summary")).toContainText(
      "2 operations predicted to fail",
    );
    await expect(page.getByTestId("apply-dialog-verify-summary")).toContainText(
      "2 def targets affected",
    );
    await expect(page.getByTestId("apply-dialog-verify-cause-counts")).toContainText(
      "Removed by an earlier mod: 1",
    );
    await expect(page.getByTestId("apply-dialog-verify-cause-counts")).toContainText(
      "Dead target: 1",
    );
    // RemovedBy is order-fixable, listed individually.
    await expect(page.getByTestId("apply-dialog-verify-order-fixable")).toBeVisible();
    // DeadTarget is not order-fixable, grouped by mod instead.
    await expect(page.getByTestId("apply-dialog-verify-group-example.bionicsfork")).toContainText(
      "1 operation",
    );

    const calls = await page.evaluate(() => window.__VERIFY_ORDER_CALLS__ ?? []);
    expect(calls).toEqual([{ source: "suggested" }]);
  });

  // The accept half of the sort -> verify -> accept -> re-sort loop,
  // driven end to end through the UI. The `RemovedBy` fixture operation
  // carries a `reorder`; the `deadTarget` one does not, so exactly one
  // button exists — the "only order-fixable rows offer a rule" rule,
  // observable rather than asserted.
  test("creates a userDecision pair rule from an order-fixable verify row, and the rules page shows it", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-verify-button").click();
    await expect(page.getByTestId("apply-dialog-verify-report")).toBeVisible();

    const createButton = page.getByTestId(
      "apply-dialog-verify-create-pair-rule-ludeon.rimworld-example.bionicsfork",
    );
    await expect(createButton).toBeVisible();
    await expect(page.getByTestId(/^apply-dialog-verify-create-pair-rule-/)).toHaveCount(1);

    await createButton.click();

    await expect(
      page.getByTestId("apply-dialog-verify-pair-rule-created-ludeon.rimworld-example.bionicsfork"),
    ).toContainText("Rule created");
    await expect(createButton).toBeDisabled();

    // The exact payload, not just that an upsert happened — the
    // direction is computed in Rust and must reach the command
    // untouched.
    const calls = await page.evaluate(() => window.__UPSERT_RULE_CALLS__ ?? []);
    expect(calls).toEqual([
      {
        kind: "pair",
        after: "ludeon.rimworld",
        before: "example.bionicsfork",
        origin: "userDecision",
        comment:
          "Load example.bionicsfork before ludeon.rimworld, so its operation runs while the node still exists.",
        promotedFrom: null,
        alreadyPromoted: false,
        overridesDeclared: false,
      },
    ]);

    await page.getByTestId("apply-dialog-cancel").click();
    await page.getByTestId("nav-rules").click();

    await expect(
      page.getByTestId("pair-rule-ludeon.rimworld-example.bionicsfork-userDecision"),
    ).toBeVisible();
  });

  // The same order-fixable row's reorder also carries one conflict in
  // this fixture — the button relabels but is never hidden, and the
  // conflict itself is named.
  test("names a reorder's conflict and relabels the button, without hiding it", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-verify-button").click();
    await expect(page.getByTestId("apply-dialog-verify-report")).toBeVisible();

    const createButton = page.getByTestId(
      "apply-dialog-verify-create-pair-rule-ludeon.rimworld-example.bionicsfork",
    );
    await expect(createButton).toContainText("Create pair rule anyway");
    await expect(createButton).toBeEnabled();

    await expect(
      page.getByTestId("apply-dialog-verify-reorder-conflicts-ludeon.rimworld-example.bionicsfork"),
    ).toContainText(
      "conflicts with Load after: example.bionicsfork declares loadAfter ludeon.rimworld (satisfied)",
    );
  });

  // A cosmetic row offers no `set-pair` button, greys itself out, and
  // points at the existing merge instead of a reorder that would
  // change nothing.
  test("greys out a cosmetic verify row and offers no pair-rule button", async ({ page }) => {
    await loadScenario(page);
    await page.evaluate(() => {
      window.__VERIFY_ORDER_OVERRIDE__ = {
        source: "current",
        defsChecked: 1,
        operationsTotal: 1,
        defTargetsTotal: 1,
        operations: [
          {
            modId: "example.bionicsfork",
            operation: 'Verse.PatchOperationRemove(Defs/ThingDef[defName="Fixture_Wall"]/label)',
            defs: [
              {
                defKey: { defType: "ThingDef", defName: "Fixture_Wall" },
                selector: "defName",
                leafXpath: 'Defs/ThingDef[defName="Fixture_Wall"]/label',
                cause: { kind: "removedBy", modId: "ludeon.rimworld" },
                reorderKind: {
                  kind: "cosmetic",
                  existingMergeKey:
                    "patch_collision:ThingDef/Fixture_Wall:def_name:label:[example.bionicsfork,ludeon.rimworld]",
                },
                reorder: null,
              },
            ],
          },
        ],
        skipped: [],
        counterfactual: {
          jobs: 1,
          resolved: 0,
          demotedDeadTargets: 1,
          skippedTooManyMods: 0,
          skippedCoOwner: 0,
          rejectedForRegression: 0,
        },
      };
    });
    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-verify-button").click();
    await expect(page.getByTestId("apply-dialog-verify-report")).toBeVisible();

    const cosmeticRow = page.getByTestId("apply-dialog-verify-cosmetic-row");
    await expect(cosmeticRow).toBeVisible();
    await expect(cosmeticRow).toContainText("cosmetic: the same final def either order");
    await expect(cosmeticRow.getByText("Merge instead")).toBeVisible();

    // No pair-rule button anywhere — the cosmetic row's own reorder is
    // `null`, so `VerifyReorderActions` has nothing to render for it.
    await expect(page.getByTestId(/^apply-dialog-verify-create-pair-rule-/)).toHaveCount(0);
  });

  test("joins a verify pass against an imported log's own observed failures", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("dashboard-apply-button").click();

    await page.getByTestId("apply-dialog-verify-button").click();
    await expect(page.getByTestId("apply-dialog-verify-report")).toBeVisible();
    // No log imported yet — nothing to join against.
    await expect(page.getByTestId("apply-dialog-predicted-vs-observed")).toHaveCount(0);

    await page.getByTestId("apply-dialog-cancel").click();
    await page.evaluate(() => {
      window.__IMPORT_GAME_LOG_RESULT__ = {
        patchFailures: [
          // Confirms one of the two `verify_order` mock's own operations.
          {
            attribution: { kind: "mod", modId: "example.bionicsfork" },
            operation: 'Verse.PatchOperationAdd(Defs/HediffDef[defName="BionicHeart"]/comps)',
            sourceFile: null,
            stackTrace: null,
          },
          // Observed, but neither predicted nor attributable — kept, not dropped.
          {
            attribution: { kind: "unattributed", raw: "[Some Other Mod]" },
            operation: "Verse.PatchOperationAdd(Defs/SomeDef)",
            sourceFile: null,
            stackTrace: null,
          },
        ],
        extraStackTraces: [],
        crossReferences: [],
        ddsFailures: [],
        dependencyWarnings: [],
        timers: [],
        defCacheLines: [],
        loadEvents: [],
        readStats: {
          linesRead: 0,
          linesTruncated: 0,
          linesWithInvalidUtf8: 0,
          stackBlockLinesDropped: 0,
          stackJoinsAbandoned: 0,
          passesFolded: 0,
          stackRefsDropped: 0,
          loggingGapsDropped: 0,
          crashReportPathsTruncated: 0,
        },
        coverage: {
          kind: "playerLog",
          passes: 2,
          patchPhase: "noneLogged",
          endState: { kind: "cleanExit" },
        },
        loggingGaps: [],
        lowerBound: null,
      };
      window.__PICK_FILE_RESULT__ = "C:/RimWorld/Player.log";
    });
    await page.getByTestId("import-game-log-button").click();

    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-verify-button").click();

    await expect(page.getByTestId("apply-dialog-pvo-matched-summary")).toHaveText(
      "Confirmed by the log: 1",
    );
    await expect(page.getByTestId("apply-dialog-pvo-predicted-only-summary")).toHaveText(
      "Predicted, not observed in the log: 1",
    );
    await expect(page.getByTestId("apply-dialog-pvo-observed-only-entry")).toContainText(
      "[Some Other Mod]",
    );
  });

  test("warns above the predicted-vs-observed panel when the imported log's own order differs from the selected one", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-verify-button").click();
    await expect(page.getByTestId("apply-dialog-verify-report")).toBeVisible();
    await page.getByTestId("apply-dialog-cancel").click();

    await page.evaluate(() => {
      window.__IMPORT_GAME_LOG_RESULT__ = {
        patchFailures: [
          {
            attribution: { kind: "mod", modId: "example.bionicsfork" },
            operation: 'Verse.PatchOperationAdd(Defs/HediffDef[defName="BionicHeart"]/comps)',
            sourceFile: null,
            stackTrace: null,
          },
        ],
        extraStackTraces: [],
        crossReferences: [],
        ddsFailures: [],
        dependencyWarnings: [],
        timers: [],
        defCacheLines: [],
        // The real, currently selected order is the fixture's own
        // 60-mod `currentOrder` — this is deliberately a different
        // (and much shorter) sequence.
        loadEvents: [{ line: 1, kind: { type: "newGame" }, mods: ["mod.059", "mod.000"] }],
        readStats: {
          linesRead: 0,
          linesTruncated: 0,
          linesWithInvalidUtf8: 0,
          stackBlockLinesDropped: 0,
          stackJoinsAbandoned: 0,
          passesFolded: 0,
          stackRefsDropped: 0,
          loggingGapsDropped: 0,
          crashReportPathsTruncated: 0,
        },
        coverage: {
          kind: "playerLog",
          passes: 2,
          patchPhase: "noneLogged",
          endState: { kind: "cleanExit" },
        },
        loggingGaps: [],
        lowerBound: null,
      };
      window.__PICK_FILE_RESULT__ = "C:/RimWorld/Player.log";
    });
    await page.getByTestId("import-game-log-button").click();

    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-verify-button").click();

    await expect(page.getByTestId("apply-dialog-predicted-vs-observed")).toBeVisible();
    await expect(page.getByTestId("apply-dialog-pvo-order-mismatch-warning")).toContainText(
      "different mod order",
    );
  });

  test("warns that a cut-off Player.log with no patch result logged is no evidence for 'predicted, not observed'", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      window.__IMPORT_GAME_LOG_OVERRIDES__ = {
        coverage: {
          kind: "playerLog",
          passes: 1,
          patchPhase: "noneLogged",
          endState: { kind: "truncated" },
        },
      };
    });
    await loadScenario(page);
    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-verify-button").click();
    await expect(page.getByTestId("apply-dialog-verify-report")).toBeVisible();
    await page.getByTestId("apply-dialog-cancel").click();
    await page.evaluate(() => {
      window.__PICK_FILE_RESULT__ = "C:/RimWorld/Player.log";
    });
    await page.getByTestId("import-game-log-button").click();

    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-verify-button").click();

    await expect(page.getByTestId("apply-dialog-predicted-vs-observed")).toBeVisible();
    await expect(page.getByTestId("apply-dialog-pvoCutOffNote")).toContainText(
      "ended without a clean exit and before any patch result was logged",
    );
  });
});
