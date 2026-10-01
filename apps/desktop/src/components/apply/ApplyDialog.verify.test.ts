// ApplyDialog: the Verify order section.

import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import {
  dashboardFixture,
  EMPTY_PENDING_ACTIVE_CHANGES,
  EMPTY_RULE_SET,
  emptyMergeModFixture,
  flush,
  mountDialog,
  preflightFixture,
} from "@/components/apply/ApplyDialog.test-support";
import { installMockIpc } from "@/services/ipc.mock";
import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import type { RuleSetDto } from "@/types/generated/RuleSetDto";
import type { VerifyOperationDto } from "@/types/generated/VerifyOperationDto";
import type { VerifyReorderDto } from "@/types/generated/VerifyReorderDto";
import type { VerifyReportDto } from "@/types/generated/VerifyReportDto";
import type { VerifyRequestDto } from "@/types/generated/VerifyRequestDto";
import {
  gameLogSummary,
  playerLogCoverage,
  snapshotCoverage,
} from "@/utils/gameLogSummary.test-support";

describe("ApplyDialog", () => {
  afterEach(() => {
    clearMocks();
  });

  describe("Verify order", () => {
    function verifyOperation(overrides: Partial<VerifyOperationDto> = {}): VerifyOperationDto {
      return {
        modId: "x.mod",
        operation: 'Verse.PatchOperationAdd(Defs/ThingDef[defName="Wall"]/comps)',
        defs: [
          {
            defKey: { defType: "ThingDef", defName: "Wall" },
            selector: "defName",
            leafXpath: null,
            cause: { kind: "deadTarget" },
            reorderKind: null,
            reorder: null,
          },
        ],
        ...overrides,
      };
    }

    function verifyReportFixture(operations: VerifyOperationDto[]): VerifyReportDto {
      return {
        source: "current",
        defsChecked: 12,
        operationsTotal: operations.length,
        defTargetsTotal: operations.reduce((sum, operation) => sum + operation.defs.length, 0),
        operations,
        skipped: [],
        counterfactual: {
          jobs: 0,
          resolved: 0,
          demotedDeadTargets: 0,
          skippedTooManyMods: 0,
          skippedCoOwner: 0,
          rejectedForRegression: 0,
        },
      };
    }

    it("sends the selected source and renders the summary line on success", async () => {
      const calls: VerifyRequestDto[] = [];
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        verify_order: (payload: unknown) => {
          calls.push((payload as { request: VerifyRequestDto }).request);
          return verifyReportFixture([verifyOperation()]);
        },
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      expect(calls).toEqual([{ source: "current" }]);
      const summary = wrapper.get('[data-testid="apply-dialog-verify-summary"]').text();
      expect(summary).toContain("Checked 12 defs");
      expect(summary).toContain("1 operation predicted to fail");
      expect(summary).toContain("1 def target affected");
    });

    it("renders the counterfactual summary when the phase did something, and nothing when it did not", async () => {
      // The counterfactual summary line. The distinction it exists for:
      // "the experiment ran and found nothing" must not read the same as
      // "the experiment never ran" — few order-fixable rows means very
      // different things in those two cases.
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        verify_order: () => ({
          ...verifyReportFixture([verifyOperation()]),
          counterfactual: {
            jobs: 7,
            resolved: 2,
            demotedDeadTargets: 3,
            skippedTooManyMods: 1,
            skippedCoOwner: 4,
            rejectedForRegression: 5,
          },
        }),
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      const line = wrapper.get('[data-testid="apply-dialog-verify-counterfactual"]').text();
      expect(line).toContain("7 operations re-checked against other load orders");
      expect(line).toContain("2 failures explained");
      expect(line).toContain('3 "dead target" rows turned out to be order-fixable');
      expect(line).toContain("5 operations skipped");
      expect(line).toContain("5 reorders rejected");
    });

    it("hides the counterfactual line entirely when the phase did nothing", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        verify_order: () => verifyReportFixture([verifyOperation()]),
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      expect(wrapper.find('[data-testid="apply-dialog-verify-report"]').exists()).toBe(true);
      expect(wrapper.find('[data-testid="apply-dialog-verify-counterfactual"]').exists()).toBe(
        false,
      );
    });

    async function runVerify(report: VerifyReportDto) {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        verify_order: () => report,
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();
      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();
      return wrapper;
    }

    it("words the summary in the singular for one def checked and the plural for no failures", async () => {
      const wrapper = await runVerify({ ...verifyReportFixture([]), defsChecked: 1 });

      expect(wrapper.get('[data-testid="apply-dialog-verify-summary"]').text()).toBe(
        "Checked 1 def against the current order — 0 operations predicted to fail and 0 def targets affected.",
      );
    });

    it("counts the candidates it could not check in a sentence of their own, singular and plural", async () => {
      const skipped = (defName: string) => ({
        defKey: { defType: "ThingDef", defName },
        selector: "defName" as const,
        reason: "too many mods",
      });
      const one = await runVerify({ ...verifyReportFixture([]), skipped: [skipped("A")] });
      expect(one.get('[data-testid="apply-dialog-verify-skipped"]').text()).toBe(
        "1 candidate couldn't be checked.",
      );
      clearMocks();
      const two = await runVerify({
        ...verifyReportFixture([]),
        skipped: [skipped("A"), skipped("B")],
      });
      expect(two.get('[data-testid="apply-dialog-verify-skipped"]').text()).toBe(
        "2 candidates couldn't be checked.",
      );
    });

    it("renders a cosmetic row's merge-instead link inside one whole sentence", async () => {
      const wrapper = await runVerify(
        verifyReportFixture([
          verifyOperation({
            defs: [
              {
                defKey: { defType: "ThingDef", defName: "Wall" },
                selector: "defName",
                leafXpath: null,
                cause: { kind: "removedBy", modId: "other.mod" },
                reorderKind: {
                  kind: "cosmetic",
                  existingMergeKey: "patch_collision:ThingDef/Wall",
                },
                reorder: null,
              },
            ],
          }),
        ]),
      );

      const row = wrapper.get('[data-testid="apply-dialog-verify-cosmetic-row"]');
      expect(row.text()).toContain("Merge instead keeps the losing mod's own intent.");
      expect(row.get("a").text()).toBe("Merge instead");
    });

    it("collapses multiple def targets under one operation into a single header, listing every def underneath", async () => {
      // `VerifyOrder`'s own per-def architecture reports
      // one operation matching several defs (a common `OR`-list head) as
      // one `Finding` per def — the backend groups these into one
      // `VerifyOperationDto` with several `defs`, and this component must
      // render exactly one header for it, not one per def.
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        verify_order: () =>
          verifyReportFixture([
            verifyOperation({
              modId: "example.sequencewrapper",
              operation: "Example.PatchOperationToggableSequence(count=1, lastFailedOperation=...)",
              defs: [
                {
                  defKey: { defType: "GeneDef", defName: "Learning_Fast" },
                  selector: "defName",
                  leafXpath: null,
                  cause: { kind: "deadTarget" },
                  reorderKind: null,
                  reorder: null,
                },
                {
                  defKey: {
                    defType: "ExampleVehicles.VehicleTurretDef",
                    defName: "RT_ExampleTurret",
                  },
                  selector: "defName",
                  leafXpath: null,
                  cause: { kind: "unknown" },
                  reorderKind: null,
                  reorder: null,
                },
              ],
            }),
          ]),
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      const summary = wrapper.get('[data-testid="apply-dialog-verify-summary"]').text();
      expect(summary).toContain("1 operation predicted to fail");
      expect(summary).toContain("2 def targets affected");
      expect(
        wrapper.findAll('[data-testid="apply-dialog-verify-group-example.sequencewrapper"]').length,
      ).toBe(1);
      const group = wrapper.get(
        '[data-testid="apply-dialog-verify-group-example.sequencewrapper"]',
      );
      expect(group.text()).toContain("1 operation and 2 def targets");
    });

    it("lists RemovedBy/NotYetInjected operations individually as order-fixable", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        verify_order: () =>
          verifyReportFixture([
            verifyOperation({
              modId: "removed.mod",
              defs: [
                {
                  defKey: { defType: "ThingDef", defName: "Wall" },
                  selector: "defName",
                  leafXpath: null,
                  cause: { kind: "removedBy", modId: "earlier.mod" },
                  reorderKind: null,
                  reorder: null,
                },
              ],
            }),
            verifyOperation({
              modId: "injected.mod",
              defs: [
                {
                  defKey: { defType: "ThingDef", defName: "Door" },
                  selector: "defName",
                  leafXpath: null,
                  cause: { kind: "notYetInjected", modId: "later.mod" },
                  reorderKind: null,
                  reorder: null,
                },
              ],
            }),
          ]),
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      const orderFixable = wrapper.get('[data-testid="apply-dialog-verify-order-fixable"]');
      expect(orderFixable.text()).toContain("removed.mod");
      expect(orderFixable.text()).toContain("earlier.mod");
      expect(orderFixable.text()).toContain("injected.mod");
      expect(orderFixable.text()).toContain("later.mod");
      const causeCounts = wrapper.get('[data-testid="apply-dialog-verify-cause-counts"]');
      expect(causeCounts.text()).toContain("Removed by an earlier mod: 1");
      expect(causeCounts.text()).toContain("Not yet injected: 1");
      expect(wrapper.find('[data-testid="apply-dialog-verify-grouped"]').exists()).toBe(false);
    });

    describe("Create pair rule", () => {
      /**
       * An order-fixable operation whose def target carries the reorder
       * the backend computed for it. `reorder` is deliberately *not*
       * derived from `cause` here, matching the real DTO: the direction
       * comes from `rim_resolve::ledger::suggest`, and a fixture that
       * re-derived it would test this component against the very
       * inversion the field exists to prevent.
       */
      function orderFixableOperation(
        modId: string,
        defName: string,
        cause: { kind: "removedBy" | "notYetInjected"; modId: string },
        reorder: VerifyReorderDto | null,
      ): VerifyOperationDto {
        return verifyOperation({
          modId,
          operation: `Verse.PatchOperationAdd(Defs/ThingDef[defName="${defName}"]/comps)`,
          defs: [
            {
              defKey: { defType: "ThingDef", defName },
              selector: "defName",
              leafXpath: null,
              cause,
              // Mirrors the real backend's own pairing: a row with no
              // reorder at all carries no `reorderKind` either — this
              // factory's own `reorder: null` callers are testing "no
              // reorder was computed" edge cases, not the cosmetic
              // classification specifically.
              reorderKind: reorder ? { kind: "content" as const } : null,
              reorder,
            },
          ],
        });
      }

      const REMOVED_BY_REORDER: VerifyReorderDto = {
        after: "earlier.mod",
        before: "removed.mod",
        rationale:
          "Load removed.mod before earlier.mod, so its operation runs while the node still exists.",
        rationaleCode: {
          kind: "reorderBeforeRemover",
          modId: "removed.mod",
          remover: "earlier.mod",
        },
        conflicts: [],
      };

      /**
       * A reorder that
       * contradicts something already in the report — the button must
       * still render (never hidden), relabelled, and the conflict named.
       */
      const CONFLICTING_REORDER: VerifyReorderDto = {
        after: "earlier.mod",
        before: "removed.mod",
        rationale: "Load removed.mod before earlier.mod.",
        rationaleCode: {
          kind: "reorderBeforeRemover",
          modId: "removed.mod",
          remover: "earlier.mod",
        },
        conflicts: [
          {
            kind: "loadAfter",
            detail: "removed.mod declares loadAfter earlier.mod",
            status: "satisfied",
            direction: "reverses",
          },
        ],
      };

      it("renders a button only for order-fixable rows that carry a reorder", async () => {
        installMockIpc({
          list_rules: EMPTY_RULE_SET,
          list_mod_names: {},
          get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
          get_dashboard: dashboardFixture(0),
          get_apply_preflight: preflightFixture(),
          get_merge_mod: emptyMergeModFixture(),
          get_def_cache_carrier: { carrierModId: null },
          list_order: [],
          verify_order: () =>
            verifyReportFixture([
              orderFixableOperation(
                "removed.mod",
                "Wall",
                { kind: "removedBy", modId: "earlier.mod" },
                REMOVED_BY_REORDER,
              ),
              // Same section, no reorder computed for it — the row is
              // still listed, but offers no rule to create.
              orderFixableOperation(
                "injected.mod",
                "Door",
                { kind: "notYetInjected", modId: "later.mod" },
                null,
              ),
            ]),
        });
        const { wrapper } = mountDialog();
        await flush();
        await wrapper.vm.$nextTick();

        await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
        await flush();
        await wrapper.vm.$nextTick();

        expect(
          wrapper.findAll('[data-testid^="apply-dialog-verify-create-pair-rule-"]').length,
        ).toBe(1);
        // Exact text, not `toContain` — "Create pair rule anyway" also
        // contains the substring "Create pair rule", so a `toContain`
        // assertion here would pass even if this unconflicted row were
        // wrongly relabelled.
        expect(
          wrapper
            .get('[data-testid="apply-dialog-verify-create-pair-rule-earlier.mod-removed.mod"]')
            .text(),
        ).toBe("Create pair rule");
      });

      it("relabels the button and names the conflict when the reorder contradicts the report", async () => {
        installMockIpc({
          list_rules: EMPTY_RULE_SET,
          list_mod_names: {},
          get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
          get_dashboard: dashboardFixture(0),
          get_apply_preflight: preflightFixture(),
          get_merge_mod: emptyMergeModFixture(),
          get_def_cache_carrier: { carrierModId: null },
          list_order: [],
          verify_order: () =>
            verifyReportFixture([
              orderFixableOperation(
                "removed.mod",
                "Wall",
                { kind: "removedBy", modId: "earlier.mod" },
                CONFLICTING_REORDER,
              ),
            ]),
        });
        const { wrapper } = mountDialog();
        await flush();
        await wrapper.vm.$nextTick();

        await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
        await flush();
        await wrapper.vm.$nextTick();

        // Never hidden — the user decides.
        const button = wrapper.get(
          '[data-testid="apply-dialog-verify-create-pair-rule-earlier.mod-removed.mod"]',
        );
        expect(button.text()).toContain("Create pair rule anyway");
        expect(button.attributes("disabled")).toBeUndefined();
        expect(
          wrapper
            .get('[data-testid="apply-dialog-verify-reorder-conflicts-earlier.mod-removed.mod"]')
            .text(),
        ).toContain(
          "conflicts with Load after: removed.mod declares loadAfter earlier.mod (satisfied)",
        );
      });

      it("sends the exact userDecision pair rule the backend computed, comment included", async () => {
        const upserts: unknown[] = [];
        installMockIpc({
          list_rules: EMPTY_RULE_SET,
          list_mod_names: {},
          get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
          get_dashboard: dashboardFixture(0),
          get_apply_preflight: preflightFixture(),
          get_merge_mod: emptyMergeModFixture(),
          get_def_cache_carrier: { carrierModId: null },
          list_order: [],
          verify_order: () =>
            verifyReportFixture([
              orderFixableOperation(
                "removed.mod",
                "Wall",
                { kind: "removedBy", modId: "earlier.mod" },
                REMOVED_BY_REORDER,
              ),
            ]),
          upsert_rule: (payload: unknown) => {
            upserts.push((payload as { rule: unknown }).rule);
            return null;
          },
        });
        const { wrapper } = mountDialog();
        await flush();
        await wrapper.vm.$nextTick();

        await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
        await flush();
        await wrapper.vm.$nextTick();

        await wrapper
          .get('[data-testid="apply-dialog-verify-create-pair-rule-earlier.mod-removed.mod"]')
          .trigger("click");
        await flush();
        await wrapper.vm.$nextTick();

        expect(upserts).toEqual([
          {
            kind: "pair",
            after: "earlier.mod",
            before: "removed.mod",
            origin: "userDecision",
            comment: REMOVED_BY_REORDER.rationale,
            promotedFrom: null,
            alreadyPromoted: false,
            overridesDeclared: false,
          },
        ]);
      });

      it("confirms on the row that created the rule, disables it, and reads 'already exists' on another row asking for the same pair", async () => {
        const upserts: unknown[] = [];
        installMockIpc({
          list_rules: EMPTY_RULE_SET,
          list_mod_names: {},
          get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
          get_dashboard: dashboardFixture(0),
          get_apply_preflight: preflightFixture(),
          get_merge_mod: emptyMergeModFixture(),
          get_def_cache_carrier: { carrierModId: null },
          list_order: [],
          verify_order: () =>
            verifyReportFixture([
              orderFixableOperation(
                "removed.mod",
                "Wall",
                { kind: "removedBy", modId: "earlier.mod" },
                REMOVED_BY_REORDER,
              ),
              // A second, genuinely distinct operation whose fix is the
              // same relation — one rule covers both.
              orderFixableOperation(
                "removed.mod",
                "Door",
                { kind: "removedBy", modId: "earlier.mod" },
                REMOVED_BY_REORDER,
              ),
            ]),
          upsert_rule: (payload: unknown) => {
            upserts.push((payload as { rule: unknown }).rule);
            return null;
          },
        });
        const { wrapper } = mountDialog();
        await flush();
        await wrapper.vm.$nextTick();

        await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
        await flush();
        await wrapper.vm.$nextTick();

        const buttons = wrapper.findAll(
          '[data-testid="apply-dialog-verify-create-pair-rule-earlier.mod-removed.mod"]',
        );
        expect(buttons.length).toBe(2);
        const first = buttons[0];
        if (!first) throw new Error("expected the first row's button");
        await first.trigger("click");
        await flush();
        await wrapper.vm.$nextTick();

        expect(upserts.length).toBe(1);
        expect(
          wrapper.findAll(
            '[data-testid="apply-dialog-verify-pair-rule-created-earlier.mod-removed.mod"]',
          ).length,
        ).toBe(1);
        expect(
          wrapper
            .get('[data-testid="apply-dialog-verify-pair-rule-exists-earlier.mod-removed.mod"]')
            .text(),
        ).toContain("Rule already exists");
        for (const button of wrapper.findAll(
          '[data-testid="apply-dialog-verify-create-pair-rule-earlier.mod-removed.mod"]',
        )) {
          expect(button.attributes("disabled")).toBeDefined();
        }

        // A second click on the already-created pair is inert, not a
        // second write: `upsert_rule` is upsert-by-key, so a duplicate
        // would silently rewrite the row rather than error.
        await first.trigger("click");
        await flush();
        expect(upserts.length).toBe(1);
      });

      /**
       * `isOrderFixableOperation` is a *display*
       * judgment (`defs.every(...)`). An operation whose def targets are
       * a mix of `removedBy` and `deadTarget` is filed under "not fully
       * order-fixable", but a reorder still fixes the def targets it
       * covers — and `apps/cli`'s `verify` prints the very same
       * `rule set-pair` line for it, so the button is not gated on that
       * judgment — the two surfaces must agree about whether a fix exists.
       */
      it("offers the button on a mixed operation filed under the grouped section", async () => {
        installMockIpc({
          list_rules: EMPTY_RULE_SET,
          list_mod_names: {},
          get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
          get_dashboard: dashboardFixture(0),
          get_apply_preflight: preflightFixture(),
          get_merge_mod: emptyMergeModFixture(),
          get_def_cache_carrier: { carrierModId: null },
          list_order: [],
          verify_order: () =>
            verifyReportFixture([
              verifyOperation({
                modId: "mixed.mod",
                operation: 'Verse.PatchOperationAdd(Defs/ThingDef[defName="A" or defName="B"])',
                defs: [
                  {
                    defKey: { defType: "ThingDef", defName: "A" },
                    selector: "defName",
                    leafXpath: null,
                    cause: { kind: "removedBy", modId: "earlier.mod" },
                    reorderKind: { kind: "content" },
                    reorder: REMOVED_BY_REORDER,
                  },
                  // The def target no reorder can help — what makes the
                  // whole operation "not fully order-fixable".
                  {
                    defKey: { defType: "ThingDef", defName: "B" },
                    selector: "defName",
                    leafXpath: null,
                    cause: { kind: "deadTarget" },
                    reorderKind: null,
                    reorder: null,
                  },
                ],
              }),
            ]),
        });
        const { wrapper } = mountDialog();
        await flush();
        await wrapper.vm.$nextTick();

        await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
        await flush();
        await wrapper.vm.$nextTick();

        // It really is filed under the grouped section, not the
        // order-fixable one — the grouping split stays.
        expect(wrapper.find('[data-testid="apply-dialog-verify-order-fixable"]').exists()).toBe(
          false,
        );
        expect(wrapper.get('[data-testid="apply-dialog-verify-grouped"]').text()).toContain(
          "Not fully order-fixable",
        );
        expect(wrapper.get('[data-testid="apply-dialog-verify-partly-fixable"]').text()).toContain(
          "partly order-fixable",
        );
        expect(
          wrapper
            .get('[data-testid="apply-dialog-verify-create-pair-rule-earlier.mod-removed.mod"]')
            .attributes("disabled"),
        ).toBeUndefined();
      });

      /**
       * The dedup needs a test with teeth — every
       * other fixture has one def per operation, so deleting
       * `distinctReorders`' `seen` map would pass them all. Three def targets,
       * one relation, one button.
       */
      it("renders one button for three def targets that share a reorder", async () => {
        const defNames = ["Wall", "Door", "Lamp"];
        installMockIpc({
          list_rules: EMPTY_RULE_SET,
          list_mod_names: {},
          get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
          get_dashboard: dashboardFixture(0),
          get_apply_preflight: preflightFixture(),
          get_merge_mod: emptyMergeModFixture(),
          get_def_cache_carrier: { carrierModId: null },
          list_order: [],
          verify_order: () =>
            verifyReportFixture([
              verifyOperation({
                modId: "removed.mod",
                operation: 'Verse.PatchOperationAdd(Defs/ThingDef[defName="Wall" or ...])',
                defs: defNames.map((defName) => ({
                  defKey: { defType: "ThingDef", defName },
                  selector: "defName" as const,
                  leafXpath: null,
                  cause: { kind: "removedBy" as const, modId: "earlier.mod" },
                  reorderKind: { kind: "content" as const },
                  reorder: REMOVED_BY_REORDER,
                })),
              }),
            ]),
        });
        const { wrapper } = mountDialog();
        await flush();
        await wrapper.vm.$nextTick();

        await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
        await flush();
        await wrapper.vm.$nextTick();

        expect(
          wrapper.findAll('[data-testid^="apply-dialog-verify-create-pair-rule-"]').length,
        ).toBe(1);
      });

      /**
       * `upsert_rule` replaces a `userDecision` pair
       * rule **wholesale**. Creating one from a verify row without
       * reading what is stored clears `overridesDeclared` (dropping the
       * rule out of `Layer::DeclaredOverride`) and overwrites a
       * hand-written comment — while the UI claims "Rule created".
       */
      it("preserves an existing rule's override flag and comment, and says it already exists", async () => {
        const upserts: unknown[] = [];
        installMockIpc({
          list_rules: {
            ...EMPTY_RULE_SET,
            pairs: [
              {
                after: "earlier.mod",
                before: "removed.mod",
                origin: "userDecision",
                comment: "hand-written, keep me",
                promotedFrom: null,
                alreadyPromoted: false,
                overridesDeclared: true,
              },
            ],
          } satisfies RuleSetDto,
          list_mod_names: {},
          get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
          get_dashboard: dashboardFixture(0),
          get_apply_preflight: preflightFixture(),
          get_merge_mod: emptyMergeModFixture(),
          get_def_cache_carrier: { carrierModId: null },
          list_order: [],
          verify_order: () =>
            verifyReportFixture([
              orderFixableOperation(
                "removed.mod",
                "Wall",
                { kind: "removedBy", modId: "earlier.mod" },
                REMOVED_BY_REORDER,
              ),
            ]),
          upsert_rule: (payload: unknown) => {
            upserts.push((payload as { rule: unknown }).rule);
            return null;
          },
        });
        const { wrapper } = mountDialog();
        await flush();
        await wrapper.vm.$nextTick();

        await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
        await flush();
        await wrapper.vm.$nextTick();

        // Disclosed before anything is written, not after.
        expect(
          wrapper
            .get('[data-testid="apply-dialog-verify-pair-rule-exists-earlier.mod-removed.mod"]')
            .text(),
        ).toContain("Rule already exists");

        await wrapper
          .get('[data-testid="apply-dialog-verify-create-pair-rule-earlier.mod-removed.mod"]')
          .trigger("click");
        await flush();
        await wrapper.vm.$nextTick();

        expect(upserts).toEqual([
          {
            kind: "pair",
            after: "earlier.mod",
            before: "removed.mod",
            origin: "userDecision",
            comment: "hand-written, keep me",
            promotedFrom: null,
            alreadyPromoted: false,
            overridesDeclared: true,
          },
        ]);
        // Still "already exists" afterwards — it was never created here.
        expect(
          wrapper
            .find('[data-testid="apply-dialog-verify-pair-rule-created-earlier.mod-removed.mod"]')
            .exists(),
        ).toBe(false);
      });
    });

    it("groups DeadTarget/Unknown operations by mod instead of one row each", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        verify_order: () =>
          verifyReportFixture([
            verifyOperation({
              modId: "stale.mod",
              operation: 'Verse.PatchOperationAdd(Defs/ThingDef[defName="A"]/comps)',
              defs: [
                {
                  defKey: { defType: "ThingDef", defName: "A" },
                  selector: "defName",
                  leafXpath: null,
                  cause: { kind: "deadTarget" },
                  reorderKind: null,
                  reorder: null,
                },
              ],
            }),
            verifyOperation({
              modId: "stale.mod",
              operation: 'Verse.PatchOperationAdd(Defs/ThingDef[defName="B"]/comps)',
              defs: [
                {
                  defKey: { defType: "ThingDef", defName: "B" },
                  selector: "defName",
                  leafXpath: null,
                  cause: { kind: "deadTarget" },
                  reorderKind: null,
                  reorder: null,
                },
              ],
            }),
            verifyOperation({
              modId: "other.mod",
              defs: [
                {
                  defKey: { defType: "ThingDef", defName: "C" },
                  selector: "defName",
                  leafXpath: null,
                  cause: { kind: "unknown" },
                  reorderKind: null,
                  reorder: null,
                },
              ],
            }),
          ]),
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      expect(wrapper.find('[data-testid="apply-dialog-verify-order-fixable"]').exists()).toBe(
        false,
      );
      expect(wrapper.get('[data-testid="apply-dialog-verify-group-stale.mod"]').text()).toContain(
        "2 operations",
      );
      expect(wrapper.get('[data-testid="apply-dialog-verify-group-other.mod"]').text()).toContain(
        "1 operation",
      );
      const causeCounts = wrapper.get('[data-testid="apply-dialog-verify-cause-counts"]');
      expect(causeCounts.text()).toContain("Dead target: 2");
      expect(causeCounts.text()).toContain("Unknown: 1");
    });

    it("shows a progress area while a pass is in flight", async () => {
      let resolveVerify: (report: VerifyReportDto) => void = () => {};
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        verify_order: () =>
          new Promise<VerifyReportDto>((resolve) => {
            resolveVerify = resolve;
          }),
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await wrapper.vm.$nextTick();

      expect(wrapper.find('[data-testid="apply-dialog-verify-progress"]').exists()).toBe(true);

      resolveVerify(verifyReportFixture([]));
      await flush();
      await wrapper.vm.$nextTick();

      expect(wrapper.find('[data-testid="apply-dialog-verify-progress"]').exists()).toBe(false);
      expect(wrapper.get('[data-testid="apply-dialog-verify-summary"]').text()).toContain(
        "0 operations predicted to fail",
      );
    });

    it("shows an error message when the pass itself fails", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        verify_order: () => {
          throw { code: "no_project_loaded", message: "no project is loaded" };
        },
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      expect(wrapper.get('[data-testid="apply-dialog-verify-error"]').text()).toContain(
        "no project is loaded",
      );
    });

    it("resets the verify report when the dialog is reopened", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        verify_order: () => verifyReportFixture([verifyOperation()]),
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();
      expect(wrapper.find('[data-testid="apply-dialog-verify-report"]').exists()).toBe(true);

      await wrapper.setProps({ visible: false });
      await wrapper.setProps({ visible: true });
      await wrapper.vm.$nextTick();

      expect(wrapper.find('[data-testid="apply-dialog-verify-report"]').exists()).toBe(false);
    });

    it("joins a verify pass against an imported log into matched/predicted-only/observed-only groups", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        verify_order: () =>
          verifyReportFixture([
            verifyOperation({ modId: "x.mod", operation: "Verse.PatchOperationAdd(Defs/X)" }),
            verifyOperation({ modId: "y.mod", operation: "Verse.PatchOperationAdd(Defs/Y)" }),
          ]),
      });
      const { wrapper, session } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      // No import yet — the section stays absent even with a verify report in hand.
      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();
      expect(wrapper.find('[data-testid="apply-dialog-predicted-vs-observed"]').exists()).toBe(
        false,
      );

      session.setGameLogSummary({
        patchFailures: [
          // Confirms x.mod's prediction.
          {
            attribution: { kind: "mod", modId: "x.mod" },
            operation: "Verse.PatchOperationAdd(Defs/X)",
            sourceFile: null,
            stackTrace: null,
          },
          // Observed, but never predicted, and never attributed to an active mod.
          {
            attribution: { kind: "unattributed", raw: "[Some Mod]" },
            operation: "Verse.PatchOperationAdd(Defs/Unrelated)",
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
      });
      await wrapper.vm.$nextTick();

      expect(wrapper.get('[data-testid="apply-dialog-pvo-matched-summary"]').text()).toBe(
        "Confirmed by the log: 1",
      );
      expect(wrapper.get('[data-testid="apply-dialog-pvo-matched-x.mod"]').text()).toContain(
        "Verse.PatchOperationAdd(Defs/X)",
      );
      // y.mod was predicted but never observed.
      expect(wrapper.get('[data-testid="apply-dialog-pvo-predicted-only-summary"]').text()).toBe(
        "Predicted, not observed in the log: 1",
      );
      expect(wrapper.get('[data-testid="apply-dialog-pvo-predicted-only-y.mod"]').text()).toContain(
        "Verse.PatchOperationAdd(Defs/Y)",
      );
      // The unattributed, unmatched line is listed as-is, never dropped.
      const observedOnly = wrapper.get('[data-testid="apply-dialog-pvo-observed-only-entry"]');
      expect(observedOnly.text()).toContain("[Some Mod]");
      expect(observedOnly.text()).toContain("Verse.PatchOperationAdd(Defs/Unrelated)");
    });

    it("warns above the predicted-vs-observed panel when the imported log's own order differs from the one just verified", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [
          {
            modId: "a.mod",
            name: "A",
            position: 0,
            previousPosition: null,
            tier: "core",
            tags: [],
            hardDependents: 0,
            needsInputCount: 0,
          },
          {
            modId: "b.mod",
            name: "B",
            position: 1,
            previousPosition: null,
            tier: "body",
            tags: [],
            hardDependents: 0,
            needsInputCount: 0,
          },
        ],
        verify_order: () =>
          verifyReportFixture([
            verifyOperation({ modId: "x.mod", operation: "Verse.PatchOperationAdd(Defs/X)" }),
          ]),
      });
      const { wrapper, session } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      // Confirms the one prediction, so the panel renders — but the
      // logged order (b.mod, a.mod) is reversed from the selected one
      // (a.mod, b.mod) `list_order` reports above.
      session.setGameLogSummary({
        patchFailures: [
          {
            attribution: { kind: "mod", modId: "x.mod" },
            operation: "Verse.PatchOperationAdd(Defs/X)",
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
        loadEvents: [{ line: 1, kind: { type: "newGame" }, mods: ["b.mod", "a.mod"] }],
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
      });
      await wrapper.vm.$nextTick();

      // The panel still renders (the evidence isn't hidden)...
      expect(wrapper.find('[data-testid="apply-dialog-predicted-vs-observed"]').exists()).toBe(
        true,
      );
      // ...but flagged as comparing two unrelated runs.
      expect(
        wrapper.get('[data-testid="apply-dialog-pvo-order-mismatch-warning"]').text(),
      ).toContain("different mod order");
    });

    it("shows no order-mismatch warning when the logged order matches the selected one", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [
          {
            modId: "a.mod",
            name: "A",
            position: 0,
            previousPosition: null,
            tier: "core",
            tags: [],
            hardDependents: 0,
            needsInputCount: 0,
          },
        ],
        verify_order: () =>
          verifyReportFixture([
            verifyOperation({ modId: "x.mod", operation: "Verse.PatchOperationAdd(Defs/X)" }),
          ]),
      });
      const { wrapper, session } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      session.setGameLogSummary({
        patchFailures: [
          {
            attribution: { kind: "mod", modId: "x.mod" },
            operation: "Verse.PatchOperationAdd(Defs/X)",
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
        loadEvents: [{ line: 1, kind: { type: "newGame" }, mods: ["a.mod"] }],
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
      });
      await wrapper.vm.$nextTick();

      expect(wrapper.find('[data-testid="apply-dialog-predicted-vs-observed"]').exists()).toBe(
        true,
      );
      expect(wrapper.find('[data-testid="apply-dialog-pvo-order-mismatch-warning"]').exists()).toBe(
        false,
      );
    });

    describe("a partial imported log", () => {
      // Each caveat carries its own test id (the last segment of its key).
      const SNAPSHOT_NOTE = '[data-testid="apply-dialog-pvoSnapshotNote"]';
      const CUT_OFF_NOTE = '[data-testid="apply-dialog-pvoCutOffNote"]';
      const GAPS_NOTE = '[data-testid="apply-dialog-pvoGapsNote"]';
      const ANY_NOTE = [SNAPSHOT_NOTE, CUT_OFF_NOTE, GAPS_NOTE].join(", ");

      async function verifiedWithLog(log: GameLogSummaryDto) {
        installMockIpc({
          list_rules: EMPTY_RULE_SET,
          list_mod_names: {},
          get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
          get_dashboard: dashboardFixture(0),
          get_apply_preflight: preflightFixture(),
          get_merge_mod: emptyMergeModFixture(),
          get_def_cache_carrier: { carrierModId: null },
          list_order: [],
          verify_order: () =>
            verifyReportFixture([
              verifyOperation({ modId: "x.mod", operation: "Verse.PatchOperationAdd(Defs/X)" }),
            ]),
        });
        const { wrapper, session } = mountDialog();
        await flush();
        await wrapper.vm.$nextTick();
        await wrapper.get('[data-testid="apply-dialog-verify-button"]').trigger("click");
        await flush();
        await wrapper.vm.$nextTick();
        session.setGameLogSummary(log);
        await wrapper.vm.$nextTick();
        return wrapper;
      }

      it("says 'predicted, not observed' is no evidence when the log is a console snapshot, and still shows the panel", async () => {
        const wrapper = await verifiedWithLog(gameLogSummary({ coverage: snapshotCoverage(1000) }));

        expect(wrapper.find('[data-testid="apply-dialog-predicted-vs-observed"]').exists()).toBe(
          true,
        );
        expect(wrapper.get(SNAPSHOT_NOTE).text()).toContain("console snapshot");
      });

      it("says so too when the game stopped writing messages during the log", async () => {
        const wrapper = await verifiedWithLog(
          gameLogSummary({ loggingGaps: [{ stopLine: 10, end: { kind: "neverResumed" } }] }),
        );

        expect(wrapper.get(GAPS_NOTE).text()).toContain("stopped writing messages");
      });

      it("warns that a cut-off Player.log with no patch result logged is no evidence either", async () => {
        const wrapper = await verifiedWithLog(
          gameLogSummary({ coverage: playerLogCoverage({ endState: { kind: "truncated" } }) }),
        );

        expect(wrapper.get(CUT_OFF_NOTE).text()).toContain(
          "ended without a clean exit and before any patch result was logged",
        );
      });

      it("warns for a crashed Player.log that logged no patch result", async () => {
        const wrapper = await verifiedWithLog(
          gameLogSummary({
            coverage: playerLogCoverage({ endState: { kind: "crashed", reportPath: null } }),
          }),
        );

        expect(wrapper.get(CUT_OFF_NOTE).text()).toContain(
          "ended without a clean exit and before any patch result was logged",
        );
      });

      it("does not call a cut-off log blind once a patch failure was logged", async () => {
        const wrapper = await verifiedWithLog(
          gameLogSummary({
            coverage: playerLogCoverage({
              endState: { kind: "truncated" },
              patchPhase: "patchFailureLogged",
            }),
          }),
        );

        expect(wrapper.find(ANY_NOTE).exists()).toBe(false);
      });

      it("lists both notes when a cut-off Player.log also has a logging gap", async () => {
        const wrapper = await verifiedWithLog(
          gameLogSummary({
            coverage: playerLogCoverage({ endState: { kind: "truncated" } }),
            loggingGaps: [{ stopLine: 10, end: { kind: "neverResumed" } }],
          }),
        );

        const notes = wrapper.findAll(ANY_NOTE).map((note) => note.text());
        expect(notes).toHaveLength(2);
        expect(notes[0]).toContain(
          "ended without a clean exit and before any patch result was logged",
        );
        expect(notes[1]).toContain("stopped writing messages");
      });

      it("adds no note for a whole Player.log without gaps", async () => {
        const wrapper = await verifiedWithLog(gameLogSummary());

        expect(wrapper.find('[data-testid="apply-dialog-predicted-vs-observed"]').exists()).toBe(
          true,
        );
        expect(wrapper.find(ANY_NOTE).exists()).toBe(false);
      });
    });
  });
});
