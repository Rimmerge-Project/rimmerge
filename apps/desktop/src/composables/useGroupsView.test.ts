import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { computed, defineComponent, ref } from "vue";

import { useGroupsView } from "@/composables/useGroupsView";
import { installMockIpc } from "@/services/ipc.mock";
import { usePreferencesStore } from "@/stores/preferences";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";

const QUERY: ModListQueryDto = { search: null, tag: null, source: null, sort: null };
const LIST = { total: 0, items: [], reorderable: true };
const GROUPED = { total: 0, items: [], reorderable: false, entries: [], totalEntries: 0 };

let wrappers: ReturnType<typeof mount>[] = [];

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
  clearMocks();
});

/** The page's wiring with the organization's state set by the test (`isKnownUnusable`, `isUsable`). */
function mountView() {
  installMockIpc({
    list_working_mods: LIST,
    list_inactive_mods: LIST,
    list_working_mods_grouped: GROUPED,
    list_inactive_mods_grouped: GROUPED,
  });
  const isUsable = ref(true);
  const isKnownUnusable = ref(false);
  let view: ReturnType<typeof useGroupsView> | null = null;
  let preferences: ReturnType<typeof usePreferencesStore> | null = null;
  const wrapper = mount(
    defineComponent({
      setup() {
        preferences = usePreferencesStore();
        view = useGroupsView({
          groups: {
            isUsable: computed(() => isUsable.value),
            isKnownUnusable: computed(() => isKnownUnusable.value),
            nodes: computed(() => []),
            dropScopeIfStale: () => undefined,
          },
          queries: { active: QUERY, inactive: QUERY },
          selections: { active: ref([]), inactive: ref([]) },
          announce: async () => undefined,
          activateGroup: async () => undefined,
          deactivateGroup: async () => undefined,
        });
        return () => null;
      },
    }),
    { global: { plugins: [createPinia(), PiniaColada, [PrimeVue, {}], ToastService] } },
  );
  wrappers.push(wrapper);
  if (view === null || preferences === null) {
    throw new Error("the composable did not run");
  }
  return {
    view: view as ReturnType<typeof useGroupsView>,
    preferences: preferences as ReturnType<typeof usePreferencesStore>,
    isUsable,
    isKnownUnusable,
  };
}

describe("useGroupsView shown view", () => {
  it("shows List while the organization is unusable, keeps the remembered Groups, and brings it back", async () => {
    const pane = mountView();
    pane.view.setActiveView("groups");
    await flushPromises();
    expect(pane.view.activeView.value).toBe("groups");

    pane.isKnownUnusable.value = true;
    pane.isUsable.value = false;
    await flushPromises();
    expect(pane.view.activeView.value).toBe("list");
    expect(pane.preferences.panes.active.view).toBe("groups");
    expect(JSON.parse(localStorage.getItem("rimmerge.modsPanes.v1") ?? "{}").active.view).toBe(
      "groups",
    );

    pane.isKnownUnusable.value = false;
    pane.isUsable.value = true;
    await flushPromises();
    expect(pane.view.activeView.value).toBe("groups");
  });

  it("keeps the remembered Groups view while the organization is still loading", async () => {
    localStorage.setItem(
      "rimmerge.modsPanes.v1",
      JSON.stringify({
        active: { sort: { key: "loadOrder", direction: "ascending" }, view: "groups" },
        inactive: { sort: { key: "name", direction: "ascending" }, view: "list" },
      }),
    );
    const pane = mountView();
    // Not usable yet, but not known to be unusable either: no answer has come.
    pane.isUsable.value = false;
    pane.isKnownUnusable.value = false;
    await flushPromises();

    expect(pane.view.activeView.value).toBe("groups");
    expect(pane.view.inactiveView.value).toBe("list");
  });

  it("a choice made while the organization is unusable is not shown, but is remembered", async () => {
    const pane = mountView();
    pane.isKnownUnusable.value = true;
    pane.view.setInactiveView("groups");
    await flushPromises();

    expect(pane.view.inactiveView.value).toBe("list");
    expect(pane.preferences.panes.inactive.view).toBe("groups");
  });
});
