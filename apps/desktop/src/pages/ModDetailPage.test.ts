import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import ModDetailPage from "@/pages/ModDetailPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { ModDetailDto } from "@/types/generated/ModDetailDto";

afterEach(() => clearMocks());

function modDetailFixture(overrides: Partial<ModDetailDto> = {}): ModDetailDto {
  return {
    modId: "some.mod",
    name: "Some Mod",
    authors: [],
    url: null,
    source: "workshop",
    supportedVersions: [],
    declared: {
      dependencies: [],
      loadAfter: [],
      loadBefore: [],
      forceLoadAfter: [],
      forceLoadBefore: [],
      incompatibleWith: [],
    },
    tags: [],
    hardDependents: 0,
    isFrameworkCandidate: false,
    generated: null,
    workshopId: null,
    edgesIn: [],
    edgesOut: [],
    findingKeys: [],
    findingKeysTotal: 0,
    ...overrides,
  };
}

async function mountModDetailPage() {
  installMockIpc({
    get_mod: () => modDetailFixture(),
    list_mod_names: {},
  });

  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/mods/:modId", name: "mod-detail", component: ModDetailPage }],
  });
  const pinia = createPinia();

  await router.push({ name: "mod-detail", params: { modId: "some.mod" } });
  await router.isReady();

  const wrapper = mount(
    { template: "<router-view />" },
    { global: { plugins: [pinia, PiniaColada, router] } },
  );
  await flushPromises();

  return { wrapper, pinia };
}

describe("ModDetailPage", () => {
  it("shows only this mod's scan notes, filtered from the session's full list", async () => {
    const { wrapper, pinia } = await mountModDetailPage();
    useSessionStore(pinia).setScanNotes([
      { modId: "some.mod", message: "Defs/Buildings.xml: skipped: bad xml" },
      { modId: "other.mod", message: "Defs/Weapons.xml: skipped: bad xml" },
    ]);
    await wrapper.vm.$nextTick();

    const notes = wrapper.findAll('[data-testid="scan-note"]');
    expect(notes).toHaveLength(1);
    expect(notes[0]?.text()).toContain("Defs/Buildings.xml");
  });

  it("shows the empty state when this mod has no scan notes", async () => {
    const { wrapper } = await mountModDetailPage();

    expect(wrapper.find('[data-testid="scan-note-list-empty"]').exists()).toBe(true);
  });
});
