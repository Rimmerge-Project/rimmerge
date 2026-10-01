import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import MergeEditorPage from "@/pages/MergeEditorPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { asFindingKey } from "@/types/brands";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import type { MergeFieldFilterDto } from "@/types/generated/MergeFieldFilterDto";
import type { MergePreviewDto } from "@/types/generated/MergePreviewDto";
import type { MergePreviewRequestDto } from "@/types/generated/MergePreviewRequestDto";

const KEY = asFindingKey("def_override:ThingDef/Wall:[a,b]");
const CONFLICT_COUNT = 2;
const UNCHANGED_COUNT = 250;

function conflictField(index: number): MergeFieldDto {
  return {
    path: `conflict-${index}`,
    depth: 0,
    isListItem: false,
    entry: "leaf",
    container: null,
    class: "conflict",
    changedBy: ["a.mod", "b.mod"],
    confidence: 50,
    base: "base",
    candidates: { "a.mod": "a-value" },
    result: null,
    choice: null,
    preselected: null,
  };
}

function unchangedField(index: number): MergeFieldDto {
  return {
    path: `unchanged-${index}`,
    depth: 0,
    isListItem: false,
    entry: "leaf",
    container: null,
    class: "unchanged",
    changedBy: [],
    confidence: 0,
    base: "base",
    candidates: {},
    result: "base",
    choice: null,
    preselected: null,
  };
}

const CONFLICT_FIELDS = Array.from({ length: CONFLICT_COUNT }, (_, i) => conflictField(i));
const UNCHANGED_FIELDS = Array.from({ length: UNCHANGED_COUNT }, (_, i) => unchangedField(i));
const ALL_FIELDS = [...CONFLICT_FIELDS, ...UNCHANGED_FIELDS];

function preview(filter: MergeFieldFilterDto): MergePreviewDto {
  const matching = filter.onlyConflicts ? CONFLICT_FIELDS : ALL_FIELDS;
  return {
    key: KEY,
    defKey: { defType: "ThingDef", defName: "Wall" },
    kind: "defOverride",
    owners: [
      { modId: "a.mod", name: "Mod A", position: 0 },
      { modId: "b.mod", name: "Mod B", position: 1 },
    ],
    base: "a.mod",
    winner: "a.mod",
    totals: {
      fields: ALL_FIELDS.length,
      unresolved: CONFLICT_COUNT,
      conflicts: CONFLICT_COUNT,
      auto: 0,
      unchanged: UNCHANGED_COUNT,
    },
    state: { kind: "needsFieldInput", unresolved: CONFLICT_COUNT, total: ALL_FIELDS.length },
    structuralGuard: null,
    caveats: [],
    fields: matching.slice(filter.offset, filter.offset + filter.limit),
    total: matching.length,
    resolvedXml: null,
    outOfScopeOwners: [],
    defRef: "ThingDef/Wall",
  };
}

function mountPage() {
  const requests: MergeFieldFilterDto[] = [];
  installMockIpc({
    get_merge_preview: (payload: unknown) => {
      const request = (payload as { request: MergePreviewRequestDto }).request;
      requests.push(request.filter);
      return preview(request.filter);
    },
    // `useModLabel` (via `MergeHeader`/`MergeFieldTable`/`MergeFieldRow`)
    // always queries this — see the identical note in `InboxPage.test.ts`.
    list_mod_names: {},
  });

  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/inbox", name: "inbox", component: { template: "<div />" } },
      { path: "/setup", name: "setup", component: { template: "<div />" } },
      { path: "/merge/:key", name: "merge-editor", component: MergeEditorPage },
      { path: "/defs/:defRef", name: "def", component: { template: "<div />" } },
    ],
  });

  const pinia = createPinia();
  setActivePinia(pinia);

  return { router, pinia, requests };
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("MergeEditorPage", () => {
  // `@tanstack/vue-virtual` (used by `MergeFieldTable`) measures its
  // scroll container via `offsetWidth`/`offsetHeight`, which happy-dom
  // always reports as `0` — stubbed so it renders every row instead of an
  // empty visible range (see `MergeFieldTable.test.ts`).
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      value: 800,
    });
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, value: 600 });
  });

  afterEach(() => {
    clearMocks();
    Reflect.deleteProperty(HTMLElement.prototype, "offsetHeight");
    Reflect.deleteProperty(HTMLElement.prototype, "offsetWidth");
  });

  it("shows the server-computed section totals immediately and pages in the rest via Show more", async () => {
    const { router, pinia, requests } = mountPage();
    await router.push({ name: "merge-editor", params: { key: KEY } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      {
        global: { plugins: [pinia, PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }]] },
      },
    );
    await flush();
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    // The unchanged section's header count is the server total (250),
    // even though only one 200-row page of the `onlyConflicts: false`
    // query has landed so far.
    expect(wrapper.get('[data-testid="merge-section-unchanged"]').text()).toContain(
      "Unchanged (250)",
    );
    expect(wrapper.get('[data-testid="merge-section-conflicts"]').text()).toContain(
      "Conflicts (2)",
    );

    const showMore = wrapper.get('[data-testid="merge-show-more"]');
    expect(showMore.text()).toContain("200 of 252");

    await showMore.trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(requests.some((f) => !f.onlyConflicts && f.offset === 200)).toBe(true);
    expect(wrapper.find('[data-testid="merge-show-more"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="merge-section-unchanged"]').text()).toContain(
      "Unchanged (250)",
    );

    wrapper.unmount();
  });

  // -- keyed-map container collapse --

  const WILD_ANIMALS_KEY = asFindingKey(
    "patch_collision:BiomeDef/AridShrubland:defName:wildAnimals:[a.mod,b.mod,c.mod]",
  );
  const WILD_ANIMALS_AUTO_COUNT = 13;

  function wildAnimalsMapEntry(index: number): MergeFieldDto {
    return {
      path: `wildAnimals/Key${index}`,
      depth: 1,
      isListItem: false,
      entry: "mapEntry",
      container: "wildAnimals",
      class: "oneSided",
      changedBy: ["a.mod"],
      confidence: 95,
      base: null,
      candidates: { "a.mod": "0.2" },
      result: "0.2",
      choice: null,
      preselected: null,
    };
  }

  const WILD_ANIMALS_CONFLICT: MergeFieldDto = {
    path: "label",
    depth: 0,
    isListItem: false,
    entry: "leaf",
    container: null,
    class: "conflict",
    changedBy: ["a.mod", "b.mod"],
    confidence: 0,
    base: "shrubland",
    candidates: { "a.mod": "dry shrubland", "b.mod": "arid shrubland" },
    result: null,
    choice: null,
    preselected: "b.mod",
  };
  const WILD_ANIMALS_AUTO_FIELDS = Array.from({ length: WILD_ANIMALS_AUTO_COUNT }, (_, i) =>
    wildAnimalsMapEntry(i),
  );
  const WILD_ANIMALS_ALL_FIELDS = [WILD_ANIMALS_CONFLICT, ...WILD_ANIMALS_AUTO_FIELDS];

  function wildAnimalsPreview(filter: MergeFieldFilterDto): MergePreviewDto {
    const matching = filter.onlyConflicts ? [WILD_ANIMALS_CONFLICT] : WILD_ANIMALS_ALL_FIELDS;
    return {
      key: WILD_ANIMALS_KEY,
      defKey: { defType: "BiomeDef", defName: "AridShrubland" },
      kind: "patchCollision",
      owners: [
        { modId: "a.mod", name: "Mod A", position: 0 },
        { modId: "b.mod", name: "Mod B", position: 1 },
        { modId: "c.mod", name: "Mod C", position: 2 },
      ],
      base: "a.mod",
      winner: "b.mod",
      totals: {
        fields: WILD_ANIMALS_ALL_FIELDS.length,
        unresolved: 1,
        conflicts: 1,
        auto: WILD_ANIMALS_AUTO_COUNT,
        unchanged: 0,
      },
      state: { kind: "needsFieldInput", unresolved: 1, total: WILD_ANIMALS_ALL_FIELDS.length },
      structuralGuard: null,
      caveats: [],
      fields: matching.slice(filter.offset, filter.offset + filter.limit),
      total: matching.length,
      resolvedXml: null,
      outOfScopeOwners: [],
      defRef: "BiomeDef/AridShrubland",
    };
  }

  function mountWildAnimalsPage() {
    installMockIpc({
      get_merge_preview: (payload: unknown) => {
        const request = (payload as { request: MergePreviewRequestDto }).request;
        return wildAnimalsPreview(request.filter);
      },
      list_mod_names: {},
    });

    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: "/merge/:key", name: "merge-editor", component: MergeEditorPage },
        { path: "/inbox", name: "inbox", component: { template: "<div />" } },
        { path: "/defs/:defRef", name: "def", component: { template: "<div />" } },
      ],
    });
    const pinia = createPinia();
    setActivePinia(pinia);
    return { router, pinia };
  }

  it("a large, fully-auto-resolved container collapses by default and expands on click, without touching the always-open conflict row", async () => {
    const { router, pinia } = mountWildAnimalsPage();
    await router.push({ name: "merge-editor", params: { key: WILD_ANIMALS_KEY } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      {
        global: { plugins: [pinia, PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }]] },
      },
    );
    await flush();
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    // The one genuine conflict is always visible, regardless of the
    // container's own collapse state.
    expect(wrapper.text()).toContain("label");

    await wrapper.get('[data-testid="merge-section-auto"]').trigger("click");
    await wrapper.vm.$nextTick();

    const header = wrapper.get('[data-testid="merge-container-auto-wildAnimals"]');
    expect(header.text()).toContain("wildAnimals — 13 entries");
    expect(wrapper.text()).not.toContain("wildAnimals/Key0");

    await header.trigger("click");
    await wrapper.vm.$nextTick();

    // The virtualizer only renders however many rows fit the mocked
    // viewport height, so this asserts the now-visible first entry rather
    // than every one of the 13 (a full-page render is `MergeFieldTable.test.ts`'s
    // own, non-virtualized-router-page concern).
    expect(wrapper.text()).toContain("wildAnimals/Key0");
    expect(wrapper.findAll('[data-testid="merge-field-row"]').length).toBeGreaterThan(1);

    wrapper.unmount();
  });
});
