// ApplyDialog: the working active-mod set.

import { PiniaColada, useQueryCache } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
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
import ApplyDialog from "@/components/apply/ApplyDialog.vue";
import { queryKeys } from "@/queries/keys";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";

describe("ApplyDialog", () => {
  afterEach(() => {
    clearMocks();
  });

  describe("the working active-mod set", () => {
    it("forces off and disables the Write ModsConfig.xml checkbox, and shows the stale banner, while unscanned", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: {
          unscanned: { added: ["a.mod"], removed: [] },
          unapplied: { added: [], removed: [] },
        },
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      const checkbox = wrapper.get(
        '[data-testid="apply-dialog-write-modsconfig-checkbox"] input[type="checkbox"]',
      );
      expect((checkbox.element as HTMLInputElement).checked).toBe(false);
      expect((checkbox.element as HTMLInputElement).disabled).toBe(true);
      expect(wrapper.get('[data-testid="apply-dialog-stale-active-set"]').text()).toContain(
        "pending changes",
      );
    });

    it("does not force the checkbox or show the stale banner once nothing is unscanned", async () => {
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

      const checkbox = wrapper.get(
        '[data-testid="apply-dialog-write-modsconfig-checkbox"] input[type="checkbox"]',
      );
      expect((checkbox.element as HTMLInputElement).checked).toBe(true);
      expect((checkbox.element as HTMLInputElement).disabled).toBe(false);
      expect(wrapper.find('[data-testid="apply-dialog-stale-active-set"]').exists()).toBe(false);
    });

    it("lists the added and removed unapplied active-mod ids above the summary", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: {
          unscanned: { added: [], removed: [] },
          unapplied: { added: ["a.mod"], removed: ["b.mod"] },
        },
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      const text = wrapper.get('[data-testid="apply-dialog-unapplied-active-changes"]').text();
      expect(text).toContain("a.mod");
      expect(text).toContain("b.mod");
    });

    it("restores an unchecked choice when the set goes stale and then clean again", async () => {
      let stale = false;
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: () =>
          stale
            ? {
                unscanned: { added: ["a.mod"], removed: [] },
                unapplied: { added: [], removed: [] },
              }
            : EMPTY_PENDING_ACTIVE_CHANGES,
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
      });
      const { wrapper, pinia } = mountDialog();
      await flush();
      const checkbox = wrapper.get(
        '[data-testid="apply-dialog-write-modsconfig-checkbox"] input[type="checkbox"]',
      );
      const submit = wrapper.get('[data-testid="apply-dialog-submit"]');
      await checkbox.setValue(false);

      const refreshPending = async (isStale: boolean): Promise<void> => {
        stale = isStale;
        await useQueryCache(pinia).invalidateQueries({
          key: queryKeys.pendingActiveChanges(),
        });
        await flush();
        await wrapper.vm.$nextTick();
      };

      await refreshPending(true);
      expect((checkbox.element as HTMLInputElement).checked).toBe(false);
      expect((checkbox.element as HTMLInputElement).disabled).toBe(true);
      expect(submit.text()).toBe("Save decisions and rules");

      await refreshPending(false);
      expect((checkbox.element as HTMLInputElement).checked).toBe(false);
      expect((checkbox.element as HTMLInputElement).disabled).toBe(false);
      expect(submit.text()).toBe("Save decisions and rules");
    });

    it("clicking Rescan calls rescan_project and clears the stale banner once it resolves", async () => {
      let scanned = false;
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: () =>
          scanned
            ? EMPTY_PENDING_ACTIVE_CHANGES
            : {
                unscanned: { added: ["a.mod"], removed: [] },
                unapplied: { added: [], removed: [] },
              },
        rescan_project: () => {
          scanned = true;
          return {
            modCount: 1,
            gameVersion: "1.6",
            elapsedMs: 1,
            warnings: [],
            ruleWarnings: [],
            selected: "current",
          };
        },
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-rescan-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      expect(wrapper.find('[data-testid="apply-dialog-stale-active-set"]').exists()).toBe(false);
      // The checkbox must also un-force back to its
      // pre-stale (checked, enabled) state, not just hide the banner.
      const checkbox = wrapper.get(
        '[data-testid="apply-dialog-write-modsconfig-checkbox"] input[type="checkbox"]',
      );
      expect((checkbox.element as HTMLInputElement).checked).toBe(true);
      expect((checkbox.element as HTMLInputElement).disabled).toBe(false);
    });

    it("reopening the dialog while already stale, then rescanning, un-forces the checkbox (the real open path, not the always-visible-from-mount one every other test here uses)", async () => {
      // `mountDialog()` always mounts with `props: { visible: true }`, so
      // the `watch(visible, ...)` open-reset handler (in
      // `composables/useApplyDialog.ts`) never transitions in any other
      // test in this file (it does in `ApplyDialog.test.ts`'s "resets the
      // force-confirmation state when reopened" and
      // `ApplyDialog.verify.test.ts`'s "resets the verify report when the
      // dialog is reopened") — a real app opens
      // the dialog from `visible: false`, and the open-reset must not
      // unconditionally clear the "restore to" baseline, or a Rescan run from a
      // dialog that opened *already* stale never un-forces the checkbox
      // (`e2e/specs/active-set.spec.ts` covers the same path through the real
      // `shell-apply-button`). `setProps`
      // exercises that same transition here.
      let scanned = false;
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: () =>
          scanned
            ? EMPTY_PENDING_ACTIVE_CHANGES
            : {
                unscanned: { added: ["a.mod"], removed: [] },
                unapplied: { added: [], removed: [] },
              },
        rescan_project: () => {
          scanned = true;
          return {
            modCount: 1,
            gameVersion: "1.6",
            elapsedMs: 1,
            warnings: [],
            ruleWarnings: [],
            selected: "current",
          };
        },
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
      });
      const pinia = createPinia();
      setActivePinia(pinia);
      const session = useSessionStore();
      session.setLoaded(
        {
          gameDir: "C:/RimWorld",
          workshopDir: "C:/RimWorld/workshop",
          modsConfig: "C:/RimWorld/ModsConfig.xml",
          profileDir: "C:/Profile/rimmerge",
        },
        "current",
      );
      const wrapper = mount(ApplyDialog, {
        props: { visible: false },
        global: {
          plugins: [pinia, PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
          stubs: { teleport: true },
        },
      });
      await flush();
      await wrapper.vm.$nextTick();

      // The working set is already stale by the time the dialog opens.
      await wrapper.setProps({ visible: true });
      await flush();
      await wrapper.vm.$nextTick();

      const checkbox = wrapper.get(
        '[data-testid="apply-dialog-write-modsconfig-checkbox"] input[type="checkbox"]',
      );
      expect((checkbox.element as HTMLInputElement).checked).toBe(false);
      expect((checkbox.element as HTMLInputElement).disabled).toBe(true);

      await wrapper.get('[data-testid="apply-dialog-rescan-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      expect((checkbox.element as HTMLInputElement).checked).toBe(true);
      expect((checkbox.element as HTMLInputElement).disabled).toBe(false);
    });

    it("shows an error message when the dialog's own Rescan fails", async () => {
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: {
          unscanned: { added: ["a.mod"], removed: [] },
          unapplied: { added: [], removed: [] },
        },
        rescan_project: () => {
          throw { code: "internal", message: "the scan task panicked" };
        },
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-rescan-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      expect(wrapper.get('[data-testid="apply-dialog-rescan-error"]').text()).toContain(
        "the scan task panicked",
      );
    });

    it("Rescan from a stale dialog, then Apply, writes ModsConfig.xml (exercises the mock's own stale_active_set refusal path)", async () => {
      let scanned = false;
      const applyCalls: unknown[] = [];
      installMockIpc({
        list_rules: EMPTY_RULE_SET,
        list_mod_names: {},
        get_pending_active_changes: () =>
          scanned
            ? EMPTY_PENDING_ACTIVE_CHANGES
            : {
                unscanned: { added: ["a.mod"], removed: [] },
                unapplied: { added: [], removed: [] },
              },
        rescan_project: () => {
          scanned = true;
          return {
            modCount: 1,
            gameVersion: "1.6",
            elapsedMs: 1,
            warnings: [],
            ruleWarnings: [],
            selected: "current",
          };
        },
        apply: (payload: unknown) => {
          const request = (payload as { request: { writeModsConfig: boolean } }).request;
          // Mirrors the real backend's stale-active-set refusal, the same way
          // `e2e/fixtures/scenario.ts`'s own `apply` mock does — proves
          // this flow reaches Apply only once genuinely un-stale.
          if (request.writeModsConfig && !scanned) {
            throw { code: "stale_active_set", message: "the working set is stale" };
          }
          applyCalls.push(request);
          return {
            modsConfigPath: "C:/RimWorld/ModsConfig.xml",
            backupPath: "C:/RimWorld/ModsConfig.xml.bak-2026-09-05T00-00-00Z",
            decisionsPath: "C:/Profile/rimmerge/decisions.json",
            rulesPath: "C:/Profile/rimmerge/rules.json",
            mergeModPath: null,
            mergeModBackupPath: null,
            skippedMerges: [],
          };
        },
        get_dashboard: dashboardFixture(0),
        get_apply_preflight: preflightFixture(),
        get_merge_mod: emptyMergeModFixture(),
        get_def_cache_carrier: { carrierModId: null },
        list_order: [],
      });
      const { wrapper } = mountDialog();
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-rescan-button"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
      await flush();
      await wrapper.vm.$nextTick();

      expect(applyCalls).toEqual([
        { source: "current", writeModsConfig: true, writeMergeMod: false, force: false },
      ]);
    });
  });
});
