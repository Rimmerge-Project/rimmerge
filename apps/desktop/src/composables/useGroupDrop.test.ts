import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";

import { useGroupDrop } from "@/composables/useGroupDrop";
import type { DragState } from "@/composables/useModReorder";
import { installMockIpc } from "@/services/ipc.mock";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";

const toastAddSpy = vi.hoisted(() => vi.fn());
vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: toastAddSpy }) }));

const NODES: GroupNodeDto[] = [
  {
    id: 1,
    name: "Weapons",
    color: null,
    parent: null,
    depth: 1,
    childCount: 0,
    directCount: 0,
    scopeCount: 0,
  },
];

function drag(ids: string[], hiddenCount: number): DragState {
  return { from: "active", ids, includesCore: false, hiddenCount };
}

function setup(state: DragState | null) {
  const announced: string[] = [];
  let ended = 0;
  let drop!: ReturnType<typeof useGroupDrop>;
  const Host = defineComponent({
    setup() {
      drop = useGroupDrop({
        nodes: NODES,
        drag: state,
        endDrag: () => {
          ended += 1;
        },
        announce: async (text) => {
          announced.push(text);
        },
      });
      return () => null;
    },
  });
  const wrapper = mount(Host, { global: { plugins: [createPinia(), PiniaColada] } });
  return { drop, announced, ended: () => ended, wrapper };
}

afterEach(() => {
  clearMocks();
  toastAddSpy.mockClear();
});

describe("useGroupDrop", () => {
  it("adds every carried id, disclosing the ones already in and the ones a filter hides", async () => {
    const requests: unknown[] = [];
    installMockIpc({
      add_group_members: (payload: unknown) => {
        requests.push(payload);
        return { kind: "added", added: 2, already: 1 };
      },
      get_organization: { kind: "loaded", groups: NODES, coloredModCount: 0 },
    });
    const { drop, announced, ended } = setup(drag(["a", "b", "c"], 2));

    await drop.dropMods(1);
    await flushPromises();

    expect(requests).toEqual([{ request: { id: 1, modIds: ["a", "b", "c"] } }]);
    expect(toastAddSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        summary: "Added 2 mods to Weapons",
        detail: "1 was already in the group\n2 selected mods that a filter hides were included",
      }),
    );
    expect(announced).toHaveLength(1);
    expect(ended()).toBe(1);
  });

  it("says so when every mod was already a member", async () => {
    installMockIpc({
      add_group_members: () => ({ kind: "added", added: 0, already: 1 }),
      get_organization: { kind: "loaded", groups: NODES, coloredModCount: 0 },
    });
    const { drop } = setup(drag(["a"], 0));

    await drop.dropMods(1);
    await flushPromises();

    expect(toastAddSpy).toHaveBeenCalledWith(
      expect.objectContaining({ summary: "Every mod was already in Weapons" }),
    );
  });

  it("sends nothing when no mod drag is in progress", async () => {
    const requests: unknown[] = [];
    installMockIpc({
      add_group_members: (payload: unknown) => {
        requests.push(payload);
        return { kind: "added", added: 1, already: 0 };
      },
    });
    const { drop } = setup(null);

    await drop.dropMods(1);

    expect(requests).toEqual([]);
    expect(toastAddSpy).not.toHaveBeenCalled();
  });

  it("leaves a refused add to the app-wide error toast, with no success message", async () => {
    installMockIpc({
      add_group_members: () => {
        throw { code: "organization_limit", message: "too many" };
      },
    });
    const { drop } = setup(drag(["a"], 0));

    await drop.dropMods(1);

    expect(toastAddSpy).not.toHaveBeenCalled();
  });
});
