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
function setup(selected: string[]) {
  installMockIpc({ list_mod_names: {} });
  const active = modList([
    modRow({ modId: "a", position: 0 }),
    modRow({ modId: "b", position: 1 }),
    modRow({ modId: "c", position: 2 }),
  ]);
  let reorder!: ReturnType<typeof useModReorder>;
  const Host = defineComponent({
    setup() {
      reorder = useModReorder({
        activeList: active,
        inactiveList: undefined,
        isActiveFiltered: true,
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
