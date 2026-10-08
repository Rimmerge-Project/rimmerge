// ApplyDialog: the merge mod is read only while a dialog is open (gated by mounting, not by an
// `enabled` option), refreshes on a session change, and a missing or replaced answer is shown as
// loading, never as "nothing to merge".

import { PiniaColada, useQueryCache } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { emit } from "@tauri-apps/api/event";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, type Pinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h, reactive } from "vue";
import { createMemoryHistory, createRouter } from "vue-router";
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
import ApplyDialog from "@/components/apply/ApplyDialog.vue";
import { useSessionEvents } from "@/composables/useSessionEvents";
import MergeModPage from "@/pages/MergeModPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { MergeModDto } from "@/types/generated/MergeModDto";

const STATUS = '[data-testid="apply-dialog-merge-mod-status"]';
const CHECKBOX = '[data-testid="apply-dialog-write-merge-mod-checkbox"]';

/** A `get_merge_mod` whose answers the test releases by hand, counting every call. */
function deferredMergeMod() {
  let calls = 0;
  const releases: Array<(mergeMod: MergeModDto) => void> = [];
  return {
    handler: () => {
      calls += 1;
      return new Promise<MergeModDto>((resolve) => {
        releases.push(resolve);
      });
    },
    /** Answers every call still waiting. */
    releaseAll: (mergeMod: MergeModDto) => {
      for (const release of releases.splice(0)) {
        release(mergeMod);
      }
    },
    callCount: () => calls,
  };
}

function installHandlers(getMergeMod: () => unknown, extra: Record<string, unknown> = {}): void {
  installMockIpc({
    list_rules: EMPTY_RULE_SET,
    list_mod_names: {},
    get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
    get_dashboard: dashboardFixture(0),
    get_apply_preflight: preflightFixture(),
    get_merge_mod: getMergeMod,
    get_def_cache_carrier: { carrierModId: null },
    list_order: [],
    ...extra,
  });
}

type HostEntry = { page: "mergeMod" } | { dialog: boolean };

/**
 * Several hosts on one app, as in the real shell, in mount order. A dialog entry's `dialog` is
 * its `visible`, switchable later through `setVisible(index, value)`. `useSessionEvents` is
 * mounted once, like the shell does.
 */
function mountHosts(layout: readonly HostEntry[]) {
  const pinia = createPinia();
  setActivePinia(pinia);
  useSessionStore().setLoaded(
    {
      gameDir: "C:/RimWorld",
      workshopDir: "C:/RimWorld/workshop",
      modsConfig: "C:/RimWorld/ModsConfig.xml",
      profileDir: "C:/Profile/rimmerge",
    },
    "current",
  );
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/:rest(.*)*", component: { render: () => null } }],
  });
  const entries = reactive(layout.map((entry) => ({ ...entry })));
  const host = defineComponent({
    setup() {
      useSessionEvents();
      return () =>
        entries.map((entry, index) =>
          "page" in entry
            ? h(MergeModPage, { key: index })
            : h(ApplyDialog, { key: index, visible: entry.dialog }),
        );
    },
  });
  const wrapper = mount(host, {
    global: {
      plugins: [pinia, router, PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      stubs: { teleport: true },
    },
  });
  return {
    wrapper,
    pinia,
    setVisible: (index: number, visible: boolean) => {
      const entry = entries[index];
      if (entry && "dialog" in entry) {
        entry.dialog = visible;
      }
    },
  };
}

async function sessionChanged(): Promise<void> {
  await emit("session://changed", { reason: "ruleChanged" });
  await flush();
  await flush();
}

/**
 * What `useSessionEvents` does on `session://changed`, for a refetch that is expected to fail:
 * the shell's own `void invalidateQueries()` leaves that rejection unhandled (a console error in
 * the app), which Vitest reports as a failure, so the test awaits it and swallows it instead.
 */
async function invalidateAllExpectingFailure(pinia: Pinia): Promise<void> {
  await useQueryCache(pinia)
    .invalidateQueries()
    .catch(() => undefined);
  await flush();
}

describe("ApplyDialog merge mod", () => {
  afterEach(() => {
    clearMocks();
    vi.useRealTimers();
  });

  it("does not read the merge mod while hidden, and reads it once when it opens", async () => {
    const mergeMod = deferredMergeMod();
    installHandlers(mergeMod.handler);
    const { wrapper } = mountDialog({ visible: false });
    await flush();
    expect(mergeMod.callCount()).toBe(0);

    await wrapper.setProps({ visible: true });
    await flush();

    expect(mergeMod.callCount()).toBe(1);
  });

  it("makes no request for any of several hidden dialogs, even on a session change", async () => {
    const mergeMod = deferredMergeMod();
    installHandlers(mergeMod.handler);
    mountHosts([{ dialog: false }, { dialog: false }, { dialog: false }]);
    await flush();

    await sessionChanged();

    expect(mergeMod.callCount()).toBe(0);
  });

  it("refreshes an open dialog on a session change even when a hidden dialog mounted after it", async () => {
    const mergeMod = deferredMergeMod();
    installHandlers(mergeMod.handler);
    mountHosts([{ dialog: true }, { dialog: false }]);
    await flush();
    mergeMod.releaseAll(emptyMergeModFixture());
    await flush();
    expect(mergeMod.callCount()).toBe(1);

    await sessionChanged();

    expect(mergeMod.callCount()).toBe(2);
  });

  it("refreshes the Merge mod page on a session change even when a hidden dialog mounted after it", async () => {
    const mergeMod = deferredMergeMod();
    installHandlers(mergeMod.handler);
    mountHosts([{ page: "mergeMod" }, { dialog: false }]);
    await flush();
    mergeMod.releaseAll(emptyMergeModFixture());
    await flush();
    expect(mergeMod.callCount()).toBe(1);

    await sessionChanged();

    expect(mergeMod.callCount()).toBe(2);
  });

  it("shows the merge section as loading, with the write box disabled and described by it, until the answer arrives", async () => {
    const mergeMod = deferredMergeMod();
    installHandlers(mergeMod.handler);
    const { wrapper } = mountDialog();
    await flush();

    const status = wrapper.get(STATUS);
    expect(status.text()).toBe("Loading…");
    expect(status.attributes("role")).toBe("status");
    const input = wrapper.get(CHECKBOX).find("input");
    expect(input.attributes("disabled")).toBeDefined();
    expect(input.attributes("aria-describedby")).toBe(status.attributes("id"));

    mergeMod.releaseAll(
      mergeModFixtureWithEntry({ kind: "complete", opCount: 2 }, { kind: "defOverride" }),
    );
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get(STATUS).text()).toContain("1 def-override merge");
    expect(wrapper.get(STATUS).attributes("role")).toBe("status");
    expect(wrapper.get(CHECKBOX).find("input").attributes("disabled")).toBeUndefined();
    expect(wrapper.get(CHECKBOX).find("input").attributes("aria-describedby")).toBeUndefined();
  });

  it("settles to no summary and an enabled box when the merge mod has nothing to write", async () => {
    installHandlers(() => emptyMergeModFixture());
    const { wrapper } = mountDialog();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get(STATUS).text()).toBe("");
    expect(wrapper.get(CHECKBOX).find("input").attributes("disabled")).toBeUndefined();
  });

  it("shows loading again on reopening after a session change, not the old session's answer", async () => {
    const mergeMod = deferredMergeMod();
    installHandlers(mergeMod.handler);
    const { wrapper, setVisible } = mountHosts([{ dialog: true }]);
    await flush();
    mergeMod.releaseAll(
      mergeModFixtureWithEntry({ kind: "complete", opCount: 2 }, { kind: "defOverride" }),
    );
    await flush();
    await wrapper.vm.$nextTick();
    expect(wrapper.get(STATUS).text()).toContain("def-override merge");

    // The session is swapped while the dialog is closed: the cache keeps the old answer (stale,
    // still `success`), which must not read as current when the dialog reopens.
    setVisible(0, false);
    await flush();
    await flush();
    await sessionChanged();
    setVisible(0, true);
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get(STATUS).text()).toBe("Loading…");
    expect(wrapper.get(CHECKBOX).find("input").attributes("disabled")).toBeDefined();
    expect(mergeMod.callCount()).toBe(2);
  });

  it("reuses the last answer on reopening when the session has not changed", async () => {
    const mergeMod = deferredMergeMod();
    installHandlers(mergeMod.handler);
    const { wrapper, setVisible } = mountHosts([{ dialog: true }]);
    await flush();
    mergeMod.releaseAll(
      mergeModFixtureWithEntry({ kind: "complete", opCount: 2 }, { kind: "defOverride" }),
    );
    await flush();
    await wrapper.vm.$nextTick();

    // Only the clock moves: far past any default staleTime, so a refetch here could only be the
    // query's own freshness policy, not the session.
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(Date.now() + 24 * 60 * 60 * 1000);
    setVisible(0, false);
    await flush();
    setVisible(0, true);
    await flush();
    await wrapper.vm.$nextTick();

    expect(mergeMod.callCount()).toBe(1);
    expect(wrapper.get(STATUS).text()).toContain("def-override merge");
  });

  it("shows no summary, not the replaced session's, when the refetch after a session change fails", async () => {
    let failing = false;
    const answer = mergeModFixtureWithEntry(
      { kind: "complete", opCount: 2 },
      { kind: "defOverride" },
    );
    installHandlers(() => {
      if (failing) {
        throw new Error("render failed");
      }
      return answer;
    });
    const { wrapper, pinia } = mountHosts([{ dialog: true }]);
    await flush();
    await wrapper.vm.$nextTick();
    expect(wrapper.get(STATUS).text()).toContain("def-override merge");

    failing = true;
    await invalidateAllExpectingFailure(pinia);
    await wrapper.vm.$nextTick();

    expect(wrapper.get(STATUS).text()).toBe("");
    expect(wrapper.get(CHECKBOX).find("input").attributes("disabled")).toBeUndefined();
  });

  it("explains a skipped merge in plain wording, not from the replaced session's entries, when the refetch fails", async () => {
    const guardedKey = "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]";
    let failing = false;
    const answer = mergeModFixtureWithEntry(
      { kind: "needsFieldInput", unresolved: 9, total: 9 },
      { key: guardedKey, structuralGuardField: "ParentName" },
    );
    installHandlers(
      () => {
        if (failing) {
          throw new Error("render failed");
        }
        return answer;
      },
      {
        apply: () => ({
          modsConfigPath: "C:/RimWorld/ModsConfig.xml",
          backupPath: "C:/RimWorld/ModsConfig.xml.bak-x",
          decisionsPath: "C:/Profile/rimmerge/decisions.json",
          rulesPath: "C:/Profile/rimmerge/rules.json",
          mergeModPath: null,
          mergeModBackupPath: null,
          skippedMerges: [guardedKey],
        }),
      },
    );
    const { wrapper, pinia } = mountHosts([{ dialog: true }]);
    await flush();
    await wrapper.vm.$nextTick();
    await wrapper.get(CHECKBOX).find("input").setValue(true);
    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();
    const skipped = wrapper.get('[data-testid="apply-dialog-skipped-merges"]');
    expect(skipped.text()).toContain("not auto-merged — confirm the winner");

    failing = true;
    await invalidateAllExpectingFailure(pinia);
    await wrapper.vm.$nextTick();

    const afterFailure = wrapper.get('[data-testid="apply-dialog-skipped-merges"]');
    expect(afterFailure.text()).toContain(guardedKey);
    expect(afterFailure.text()).not.toContain("not auto-merged — confirm the winner");
  });
});
