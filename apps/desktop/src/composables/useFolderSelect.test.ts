import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";

import { modRow } from "@/components/mods/modListRow.test-support";
import { useFolderSelect } from "@/composables/useFolderSelect";
import { installMockIpc } from "@/services/ipc.mock";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { ModListDto } from "@/types/generated/ModListDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";

vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: vi.fn() }) }));

const WEAPONS: GroupNodeDto = {
  id: 4,
  name: "Weapons",
  color: null,
  parent: null,
  depth: 1,
  childCount: 0,
  directCount: 0,
  scopeCount: 0,
};

const PANE_QUERY: ModListQueryDto = { search: "gun", tag: null, source: "local", sort: null };

let wrappers: ReturnType<typeof mount>[] = [];

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
  clearMocks();
});

function setup(answer: ModListDto | Error, initial: string[] = []) {
  const requests: { command: string; query: unknown }[] = [];
  const handle = (command: string) => (payload: unknown) => {
    requests.push({ command, query: (payload as { query: unknown }).query });
    if (answer instanceof Error) {
      throw answer;
    }
    return answer;
  };
  installMockIpc({
    list_working_mods: handle("list_working_mods"),
    list_inactive_mods: handle("list_inactive_mods"),
  });
  const active = ref<string[]>(initial);
  const inactive = ref<string[]>([]);
  const announce = vi.fn(async () => {});
  let folderSelect!: ReturnType<typeof useFolderSelect>;
  const wrapper = mount(
    defineComponent({
      setup() {
        folderSelect = useFolderSelect({
          queries: { active: PANE_QUERY, inactive: PANE_QUERY },
          selections: { active, inactive },
          nodes: [WEAPONS],
          announce,
        });
        return () => null;
      },
    }),
    { global: { plugins: [createPinia(), PiniaColada] } },
  );
  wrappers.push(wrapper);
  return { folderSelect, active, inactive, announce, requests };
}

describe("useFolderSelect", () => {
  it("asks the pane's own query with the folder as its group filter, and adds what comes back", async () => {
    const answer: ModListDto = {
      total: 2,
      items: [modRow({ modId: "b" }), modRow({ modId: "c" })],
      reorderable: true,
    };
    const { folderSelect, active, requests, announce } = setup(answer, ["a", "b"]);

    await folderSelect.selectFolder("active", { kind: "group", groupId: 4 });
    await flushPromises();

    expect(requests).toEqual([
      {
        command: "list_working_mods",
        query: { ...PANE_QUERY, group: { kind: "inScope", groupId: 4 } },
      },
    ]);
    expect(active.value).toEqual(["a", "b", "c"]);
    expect(announce).toHaveBeenCalledWith("Selected 2 mods in Weapons.");
  });

  it("uses the Inactive command for the Inactive pane and leaves the other selection alone", async () => {
    const answer: ModListDto = { total: 1, items: [modRow({ modId: "x" })], reorderable: false };
    const { folderSelect, active, inactive, requests, announce } = setup(answer, ["keep"]);

    await folderSelect.selectFolder("inactive", { kind: "ungrouped" });

    expect(requests[0]).toEqual({
      command: "list_inactive_mods",
      query: { ...PANE_QUERY, group: { kind: "ungrouped" } },
    });
    expect(inactive.value).toEqual(["x"]);
    expect(active.value).toEqual(["keep"]);
    expect(announce).toHaveBeenCalledWith("Selected 1 mod in Ungrouped.");
  });

  it("says so, and changes nothing, for a folder with no mod under the filter", async () => {
    const { folderSelect, active, announce } = setup({ total: 0, items: [], reorderable: true }, [
      "a",
    ]);

    await folderSelect.selectFolder("active", { kind: "group", groupId: 4 });

    expect(active.value).toEqual(["a"]);
    expect(announce).toHaveBeenCalledWith("No mods to select in Weapons.");
  });

  it("keeps the selection when the query fails", async () => {
    const { folderSelect, active, announce } = setup(new Error("boom"), ["a"]);

    await folderSelect.selectFolder("active", { kind: "group", groupId: 4 });

    expect(active.value).toEqual(["a"]);
    expect(announce).not.toHaveBeenCalled();
  });

  it("asks nothing and selects nothing for a group the organization no longer has", async () => {
    const { folderSelect, active, announce, requests } = setup(
      { total: 1, items: [modRow({ modId: "z" })], reorderable: true },
      ["a"],
    );

    await folderSelect.selectFolder("active", { kind: "group", groupId: 99 });

    expect(requests).toEqual([]);
    expect(active.value).toEqual(["a"]);
    expect(announce).not.toHaveBeenCalled();
  });
});
