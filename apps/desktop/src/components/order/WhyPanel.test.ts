import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import WhyPanel from "@/components/order/WhyPanel.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { DashboardDto } from "@/types/generated/DashboardDto";
import type { PlacementExplanationDto } from "@/types/generated/PlacementExplanationDto";

const explanationFixture: PlacementExplanationDto = {
  modId: "addon.mod",
  position: 436,
  previousPosition: 412,
  tier: "body",
  tierReason: { kind: "body" },
  becameReadyAfter: {
    after: "addon.mod",
    before: "framework.mod",
    layer: "hard",
    kind: "assemblyRef",
    provenance: { kind: "engine" },
    detail: "AssemblyRef Foo.dll",
  },
  lowerBounds: [
    {
      after: "addon.mod",
      before: "framework.mod",
      layer: "hard",
      kind: "assemblyRef",
      provenance: { kind: "engine" },
      detail: "AssemblyRef Foo.dll",
    },
  ],
  upperBounds: [],
  dropped: [
    {
      edge: {
        after: "addon.mod",
        before: "other.mod",
        layer: "soft",
        kind: "assemblyRef",
        provenance: { kind: "engine" },
        detail: "AssemblyRef Bar.dll",
      },
      witnessCycle: ["addon.mod", "other.mod"],
      winner: {
        after: "other.mod",
        before: "addon.mod",
        layer: "hard",
        kind: "assemblyRef",
        provenance: { kind: "engine" },
        detail: "AssemblyRef Foo.dll",
      },
    },
  ],
  advisory: [
    {
      edge: {
        after: "addon.mod",
        before: "aware.mod",
        layer: "awareness",
        kind: "findMod",
        provenance: { kind: "engine" },
        detail: "FindMod Aware",
      },
      satisfied: false,
      strength: "awareness",
    },
  ],
  tieBreak: {
    currentPosition: 412,
    effectiveKey: 400,
    pulledForwardBy: null,
    modsPreferredAhead: 25,
  },
};

function buildRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", name: "dashboard", component: { template: "<div />" } },
      { path: "/inbox", name: "inbox", component: { template: "<div />" } },
      { path: "/order/:modId", name: "order-mod", component: { template: "<div />" } },
      { path: "/mods/:modId", name: "mod-detail", component: { template: "<div />" } },
    ],
  });
}

function mountWhyPanel(modId: string | null, router = buildRouter()) {
  return mount(WhyPanel, {
    props: { modId },
    global: {
      plugins: [createPinia(), PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }]],
      // PrimeVue's `Dialog` renders through a real `<Teleport>` to
      // `document.body`; stubbing it keeps the content inline so
      // `wrapper.get`/`.find` can reach it.
      stubs: { teleport: true },
    },
  });
}

describe("WhyPanel", () => {
  afterEach(() => {
    clearMocks();
  });

  it("stays closed and issues no query while modId is null", () => {
    installMockIpc({ explain_placement: explanationFixture, list_mod_names: {} });
    const wrapper = mountWhyPanel(null);

    expect(wrapper.find('[data-testid="why-panel-content"]').exists()).toBe(false);
  });

  it("renders every explanation section once loaded", async () => {
    installMockIpc({ explain_placement: explanationFixture, list_mod_names: {} });
    const wrapper = mountWhyPanel("addon.mod");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="why-panel-became-ready-after"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="why-panel-lower-bounds"]').text()).toContain("framework.mod");
    expect(wrapper.get('[data-testid="why-panel-dropped"]').text()).toContain("other.mod");
    expect(wrapper.get('[data-testid="why-panel-dropped"]').text()).toContain(
      "addon.mod → other.mod",
    );
    expect(wrapper.get('[data-testid="why-panel-dropped-winner"]').text()).toContain(
      "Overruled by",
    );
    expect(wrapper.get('[data-testid="why-panel-dropped-winner"]').text()).toContain("other.mod");
    expect(wrapper.get('[data-testid="why-panel-advisory"]').text()).toContain("aware.mod");
    expect(wrapper.get('[data-testid="advisory-violated"]').text()).toBe("violated");
    expect(wrapper.get('[data-testid="why-panel-tie-break"]').text()).toContain("#413");
    expect(wrapper.get('[data-testid="why-panel-tie-break"]').text()).toContain("25");
  });

  it("shows the sort settings that produced the suggested order, once the dashboard query resolves", async () => {
    // Typed as `DashboardDto`: `installMockIpc`'s own `IpcFixtures` is
    // `Record<string, unknown>`, so without this annotation a new DTO field
    // (an `EdgeKindCountsDto` count, say) could silently drift out of this
    // fixture unnoticed.
    const dashboardFixture: DashboardDto = {
      modCount: 1,
      edgesByStrength: { hard: 0, declared: 0, soft: 0, inferred: 0, awareness: 0 },
      edgesViolatedBySource: {
        assemblyRef: 0,
        forceLoadAfter: 0,
        forceLoadBefore: 0,
        loadAfter: 0,
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
        defOverride: 0,
        patchCollision: 0,
        textureOverride: 0,
        duplicateAssembly: 0,
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
        current: { auto: 0, needsInput: 0, overridden: 0, resolvedBySuggested: 0 },
        suggested: { auto: 0, needsInput: 0, overridden: 0, resolvedBySuggested: 0 },
      },
      needsInputByKind: {
        edgeDropped: 0,
        anyOfChoice: 0,
        defOverride: 0,
        patchCollision: 0,
        textureOverride: 0,
        duplicateAssembly: 0,
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
        // Every finding kind needs a count here — the `DashboardDto`
        // annotation above enforces it.
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
      movedMods: 0,
      selected: "suggested",
      fileMatchesSuggested: false,
      sortProvenance: {
        tieBreak: "preserveCurrent",
        useImportedPairs: true,
        useImportedPlacements: false,
      },
    };
    installMockIpc({
      explain_placement: explanationFixture,
      list_mod_names: {},
      get_dashboard: dashboardFixture,
    });
    const wrapper = mountWhyPanel("addon.mod");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    // Two separate assertions, not one exact concatenated string: the
    // heading and the summary are two separate block elements (an `h3`
    // and a `p`), and whether `textContent` joins them with or without a
    // space is an incidental artifact of how Vue's whitespace-condense
    // compiles static versus interpolated text, not anything this test
    // means to pin.
    const text = wrapper.get('[data-testid="why-panel-sort-settings"]').text();
    expect(text).toContain("Sort settings");
    expect(text).toContain(
      "tie-break: preserve current · imported pairs: on · imported placements: off",
    );
  });

  it("every edge chip links to the other mod and to the inbox", async () => {
    installMockIpc({ explain_placement: explanationFixture, list_mod_names: {} });
    const wrapper = mountWhyPanel("addon.mod");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    const link = wrapper.get('[data-testid="edge-chip-mod-link-framework.mod"]');
    expect(link.attributes("href")).toContain("/order/framework.mod");
    const findingLink = wrapper.get('[data-testid="edge-chip-finding-link-framework.mod"]');
    expect(findingLink.attributes("href")).toContain("/inbox");
  });

  it("emits close when the dialog is dismissed", async () => {
    installMockIpc({ explain_placement: explanationFixture, list_mod_names: {} });
    const wrapper = mountWhyPanel("addon.mod");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    await wrapper.get('button[aria-label="Close"]').trigger("click");
    expect(wrapper.emitted("close")).toHaveLength(1);
  });

  it("clicking the header's Open in Mods link navigates to the mod's info panel and closes the dialog", async () => {
    installMockIpc({ explain_placement: explanationFixture, list_mod_names: {} });
    const router = buildRouter();
    const wrapper = mountWhyPanel("addon.mod", router);
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="why-panel-open-in-mods"]').trigger("click");
    await flushPromises();

    expect(router.currentRoute.value.name).toBe("mod-detail");
    expect(router.currentRoute.value.params["modId"]).toBe("addon.mod");
    expect(wrapper.emitted("close")).toHaveLength(1);
  });

  it("shows no Open in Mods link while the explanation is still loading", () => {
    installMockIpc({
      explain_placement: () => new Promise(() => {}),
      list_mod_names: {},
    });
    const wrapper = mountWhyPanel("addon.mod");

    expect(wrapper.find('[data-testid="why-panel-open-in-mods"]').exists()).toBe(false);
  });
});
