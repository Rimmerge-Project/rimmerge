import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";

import { modList, modRow } from "@/components/mods/modListRow.test-support";
import { useModReorder } from "@/composables/useModReorder";
import { installMockIpc } from "@/services/ipc.mock";

vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: vi.fn() }) }));

/** An Active list showing a, b and c, with `selected` selected (some may be hidden by a filter). */
function setup(
  selected: string[],
  {
    isActiveGrouped = false,
    reorderable = true,
  }: { isActiveGrouped?: boolean; reorderable?: boolean } = {},
) {
  installMockIpc({ list_mod_names: {} });
  const active = {
    ...modList([
      modRow({ modId: "a", position: 0 }),
      modRow({ modId: "b", position: 1 }),
      modRow({ modId: "c", position: 2 }),
    ]),
    reorderable,
  };
  let reorder!: ReturnType<typeof useModReorder>;
  const Host = defineComponent({
    setup() {
      reorder = useModReorder({
        activeList: active,
        inactiveList: undefined,
        isActiveFiltered: true,
        isActiveGrouped,
        selectedActiveIds: ref(selected),
        selectedInactiveIds: ref([]),
        activation: { activateAt: async () => {} },
        deactivation: { deactivateDropped: async () => {} },
        focusActiveRow: async () => {},
        onSwitchedToCurrent: () => {},
      });
      return () => null;
    },
  });
  mount(Host, { global: { plugins: [createPinia(), PiniaColada] } });
  return reorder;
}

afterEach(() => clearMocks());

describe("useModReorder drag state", () => {
  it("counts the carried selected rows that the filter hides", () => {
    const reorder = setup(["a", "b", "hidden.1", "hidden.2"]);

    reorder.startDrag("active", "a");

    expect(reorder.drag.value?.ids).toEqual(
      expect.arrayContaining(["a", "b", "hidden.1", "hidden.2"]),
    );
    expect(reorder.drag.value?.hiddenCount).toBe(2);
  });

  it("has no hidden rows when every carried row is shown, or the dragged row is unselected", () => {
    const reorder = setup(["b", "hidden.1"]);

    reorder.startDrag("active", "c");

    expect(reorder.drag.value?.ids).toEqual(["c"]);
    expect(reorder.drag.value?.hiddenCount).toBe(0);
  });
});

describe("useModReorder in Groups view", () => {
  it("says moving is off because of the view, not the sort", () => {
    const reorder = setup(["a"], { isActiveGrouped: true, reorderable: false });

    expect(reorder.reorderBlockedReason.value).toContain("Groups view");
  });

  it("keeps the sort wording for a list that is only sorted another way", () => {
    const reorder = setup(["a"], { isActiveGrouped: false, reorderable: false });

    expect(reorder.reorderBlockedReason.value).toContain("sorted by load order");
  });

  it("refuses a drag from Active with the Groups view reason, and takes one from Inactive as a pane drop", () => {
    const reorder = setup(["a"], { isActiveGrouped: true, reorderable: false });

    reorder.startDrag("active", "a");
    expect(reorder.dropModeFor("active")).toBe("refused");
    expect(reorder.refusalFor("active")).toContain("Groups view");
    reorder.endDrag();

    reorder.drag.value = { from: "inactive", ids: ["x"], includesCore: false, hiddenCount: 0 };
    expect(reorder.dropModeFor("active")).toBe("pane");
  });

  it("has no reason while the list is reorderable", () => {
    const reorder = setup(["a"], { isActiveGrouped: false, reorderable: true });

    expect(reorder.reorderBlockedReason.value).toBeNull();
  });
});
