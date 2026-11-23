import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it, vi } from "vitest";

import OrganizationDialogs from "@/components/mods/organization/OrganizationDialogs.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import { filterGroupsByName, newGroupNameFor, type OrganizationDialog } from "@/utils/organization";

/**
 * Creating a group from "Add to group": the search field filters the list, a name no group has
 * offers "Create group" (with an optional parent), and the create-and-add is one
 * `create_group` call carrying the mods. The pure helpers behind it are covered first.
 */

const toastAdd = vi.hoisted(() => vi.fn());
vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: toastAdd }) }));

function node(id: number, name: string, parent: number | null = null, depth = 1): GroupNodeDto {
  return {
    id,
    name,
    color: null,
    parent,
    depth,
    childCount: 0,
    directCount: 0,
    scopeCount: 0,
  };
}

const NODES = [node(1, "Weapons"), node(2, "Melee", 1, 2), node(3, "Armor")];

describe("group search helpers", () => {
  it("filters by a trimmed, case-insensitive part of the name and keeps tree order", () => {
    expect(filterGroupsByName(NODES, "  ME ").map((entry) => entry.id)).toEqual([2]);
    expect(filterGroupsByName(NODES, "a").map((entry) => entry.id)).toEqual([1, 3]);
  });

  it("returns every group for an empty query", () => {
    expect(filterGroupsByName(NODES, "   ")).toBe(NODES);
  });

  it("offers the trimmed query as a new name only when no group has that name", () => {
    expect(newGroupNameFor(NODES, "  Loot  ")).toBe("Loot");
    expect(newGroupNameFor(NODES, "armor")).toBeNull();
    expect(newGroupNameFor(NODES, "Arm")).toBe("Arm");
    expect(newGroupNameFor(NODES, "   ")).toBeNull();
  });
});

interface Call {
  command: string;
  payload: unknown;
}

let wrappers: ReturnType<typeof mount>[] = [];
const calls: Call[] = [];
type Handler = (payload: unknown) => unknown;

function mountDialogs(initial: OrganizationDialog, handlers: Record<string, Handler> = {}) {
  calls.length = 0;
  installMockIpc({
    get_organization: { kind: "loaded", groups: NODES, coloredModCount: 0 },
    list_working_mods: { items: [], total: 0, reorderable: true },
    list_inactive_mods: { items: [], total: 0, reorderable: false },
    list_mod_names: {},
    ...Object.fromEntries(
      Object.entries(handlers).map(([command, handler]) => [
        command,
        (payload: unknown) => {
          calls.push({ command, payload });
          return handler(payload);
        },
      ]),
    ),
  });
  const wrapper = mount(OrganizationDialogs, {
    props: {
      nodes: NODES,
      dialog: initial,
      "onUpdate:dialog": (value: OrganizationDialog | null) => {
        void wrapper.setProps({ dialog: value });
      },
    },
    attachTo: document.body,
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true },
    },
  });
  wrappers.push(wrapper);
  return wrapper;
}

function currentDialog(wrapper: ReturnType<typeof mount>): OrganizationDialog | null {
  return (wrapper.props() as { dialog: OrganizationDialog | null }).dialog;
}

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
  clearMocks();
  toastAdd.mockClear();
});

const BULK: OrganizationDialog = {
  kind: "bulkGroups",
  mode: "add",
  modIds: ["a", "b", "c"],
  hiddenCount: 0,
};

describe("bulk Add to group: search and create", () => {
  it("filters the group list as you type and says when nothing matches", async () => {
    const wrapper = mountDialogs(BULK);
    await flushPromises();

    await wrapper.get('[data-testid="group-search-input"]').setValue("arm");
    expect(wrapper.find('[data-testid="bulk-target-3"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="bulk-target-1"]').exists()).toBe(false);

    await wrapper.get('[data-testid="group-search-input"]').setValue("zzz");
    expect(wrapper.get('[data-testid="bulk-groups-no-match"]').text()).toBe(
      "No group matches your search.",
    );
  });

  it("offers Create only for a name no group has, and not for an exact match in any letter case", async () => {
    const wrapper = mountDialogs(BULK);
    await flushPromises();

    await wrapper.get('[data-testid="group-search-input"]').setValue("Loot");
    expect(wrapper.get('[data-testid="group-create-button"]').text()).toContain(
      "Create group “Loot”",
    );

    await wrapper.get('[data-testid="group-search-input"]').setValue("WEAPONS");
    expect(wrapper.find('[data-testid="group-create-option"]').exists()).toBe(false);
  });

  it("creates the group and adds every selected mod in one call, then closes", async () => {
    // The backend counts mods, not ids: it answers 2 for the 3 ids sent (two are copies of one mod).
    const wrapper = mountDialogs(BULK, {
      create_group: () => ({ kind: "created", id: 9, members: 2 }),
    });
    await flushPromises();

    await wrapper.get('[data-testid="group-search-input"]').setValue("  Loot ");
    await wrapper.get('[data-testid="group-create-parent"]').setValue("1");
    await wrapper.get('[data-testid="group-create-button"]').trigger("click");
    await flushPromises();

    expect(calls.filter((call) => call.command === "create_group")).toEqual([
      {
        command: "create_group",
        payload: { request: { name: "Loot", parent: 1, modIds: ["a", "b", "c"] } },
      },
    ]);
    expect(calls.some((call) => call.command === "add_group_members")).toBe(false);
    expect(toastAdd.mock.calls[0]?.[0]).toMatchObject({
      summary: "Created Loot and added 2 mods to it",
    });
    expect(currentDialog(wrapper)).toBeNull();
  });

  it("creates at the top level by default", async () => {
    const wrapper = mountDialogs(BULK, {
      create_group: () => ({ kind: "created", id: 9, members: 1 }),
    });
    await flushPromises();

    await wrapper.get('[data-testid="group-search-input"]').setValue("Loot");
    await wrapper.get('[data-testid="group-create-button"]').trigger("click");
    await flushPromises();

    expect(calls.find((call) => call.command === "create_group")?.payload).toEqual({
      request: { name: "Loot", parent: null, modIds: ["a", "b", "c"] },
    });
  });

  it("keeps the dialog open with the refusal when the backend refuses, and toasts nothing", async () => {
    const wrapper = mountDialogs(BULK, {
      create_group: () => {
        throw { code: "organization_limit", message: "too many" };
      },
    });
    await flushPromises();

    await wrapper.get('[data-testid="group-search-input"]').setValue("Loot");
    await wrapper.get('[data-testid="group-create-button"]').trigger("click");
    await flushPromises();

    expect(currentDialog(wrapper)?.kind).toBe("bulkGroups");
    expect(wrapper.find('[data-testid="organization-error"]').exists()).toBe(true);
    expect(toastAdd).not.toHaveBeenCalled();
  });

  it("drops a chosen target that the search then hides", async () => {
    const wrapper = mountDialogs(BULK);
    await flushPromises();

    await wrapper.get('[data-testid="bulk-target-1"]').setValue(true);
    await wrapper.get('[data-testid="group-search-input"]').setValue("arm");
    await flushPromises();

    expect(wrapper.get('[data-testid="bulk-groups-submit"]').attributes("disabled")).toBeDefined();
  });
});

describe("a single mod's Groups dialog: search and create", () => {
  const SINGLE: OrganizationDialog = { kind: "modGroups", modId: "a", groupIds: [3] };

  it("creates the group with this mod in it and shows it checked, keeping the dialog open", async () => {
    const wrapper = mountDialogs(SINGLE, {
      create_group: () => ({ kind: "created", id: 9, members: 1 }),
    });
    await flushPromises();

    await wrapper.get('[data-testid="group-search-input"]').setValue("Loot");
    await wrapper.get('[data-testid="group-create-button"]').trigger("click");
    await flushPromises();

    expect(calls.find((call) => call.command === "create_group")?.payload).toEqual({
      request: { name: "Loot", parent: null, modIds: ["a"] },
    });
    expect(currentDialog(wrapper)).toEqual({ kind: "modGroups", modId: "a", groupIds: [3, 9] });
    expect(toastAdd.mock.calls[0]?.[0]).toMatchObject({
      summary: "Created Loot and added 1 mod to it",
    });
  });

  it("filters the checkbox list by the search", async () => {
    const wrapper = mountDialogs(SINGLE);
    await flushPromises();

    await wrapper.get('[data-testid="group-search-input"]').setValue("melee");

    expect(wrapper.find('[data-testid="mod-groups-toggle-2"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="mod-groups-toggle-1"]').exists()).toBe(false);
  });

  it("shows a refusal and leaves the checked groups as they were", async () => {
    const wrapper = mountDialogs(SINGLE, {
      create_group: () => {
        throw { code: "group_name_taken", message: "taken" };
      },
    });
    await flushPromises();

    await wrapper.get('[data-testid="group-search-input"]').setValue("Loot");
    await wrapper.get('[data-testid="group-create-button"]').trigger("click");
    await flushPromises();

    expect(currentDialog(wrapper)).toEqual({ kind: "modGroups", modId: "a", groupIds: [3] });
    expect(wrapper.find('[data-testid="organization-error"]').exists()).toBe(true);
  });
});
