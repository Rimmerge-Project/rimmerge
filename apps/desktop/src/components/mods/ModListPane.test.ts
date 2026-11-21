import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import ModGroupsTable from "@/components/mods/ModGroupsTable.vue";
import ModListPane from "@/components/mods/ModListPane.vue";
import { modRow } from "@/components/mods/modListRow.test-support";
import { installMockIpc } from "@/services/ipc.mock";
import type { GroupedModListDto } from "@/types/generated/GroupedModListDto";
import type { ModListDto } from "@/types/generated/ModListDto";

const LIST_AB: ModListDto = {
  total: 2,
  items: [modRow({ modId: "a" }), modRow({ modId: "b" })],
  reorderable: false,
};
const LIST_A: ModListDto = { total: 1, items: [modRow({ modId: "a" })], reorderable: false };
const GROUPED: GroupedModListDto = { ...LIST_AB, entries: [], totalEntries: 0 };

let wrappers: ReturnType<typeof mount>[] = [];

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
  clearMocks();
});

function mountPane(props: Partial<InstanceType<typeof ModListPane>["$props"]> = {}) {
  installMockIpc({ list_mod_names: {} });
  const selection: { current: string[] } = { current: ["a", "b"] };
  const wrapper = mount(ModListPane, {
    props: {
      kind: "inactive",
      modList: LIST_AB,
      isPending: false,
      isPlaceholder: false,
      error: null,
      isFiltered: false,
      sortKey: "loadOrder",
      selectedIds: selection.current,
      "onUpdate:selectedIds": (ids: string[]) => {
        selection.current = ids;
        void wrapper.setProps({ selectedIds: ids });
      },
      ...props,
    } as never,
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { ModTable: true, ModGroupsTable: true, teleport: true },
    },
  });
  wrappers.push(wrapper);
  return { wrapper, selection };
}

describe("ModListPane selection pruning", () => {
  it("drops a selected id the unfiltered answer does not hold", async () => {
    const { wrapper, selection } = mountPane();

    await wrapper.setProps({ modList: LIST_A });
    await flushPromises();

    expect(selection.current).toEqual(["a"]);
  });

  it("keeps hidden selected ids under a filter", async () => {
    const { wrapper, selection } = mountPane({ isFiltered: true });

    await wrapper.setProps({ modList: LIST_A });
    await flushPromises();

    expect(selection.current).toEqual(["a", "b"]);
  });

  it("waits for the real answer while the rows are the previous query's placeholder", async () => {
    // A filter was cleared: the unfiltered key is not cached, so the filtered answer ({a}) stays on
    // screen as a placeholder while `isFiltered` is already false.
    const { wrapper, selection } = mountPane({
      isFiltered: true,
      modList: LIST_A,
      isPlaceholder: false,
    });
    await wrapper.setProps({ isFiltered: false, isPlaceholder: true });
    await flushPromises();
    expect(selection.current).toEqual(["a", "b"]);

    // The real unfiltered answer lands without `b`: now it is gone, not hidden.
    await wrapper.setProps({ modList: LIST_A, isPlaceholder: false });
    await flushPromises();
    expect(selection.current).toEqual(["a"]);
  });

  it("keeps both ids when the real answer still holds them", async () => {
    const { wrapper, selection } = mountPane({ isFiltered: true, modList: LIST_A });
    await wrapper.setProps({ isFiltered: false, isPlaceholder: true });
    await wrapper.setProps({ modList: LIST_AB, isPlaceholder: false });
    await flushPromises();

    expect(selection.current).toEqual(["a", "b"]);
  });
});

describe("ModListPane Groups view re-emits", () => {
  function mountGrouped() {
    const { wrapper } = mountPane({ grouped: GROUPED, modList: GROUPED });
    return { wrapper, table: wrapper.getComponent(ModGroupsTable) };
  }

  const GROUP_FOLDER = { kind: "group", groupId: 3 } as const;

  it.each([
    ["selectMod", ["a"]],
    ["focusMod", ["a"]],
    ["dragStart", ["a"]],
    ["dragEnd", []],
    ["drop", [4]],
    ["moveStep", ["a", "up"]],
    ["menuAction", ["a", "groups"]],
    ["toggleActive", ["a"]],
    ["setColor", ["a", "teal"]],
    ["scopeGroup", [7]],
    ["setFolded", [{ kind: "ungrouped" }, true]],
    ["dropOnFolder", [7]],
    ["folderAction", [GROUP_FOLDER, "activate"]],
  ] as const)("passes the table's %s on with the same payload", (event, payload) => {
    const { wrapper, table } = mountGrouped();

    table.vm.$emit(event, ...payload);

    expect(wrapper.emitted(event)).toEqual([[...payload]]);
  });
});
