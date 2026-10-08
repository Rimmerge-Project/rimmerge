// ApplyDialog: paths, submit modes, force confirmation, and the merge-mod summary.

import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import {
  dashboardFixture,
  EMPTY_PENDING_ACTIVE_CHANGES,
  EMPTY_RULE_SET,
  emptyMergeModFixture,
  flush,
  mergeModFixtureWithEntry,
  mountDialog,
  preflightFixture,
} from "@/components/apply/ApplyDialog.test-support";
import { installMockIpc } from "@/services/ipc.mock";
import type { ApplyRequestDto } from "@/types/generated/ApplyRequestDto";
import type { MergeModDto } from "@/types/generated/MergeModDto";

describe("ApplyDialog", () => {
  afterEach(() => {
    clearMocks();
  });

  it("shows the order source and the ModsConfig/decisions/rules paths", async () => {
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="apply-dialog-order-source"]').text()).toBe("Current");
    expect(wrapper.get('[data-testid="apply-dialog-mods-config-path"]').text()).toBe(
      "C:/RimWorld/ModsConfig.xml",
    );
    expect(wrapper.get('[data-testid="apply-dialog-decisions-path"]').text()).toBe(
      "C:/Profile/rimmerge/decisions.json",
    );
    expect(wrapper.get('[data-testid="apply-dialog-rules-path"]').text()).toBe(
      "C:/Profile/rimmerge/rules.json",
    );
    expect(wrapper.get('[data-testid="apply-dialog-moved-mods"]').text()).toBe("4");
    expect(wrapper.get('[data-testid="apply-dialog-needs-input"]').text()).toBe("0");
    expect(wrapper.get('[data-testid="apply-dialog-sort-provenance"]').text()).toBe(
      "tie-break: rebuild · imported pairs: off · imported placements: on",
    );
  });

  it("enables submit immediately when nothing needs input", async () => {
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="apply-dialog-inbox-link"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="apply-dialog-submit"]').attributes("disabled")).toBe(
      undefined,
    );
  });

  it("keeps submit enabled when findings need input, showing the count and an Inbox link", async () => {
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(3),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="apply-dialog-needs-input"]').text()).toBe("3");
    expect(wrapper.find('[data-testid="apply-dialog-inbox-link"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="apply-dialog-submit"]').attributes("disabled")).toBe(
      undefined,
    );
  });

  it("submits { writeModsConfig: true, force: false } by default", async () => {
    const calls: ApplyRequestDto[] = [];
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
      apply: (payload: unknown) => {
        calls.push((payload as { request: ApplyRequestDto }).request);
        return {
          modsConfigPath: "C:/RimWorld/ModsConfig.xml",
          backupPath: "C:/RimWorld/ModsConfig.xml.bak-x",
          decisionsPath: "C:/Profile/rimmerge/decisions.json",
          rulesPath: "C:/Profile/rimmerge/rules.json",
          mergeModPath: null,
          mergeModBackupPath: null,
          skippedMerges: [],
        };
      },
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(calls).toEqual([
      { source: "current", writeModsConfig: true, writeMergeMod: false, force: false },
    ]);
    expect(wrapper.emitted("update:visible")?.at(-1)).toEqual([false]);
  });

  it("sends the store's selected order as the request's source", async () => {
    const calls: ApplyRequestDto[] = [];
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
      apply: (payload: unknown) => {
        calls.push((payload as { request: ApplyRequestDto }).request);
        return {
          modsConfigPath: "C:/RimWorld/ModsConfig.xml",
          backupPath: "C:/RimWorld/ModsConfig.xml.bak-x",
          decisionsPath: "C:/Profile/rimmerge/decisions.json",
          rulesPath: "C:/Profile/rimmerge/rules.json",
          mergeModPath: null,
          mergeModBackupPath: null,
          skippedMerges: [],
        };
      },
    });
    const { wrapper, session } = mountDialog();
    session.setSelected("suggested");
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(calls.map((request) => request.source)).toEqual(["suggested"]);
  });

  describe("write checkboxes", () => {
    const CHECKBOX = '[data-testid="apply-dialog-write-modsconfig-checkbox"] input';
    const MERGE_CHECKBOX = '[data-testid="apply-dialog-write-merge-mod-checkbox"] input';
    const SUBMIT = '[data-testid="apply-dialog-submit"]';

    function mountRecording(calls: ApplyRequestDto[]): ReturnType<typeof mountDialog> {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        apply: (payload: unknown) => {
          calls.push((payload as { request: ApplyRequestDto }).request);
          return {
            modsConfigPath: "C:/RimWorld/ModsConfig.xml",
            backupPath: null,
            decisionsPath: "C:/Profile/rimmerge/decisions.json",
            rulesPath: "C:/Profile/rimmerge/rules.json",
            mergeModPath: null,
            mergeModBackupPath: null,
            skippedMerges: [],
          };
        },
      });
      return mountDialog();
    }

    it("opens with Write ModsConfig.xml checked and the button reading Apply", async () => {
      const { wrapper } = mountRecording([]);
      await flush();
      await wrapper.vm.$nextTick();

      expect((wrapper.get(CHECKBOX).element as HTMLInputElement).checked).toBe(true);
      expect(wrapper.get(SUBMIT).text()).toBe("Apply");
    });

    it("keeps reading Apply when only the merge mod is added", async () => {
      const calls: ApplyRequestDto[] = [];
      const { wrapper } = mountRecording(calls);
      await flush();
      await wrapper.get(MERGE_CHECKBOX).setValue(true);

      expect(wrapper.get(SUBMIT).text()).toBe("Apply");
    });

    it("reads Write merge mod and submits writeModsConfig:false when only the merge mod is on", async () => {
      const calls: ApplyRequestDto[] = [];
      const { wrapper } = mountRecording(calls);
      await flush();
      await wrapper.get(CHECKBOX).setValue(false);
      await wrapper.get(MERGE_CHECKBOX).setValue(true);

      expect(wrapper.get(SUBMIT).text()).toBe("Write merge mod");
      await wrapper.get(SUBMIT).trigger("click");
      await flush();

      expect(calls).toEqual([
        { source: "current", writeModsConfig: false, writeMergeMod: true, force: false },
      ]);
    });

    it("reads Save decisions and rules and submits both flags off when neither is on", async () => {
      const calls: ApplyRequestDto[] = [];
      const { wrapper } = mountRecording(calls);
      await flush();
      await wrapper.get(CHECKBOX).setValue(false);

      expect(wrapper.get(SUBMIT).text()).toBe("Save decisions and rules");
      await wrapper.get(SUBMIT).trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      expect(calls).toEqual([
        { source: "current", writeModsConfig: false, writeMergeMod: false, force: false },
      ]);
    });
  });

  it("shows a force-confirmation and retries with force:true when the game is running", async () => {
    const calls: ApplyRequestDto[] = [];
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
      apply: (payload: unknown) => {
        const request = (payload as { request: ApplyRequestDto }).request;
        calls.push(request);
        if (!request.force) {
          throw {
            code: "rimworld_running",
            message: "RimWorldWin64.exe is running; close the game or retry with force",
          };
        }
        return {
          modsConfigPath: "C:/RimWorld/ModsConfig.xml",
          backupPath: "C:/RimWorld/ModsConfig.xml.bak-x",
          decisionsPath: "C:/Profile/rimmerge/decisions.json",
          rulesPath: "C:/Profile/rimmerge/rules.json",
          mergeModPath: null,
          mergeModBackupPath: null,
          skippedMerges: [],
        };
      },
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="apply-dialog-force-warning"]').exists()).toBe(true);
    expect(wrapper.emitted("update:visible")).toBeUndefined();

    await wrapper.get('[data-testid="apply-dialog-write-anyway"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(calls).toEqual([
      { source: "current", writeModsConfig: true, writeMergeMod: false, force: false },
      { source: "current", writeModsConfig: true, writeMergeMod: false, force: true },
    ]);
    expect(wrapper.emitted("update:visible")?.at(-1)).toEqual([false]);
  });

  it("shows a generic error message for any other apply failure", async () => {
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
      apply: () => {
        throw { code: "mods_config_io_failed", message: "disk is full" };
      },
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="apply-dialog-error"]').text()).toContain("disk is full");
    expect(wrapper.emitted("update:visible")).toBeUndefined();
  });

  it("resets the force-confirmation state when reopened", async () => {
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
      apply: () => {
        throw { code: "rimworld_running", message: "running" };
      },
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();
    expect(wrapper.find('[data-testid="apply-dialog-force-warning"]').exists()).toBe(true);

    await wrapper.setProps({ visible: false });
    await wrapper.setProps({ visible: true });
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="apply-dialog-force-warning"]').exists()).toBe(false);
  });

  it("leaves 'Write merge mod' unchecked by default even when a complete merge entry exists", async () => {
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: mergeModFixtureWithEntry({ kind: "complete", opCount: 4 }),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    expect(
      wrapper
        .get('[data-testid="apply-dialog-write-merge-mod-checkbox"]')
        .attributes("data-p-checked"),
    ).toBe("false");
  });

  // The apply dialog's summary line counts the two groups separately.
  it("shows a merge-mod summary line counting patch-collision and def-override merges separately", async () => {
    const mergeMod: MergeModDto = {
      ...emptyMergeModFixture(),
      entries: [
        {
          key: "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]",
          defKey: { defType: "HediffDef", defName: "BionicHeart" },
          kind: "defOverride",
          state: { kind: "complete", opCount: 1 },
          opCount: 1,
          dependsOn: ["example.bionicsfork"],
          patchFile: "Patches/rimmerge_HediffDef.xml",
          structuralGuardField: null,
        },
        {
          key: "patch_collision:BiomeDef/TemperateForest:defName:plantDensity:[a,b]",
          defKey: { defType: "BiomeDef", defName: "TemperateForest" },
          kind: "patchCollision",
          state: { kind: "complete", opCount: 1 },
          opCount: 1,
          dependsOn: ["a", "b"],
          patchFile: "Patches/rimmerge_BiomeDef.xml",
          structuralGuardField: null,
        },
        // Not yet decided/complete — must not inflate either count.
        {
          key: "def_override:ThingDef/Wall:[a,b]",
          defKey: { defType: "ThingDef", defName: "Wall" },
          kind: "defOverride",
          state: { kind: "needsFieldInput", unresolved: 1, total: 1 },
          opCount: 0,
          dependsOn: [],
          patchFile: null,
          structuralGuardField: null,
        },
      ],
    };
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: mergeMod,
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="apply-dialog-merge-mod-status"]').text()).toBe(
      "Will write 1 patch-collision merge (auto-suggested where undecided) and " +
        "1 def-override merge (explicitly decided).",
    );
  });

  it("shows no merge-mod summary line when nothing reaches the merge mod", async () => {
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="apply-dialog-merge-mod-status"]').text()).toBe("");
  });

  // A structurally guarded skip must read
  // as a decision to confirm, never as an incomplete merge — on the apply
  // dialog's own skip line too.
  it("describes a structurally-guarded skip as 'not auto-merged — confirm the winner', not 'still needs input'", async () => {
    const guardedKey = "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]";
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: mergeModFixtureWithEntry(
        { kind: "needsFieldInput", unresolved: 9, total: 9 },
        { structuralGuardField: "ParentName" },
      ),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
      apply: () => ({
        modsConfigPath: "C:/RimWorld/ModsConfig.xml",
        backupPath: "C:/RimWorld/ModsConfig.xml.bak-x",
        decisionsPath: "C:/Profile/rimmerge/decisions.json",
        rulesPath: "C:/Profile/rimmerge/rules.json",
        mergeModPath: null,
        mergeModBackupPath: null,
        skippedMerges: [guardedKey],
      }),
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="apply-dialog-write-merge-mod-checkbox"] input').setValue(true);
    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    const skipped = wrapper.get('[data-testid="apply-dialog-skipped-merges"]');
    expect(skipped.text()).toContain(guardedKey);
    expect(skipped.text()).toContain("not auto-merged — confirm the winner");
    expect(skipped.text()).not.toContain("still needs input");
  });

  it("sends writeMergeMod: true when the checkbox is checked, and shows the returned merge mod path without auto-closing", async () => {
    const calls: ApplyRequestDto[] = [];
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
      apply: (payload: unknown) => {
        calls.push((payload as { request: ApplyRequestDto }).request);
        return {
          modsConfigPath: "C:/RimWorld/ModsConfig.xml",
          backupPath: "C:/RimWorld/ModsConfig.xml.bak-x",
          decisionsPath: "C:/Profile/rimmerge/decisions.json",
          rulesPath: "C:/Profile/rimmerge/rules.json",
          mergeModPath: "C:/RimWorld/Mods/rimmerge_merge_abc123def456",
          mergeModBackupPath: null,
          skippedMerges: ["patch_collision:BiomeDef/TemperateForest:defName:plantDensity:[a,b]"],
        };
      },
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="apply-dialog-write-merge-mod-checkbox"] input').setValue(true);
    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(calls).toEqual([
      { source: "current", writeModsConfig: true, writeMergeMod: true, force: false },
    ]);
    // The dialog stays open (unlike the plain-apply tests above) so the
    // user can read where the merge mod landed before dismissing it.
    expect(wrapper.emitted("update:visible")).toBeUndefined();
    expect(wrapper.get('[data-testid="apply-dialog-merge-mod-path"]').text()).toContain(
      "rimmerge_merge_abc123def456",
    );
    expect(wrapper.get('[data-testid="apply-dialog-skipped-merges"]').text()).toContain(
      "plantDensity",
    );

    await wrapper.get('[data-testid="apply-dialog-close-summary"]').trigger("click");
    expect(wrapper.emitted("update:visible")?.at(-1)).toEqual([false]);
  });

  it("shows 'removed' and stays open when writeMergeMod was true but nothing came back to render", async () => {
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
      apply: () => ({
        modsConfigPath: "C:/RimWorld/ModsConfig.xml",
        backupPath: "C:/RimWorld/ModsConfig.xml.bak-x",
        decisionsPath: "C:/Profile/rimmerge/decisions.json",
        rulesPath: "C:/Profile/rimmerge/rules.json",
        mergeModPath: null,
        mergeModBackupPath: null,
        skippedMerges: [],
      }),
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="apply-dialog-write-merge-mod-checkbox"] input').setValue(true);
    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.emitted("update:visible")).toBeUndefined();
    expect(wrapper.get('[data-testid="apply-dialog-merge-mod-removed"]').text()).toBe(
      "Merge mod: removed (no merge or asset decision remains)",
    );

    await wrapper.get('[data-testid="apply-dialog-close-summary"]').trigger("click");
    expect(wrapper.emitted("update:visible")?.at(-1)).toEqual([false]);
  });

  it("does not show a merge-mod summary and closes normally when writeMergeMod was never requested", async () => {
    installMockIpc({
      list_rules: EMPTY_RULE_SET,
      list_mod_names: {},
      get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
      get_dashboard: dashboardFixture(0),
      get_apply_preflight: preflightFixture(),
      get_merge_mod: emptyMergeModFixture(),
      get_def_cache_carrier: { carrierModId: null },
      list_order: [],
      apply: () => ({
        modsConfigPath: "C:/RimWorld/ModsConfig.xml",
        backupPath: "C:/RimWorld/ModsConfig.xml.bak-x",
        decisionsPath: "C:/Profile/rimmerge/decisions.json",
        rulesPath: "C:/Profile/rimmerge/rules.json",
        mergeModPath: null,
        mergeModBackupPath: null,
        skippedMerges: [],
      }),
    });
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.emitted("update:visible")?.at(-1)).toEqual([false]);
    expect(wrapper.find('[data-testid="apply-dialog-merge-summary"]').exists()).toBe(false);
  });

  describe("applied event", () => {
    function installApplyThatReturns(mergeModPath: string | null): void {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        apply: () => ({
          modsConfigPath: "C:/RimWorld/ModsConfig.xml",
          backupPath: null,
          decisionsPath: "C:/Profile/rimmerge/decisions.json",
          rulesPath: "C:/Profile/rimmerge/rules.json",
          mergeModPath,
          mergeModBackupPath: null,
          skippedMerges: [],
        }),
      });
    }

    it("is emitted once, with the request's writeModsConfig, after a successful apply", async () => {
      installApplyThatReturns(null);
      const { wrapper } = mountDialog();
      await flush();

      await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
      await flush();

      expect(wrapper.emitted("applied")).toEqual([[{ wroteModsConfig: true }]]);
    });

    it("reports wroteModsConfig false when only decisions and rules were saved", async () => {
      installApplyThatReturns(null);
      const { wrapper } = mountDialog();
      await flush();
      await wrapper
        .get('[data-testid="apply-dialog-write-modsconfig-checkbox"] input')
        .setValue(false);

      await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
      await flush();

      expect(wrapper.emitted("applied")).toEqual([[{ wroteModsConfig: false }]]);
    });

    it("is emitted once even when the dialog stays open for the merge-mod summary", async () => {
      installApplyThatReturns("C:/RimWorld/Mods/rimmerge_merge_abc123def456");
      const { wrapper } = mountDialog();
      await flush();
      await wrapper
        .get('[data-testid="apply-dialog-write-merge-mod-checkbox"] input')
        .setValue(true);

      await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
      await flush();

      expect(wrapper.emitted("update:visible")).toBeUndefined();
      expect(wrapper.emitted("applied")).toEqual([[{ wroteModsConfig: true }]]);
    });

    it("is not emitted when the apply fails", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
        apply: () => {
          throw { code: "mods_config_io_failed", message: "disk full" };
        },
      });
      const { wrapper } = mountDialog();
      await flush();

      await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
      await flush();

      expect(wrapper.emitted("applied")).toBeUndefined();
    });
  });
});
