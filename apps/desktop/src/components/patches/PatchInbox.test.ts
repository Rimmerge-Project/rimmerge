import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import PatchInbox from "@/components/patches/PatchInbox.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useInboxStore } from "@/stores/inbox";
import { usePatchStore } from "@/stores/patch";
import type { DecidePatchRequestDto } from "@/types/generated/DecidePatchRequestDto";
import type { ResolutionDetailDto } from "@/types/generated/ResolutionDetailDto";
import type { ResolutionSummaryDto } from "@/types/generated/ResolutionSummaryDto";

function summary(key: string): ResolutionSummaryDto {
  return {
    key,
    finding: { kind: "missingMod", modId: `${key}.mod` },
    status: "needsInput",
    confidence: 50,
    effective: { kind: "ignore" },
    hasDecision: false,
    mergeState: null,
    structuralGuardField: null,
    scope: { kind: "full" },
    defRef: null,
  };
}

function detailFor(key: string): ResolutionDetailDto {
  return {
    key,
    finding: { kind: "missingMod", modId: `${key}.mod` },
    suggestion: {
      action: { kind: "ignore" },
      confidence: 50,
      rationale: "Not addressed by this patch; load order decides as today.",
      rationaleCode: { kind: "notAddressedByPatch" },
      alternatives: [],
    },
    status: "needsInput",
    effective: { kind: "ignore" },
    note: null,
    hasDecision: false,
    resolvedBySuggested: null,
    mergeState: null,
    structuralGuardField: null,
    scope: { kind: "full" },
    defRef: null,
  };
}

const STATS = { auto: 0, needsInput: 1, overridden: 0, resolvedBySuggested: 0 };

function mountPatchInbox(pinia: ReturnType<typeof createPinia>, patchId = "patch1") {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: { template: "<div />" } },
      { path: "/order/:modId", name: "order-mod", component: { template: "<div />" } },
      { path: "/mods/:modId", name: "mod-detail", component: { template: "<div />" } },
      {
        path: "/patches/:patchId/merge/:key",
        name: "patch-merge-editor",
        component: { template: "<div />" },
      },
    ],
  });

  return mount(PatchInbox, {
    props: { patchId },
    global: {
      plugins: [pinia, PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      stubs: { teleport: true },
    },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("PatchInbox", () => {
  afterEach(() => {
    clearMocks();
  });

  it("wires decide/revert to decide_patch/revert_patch_decision, folding in the patch id", async () => {
    const decideCalls: DecidePatchRequestDto[] = [];
    installMockIpc({
      list_patch_findings: () => ({ total: 1, items: [summary("a")] }),
      get_patch_finding: () => detailFor("a"),
      list_mod_names: {},
      decide_patch: (payload: unknown) => {
        decideCalls.push((payload as { request: DecidePatchRequestDto }).request);
        return { stats: STATS, resorted: false, movedMods: 0 };
      },
    });

    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mountPatchInbox(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="accept-button"]').trigger("click");
    await flush();

    expect(decideCalls).toEqual([
      { patchId: "patch1", key: "a", action: { kind: "ignore" }, note: null },
    ]);
    wrapper.unmount();
  });

  it("wires revert to revert_patch_decision with the patch id", async () => {
    const revertCalls: { patchId: string; key: string }[] = [];
    installMockIpc({
      list_patch_findings: () => ({ total: 1, items: [{ ...summary("a"), hasDecision: true }] }),
      get_patch_finding: () => ({ ...detailFor("a"), hasDecision: true }),
      list_mod_names: {},
      revert_patch_decision: (payload: unknown) => {
        revertCalls.push(payload as { patchId: string; key: string });
        return { stats: STATS, resorted: false, movedMods: 0 };
      },
    });

    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mountPatchInbox(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="revert-button"]').trigger("click");
    await flush();

    expect(revertCalls).toEqual([{ patchId: "patch1", key: "a" }]);
    wrapper.unmount();
  });

  it("keeps its own cursor/filter store separate from the profile inbox's", async () => {
    installMockIpc({
      list_patch_findings: () => ({ total: 1, items: [summary("a")] }),
      get_patch_finding: () => detailFor("a"),
      list_mod_names: {},
    });

    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mountPatchInbox(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    const patchStore = usePatchStore();
    const inboxStore = useInboxStore();
    expect(patchStore.filter.status).toBe("needsInput");

    await wrapper.get('[data-testid="status-chip-auto"]').trigger("click");
    expect(patchStore.filter.status).toBe("auto");
    expect(inboxStore.filter.status).toBe("needsInput");
    wrapper.unmount();
  });

  it("emits escape on Escape instead of navigating anywhere", async () => {
    installMockIpc({
      list_patch_findings: () => ({ total: 1, items: [summary("a")] }),
      get_patch_finding: () => detailFor("a"),
      list_mod_names: {},
    });

    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mountPatchInbox(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    document.body.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", cancelable: true, bubbles: true }),
    );

    expect(wrapper.emitted("escape")).toHaveLength(1);
    wrapper.unmount();
  });
});
