import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, type Pinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import DashboardPage from "@/pages/DashboardPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { DashboardDto } from "@/types/generated/DashboardDto";

const router = createRouter({
  history: createMemoryHistory(),
  routes: [{ path: "/mods/:modId", name: "mod-detail", component: { template: "<div />" } }],
});

const dashboardFixture: DashboardDto = {
  modCount: 42,
  edgesByStrength: { hard: 10, declared: 20, soft: 5, inferred: 2, awareness: 3 },
  edgesViolatedBySource: {
    assemblyRef: 0,
    forceLoadAfter: 0,
    forceLoadBefore: 0,
    loadAfter: 1,
    loadBefore: 0,
    modDependency: 0,
    findMod: 0,
    ifModActive: 0,
    patchTargetsDef: 0,
    mayRequire: 0,
    patchInjectedNode: 0,
    assemblyVersionPrecedence: 0,
    usesType: 0,
    parentTemplate: 0,
    patchRemovedNode: 0,
    retextureAfterOwner: 0,
    defOverrideAfterOrigin: 0,
    patchSelectsInjectedNode: 0,
    patchInvalidatesPredicate: 0,
    patchRemovedNodeCosmetic: 0,
    replaceDiscardsAddition: 0,
  },
  conflictsByKind: {
    defOverride: 2,
    patchCollision: 0,
    textureOverride: 0,
    duplicateAssembly: 1,
    likelyDuplicateMod: 0,
    duplicateTemplateName: 0,
    keyedTranslationCollision: 0,
    soundOverride: 0,
    runtimePatchCollision: 0,
    transpilerCollision: 0,
    missingTexturePath: 0,
    undecodableTexture: 0,
    brokenInheritance: 0,
    nearMissModReference: 0,
    discardedAddition: 0,
    danglingDefReference: 0,
  },
  ledgerStats: {
    current: { auto: 5, needsInput: 3, overridden: 1, resolvedBySuggested: 2 },
    suggested: { auto: 7, needsInput: 1, overridden: 1, resolvedBySuggested: 0 },
  },
  needsInputByKind: {
    edgeDropped: 0,
    anyOfChoice: 0,
    defOverride: 2,
    patchCollision: 0,
    textureOverride: 0,
    duplicateAssembly: 1,
    likelyDuplicateMod: 0,
    missingMod: 0,
    missingDependency: 0,
    incompatiblePair: 0,
    unsupportedVersion: 0,
    undeclaredHardDependency: 0,
    lazyReferenceViolated: 0,
    declarationQuestioned: 0,
    declarationOverridden: 0,
    duplicateTemplateName: 0,
    keyedTranslationCollision: 0,
    soundOverride: 0,
    undeclaredTypeDependency: 0,
    runtimePatchCollision: 0,
    transpilerCollision: 0,
    tagInferred: 0,
    ruleOverruled: 0,
    placementOverruled: 0,
    placementQuestioned: 0,
    placementOrderingOverridden: 0,
    placementPromotesDependents: 0,
    missingTexturePath: 0,
    patchWillFail: 0,
    contributesNothing: 0,
    undecodableTexture: 0,
    brokenInheritance: 0,
    nearMissModReference: 0,
    discardedAddition: 0,
    danglingDefReference: 0,
  },
  movedMods: 12,
  selected: "current",
  fileMatchesSuggested: false,
  fileMatchesCurrent: true,
  sortProvenance: { tieBreak: "rebuild", useImportedPairs: false, useImportedPlacements: true },
};

function mountDashboard(pinia: Pinia = createPinia()) {
  return mount(DashboardPage, {
    global: {
      plugins: [pinia, router, PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      // DashboardPage mounts `ApplyDialog` (initially `visible: false`),
      // which renders a PrimeVue `Dialog` through a real `<Teleport>`.
      stubs: { teleport: true },
    },
  });
}

describe("DashboardPage", () => {
  afterEach(() => {
    clearMocks();
  });

  it("renders the dashboard's counts once the query resolves", async () => {
    installMockIpc({ get_dashboard: dashboardFixture });

    const wrapper = mountDashboard();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="mod-count"]').text()).toBe("42");
    expect(wrapper.get('[data-testid="needs-input-current"]').text()).toBe("3");
    expect(wrapper.get('[data-testid="needs-input-suggested"]').text()).toBe("1");
    expect(wrapper.get('[data-testid="moved-mods"]').text()).toBe("12");
    expect(wrapper.get('[data-testid="selected-order"]').text()).toBe("Current");
    expect(wrapper.get('[data-testid="moved-mods-provenance"]').text()).toBe(
      "tie-break: rebuild · imported pairs: off · imported placements: on",
    );
  });

  it("shows the top needs-input kinds, largest first", async () => {
    installMockIpc({ get_dashboard: dashboardFixture });

    const wrapper = mountDashboard();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    const items = wrapper.findAll('[data-testid="top-needs-input-kinds"] li');
    expect(items).toHaveLength(2);
    expect(items[0]?.text()).toContain("Def override");
    expect(items[0]?.text()).toContain("2");
    expect(items[1]?.text()).toContain("Duplicate assembly");
  });

  it("opens the guided strip on step one while Current is selected", async () => {
    installMockIpc({ get_dashboard: dashboardFixture });

    const wrapper = mountDashboard();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="guide-step-choose"]').attributes("aria-current")).toBe(
      "step",
    );
    expect(
      wrapper.get('[data-testid="guide-step-apply"]').attributes("aria-current"),
    ).toBeUndefined();
  });

  it("shows the needs-input count as information with an Inbox link, never as a gate", async () => {
    installMockIpc({ get_dashboard: dashboardFixture });

    const wrapper = mountDashboard();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="guide-needs-input"]').text()).toContain("3 findings");
    expect(wrapper.find('[data-testid="guide-inbox-link"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="apply-anyway-checkbox"]').exists()).toBe(false);
    expect(
      wrapper.get('[data-testid="dashboard-apply-button"]').attributes("disabled"),
    ).toBeUndefined();
  });

  it("renders an error message when the dashboard query fails", async () => {
    // Not `no_project_loaded`/`session_lost`: those two codes are
    // intercepted by `services/ipc.ts`'s global session-lost handler and
    // redirect to `/setup` in the real app (see `services/ipc.test.ts`),
    // so they'd never actually render here — this exercises the
    // dashboard's own error state for a failure that isn't one of those.
    installMockIpc({
      get_dashboard: () => {
        throw { code: "internal", message: "background task panicked" };
      },
    });

    const wrapper = mountDashboard();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="dashboard-error"]').exists()).toBe(true);
  });

  it("shows a Scan notes card naming the note count when the session has scan notes", async () => {
    installMockIpc({ get_dashboard: dashboardFixture });
    const pinia = createPinia();
    useSessionStore(pinia).setScanNotes([
      { modId: "some.mod", message: "Languages/English/Keyed/keys.xml: skipped: bad xml" },
      { modId: "other.mod", message: "Defs/Buildings.xml: skipped: bad xml" },
    ]);

    const wrapper = mountDashboard(pinia);
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    const disclosure = wrapper.get('[data-testid="scan-notes-disclosure"]');
    expect(disclosure.text()).toContain("Scan notes (2)");
    expect(wrapper.findAll('[data-testid="scan-note"]')).toHaveLength(2);
  });

  it("shows no Scan notes card when the session has no scan notes", async () => {
    installMockIpc({ get_dashboard: dashboardFixture });

    const wrapper = mountDashboard();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="scan-notes-disclosure"]').exists()).toBe(false);
  });
});
