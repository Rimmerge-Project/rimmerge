import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import ModInfoFacts from "@/components/mods/ModInfoFacts.vue";
import { createAppI18n, registerAppI18n } from "@/i18n/i18n";
import { installMockIpc } from "@/services/ipc.mock";
import type { ActiveModInfoDto } from "@/types/generated/ActiveModInfoDto";
import type { InactiveModInfoDto } from "@/types/generated/InactiveModInfoDto";

registerAppI18n(createAppI18n());

const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: "/", name: "dashboard", component: { template: "<div />" } },
    { path: "/order/:modId?", name: "order-mod", component: { template: "<div />" } },
    { path: "/inbox", name: "inbox", component: { template: "<div />" } },
    { path: "/startup", name: "startup", component: { template: "<div />" } },
  ],
});

const declared = {
  loadAfter: [],
  loadBefore: [],
  forceLoadAfter: [],
  forceLoadBefore: [],
  dependencies: [],
  incompatibleWith: [],
};

function activeInfo(overrides: Partial<ActiveModInfoDto> = {}): ActiveModInfoDto {
  return {
    modId: "fixture.mod",
    name: "Fixture Mod",
    authors: [],
    homepage: null,
    source: "local",
    supportedVersions: [],
    supportsGameVersion: true,
    declared,
    root: "Mods/fixture",
    loadedFolders: [],
    cost: null,
    tags: [],
    hardDependents: 0,
    softDependents: 0,
    awarenessDependents: 0,
    isFrameworkCandidate: false,
    generated: null,
    workshopId: null,
    position: 4,
    previousPosition: null,
    tier: "body",
    findingsTotal: 0,
    needsInputCount: 0,
    pending: null,
    about: { kind: "notOnDisk" },
    ...overrides,
  };
}

function inactiveInfo(overrides: Partial<InactiveModInfoDto> = {}): InactiveModInfoDto {
  return {
    modId: "fixture.mod",
    name: "Fixture Mod",
    authors: [],
    source: "local",
    supportedVersions: [],
    declared,
    root: "Mods/fixture",
    workshopId: null,
    generated: null,
    pending: null,
    about: { kind: "notOnDisk" },
    ...overrides,
  };
}

async function mountFacts(info: ActiveModInfoDto | InactiveModInfoDto) {
  installMockIpc({ list_mod_names: {} });
  const wrapper = mount(ModInfoFacts, {
    props: { info },
    global: { plugins: [createPinia(), PiniaColada, router] },
  });
  await router.isReady();
  return wrapper;
}

describe("ModInfoFacts", () => {
  afterEach(() => {
    clearMocks();
  });

  it("shows placement, cost, dependents and findings sections only for an active mod", async () => {
    const wrapper = await mountFacts(activeInfo());

    expect(wrapper.find('[data-testid="mod-info-facts-placement"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="mod-info-facts-cost"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="mod-info-facts-dependents"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="mod-info-facts-findings"]').exists()).toBe(true);
  });

  it("hides placement, cost, dependents and findings sections for an inactive mod", async () => {
    const wrapper = await mountFacts(inactiveInfo());

    expect(wrapper.find('[data-testid="mod-info-facts-placement"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="mod-info-facts-cost"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="mod-info-facts-dependents"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="mod-info-facts-findings"]').exists()).toBe(false);
  });

  it("shows the 1-based position and tier for an active mod", async () => {
    const wrapper = await mountFacts(activeInfo({ position: 0 }));

    expect(wrapper.get('[data-testid="mod-info-facts-placement"]').text()).toContain("#1");
  });

  it("shows the framework-candidate note only when set", async () => {
    const candidate = await mountFacts(activeInfo({ isFrameworkCandidate: true }));
    expect(candidate.find('[data-testid="mod-info-framework-candidate"]').exists()).toBe(true);

    const notCandidate = await mountFacts(activeInfo({ isFrameworkCandidate: false }));
    expect(notCandidate.find('[data-testid="mod-info-framework-candidate"]').exists()).toBe(false);
  });

  it("clicking the findings link navigates to /inbox with the mod's own ?mod= filter", async () => {
    const wrapper = await mountFacts(activeInfo({ modId: "fixture.mod", findingsTotal: 3 }));

    await wrapper.get('[data-testid="mod-info-findings-link"]').trigger("click");
    await flushPromises();

    expect(router.currentRoute.value.path).toBe("/inbox");
    expect(router.currentRoute.value.query["mod"]).toBe("fixture.mod");
  });

  it("shows the generated-marker section only when the mod carries one", async () => {
    const merge = await mountFacts(
      activeInfo({ generated: { kind: "merge", patchId: null, scope: null } }),
    );
    expect(merge.find('[data-testid="mod-info-facts-generated"]').exists()).toBe(true);

    const noMarker = await mountFacts(activeInfo({ generated: null }));
    expect(noMarker.find('[data-testid="mod-info-facts-generated"]').exists()).toBe(false);
  });

  it("renders each declared-order field through the mod-label composable, falling back to a dependency's own displayName", async () => {
    const wrapper = await mountFacts(
      activeInfo({
        declared: {
          loadAfter: ["other.mod"],
          loadBefore: [],
          forceLoadAfter: [],
          forceLoadBefore: [],
          dependencies: [{ id: "unresolved.dep", displayName: "Unresolved Dependency" }],
          incompatibleWith: [],
        },
      }),
    );

    const declaredSection = wrapper.get('[data-testid="mod-info-facts-declared"]');
    expect(declaredSection.text()).toContain("other.mod");
    expect(declaredSection.text()).toContain("Unresolved Dependency");
  });
});
