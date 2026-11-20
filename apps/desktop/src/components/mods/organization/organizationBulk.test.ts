import { PiniaColada, useQueryCache } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import Dialog from "primevue/dialog";
import { afterEach, describe, expect, it, vi } from "vitest";
import BulkGroupsDialog from "@/components/mods/organization/BulkGroupsDialog.vue";

import OrganizationDialogs from "@/components/mods/organization/OrganizationDialogs.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { GroupMembershipCountDto } from "@/types/generated/GroupMembershipCountDto";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { OrganizationDialog } from "@/utils/organization";

/**
 * The bulk group and colour dialogs, run through the host: each acts on exactly the selected ids
 * (hidden ones included), says how many mods that is, reports the backend's counts one number per
 * line, and shows a refusal inside the dialog. Moving needs a chosen source and refuses the same
 * group as target; remove and move list only groups that hold a selected mod.
 */

const toastAdd = vi.hoisted(() => vi.fn());
vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: toastAdd }) }));

function node(id: number, name: string): GroupNodeDto {
  return {
    id,
    name,
    color: null,
    parent: null,
    depth: 1,
    childCount: 0,
    directCount: 0,
    scopeCount: 0,
  };
}

const NODES = [node(1, "Weapons"), node(2, "Armor"), node(3, "Empty")];
const IDS = ["a", "b", "c"];

interface Call {
  command: string;
  payload: unknown;
}

let wrappers: ReturnType<typeof mount>[] = [];
const calls: Call[] = [];
type Handler = (payload: unknown) => unknown;

/** What the count request answers: rows, a failure, never, or whatever a test's own function does. */
type CountsSource =
  | GroupMembershipCountDto[]
  | Error
  | "never"
  | ((request: { modIds: string[] }) => Promise<GroupMembershipCountDto[]>);

function mountDialogs(
  initial: OrganizationDialog,
  counts: CountsSource,
  handlers: Record<string, Handler> = {},
) {
  calls.length = 0;
  installMockIpc({
    get_organization: { kind: "loaded", groups: NODES, coloredModCount: 0 },
    list_working_mods: { items: [], total: 0, reorderable: true },
    list_inactive_mods: { items: [], total: 0, reorderable: false },
    group_membership_counts: (payload: unknown) => {
      calls.push({ command: "group_membership_counts", payload });
      if (counts instanceof Error) {
        throw { code: "internal", message: "boom" };
      }
      if (counts === "never") {
        return new Promise(() => {});
      }
      if (typeof counts === "function") {
        return counts((payload as { request: { modIds: string[] } }).request);
      }
      return counts;
    },
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

/** Every line the toasts said, in order: each toast's summary, then its detail lines if it has any. */
function summaries(): string[] {
  return toastAdd.mock.calls.flatMap(([toast]) => {
    const { summary, detail } = toast as { summary: string; detail?: string };
    return detail === undefined ? [summary] : [summary, ...detail.split("\n")];
  });
}

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
  clearMocks();
  toastAdd.mockClear();
});

describe("bulk add to group", () => {
  it("adds exactly the selected ids, hidden ones included, and reports the counts separately", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "add", modIds: IDS, hiddenCount: 1 },
      [],
      { add_group_members: () => ({ kind: "added", added: 2, already: 1 }) },
    );
    await flushPromises();

    expect(wrapper.get('[data-testid="bulk-groups-selection"]').text()).toBe(
      "3 selected (1 not shown)",
    );
    await wrapper.get('[data-testid="bulk-target-1"]').setValue(true);
    await wrapper.get('[data-testid="bulk-groups-submit"]').trigger("click");
    await flushPromises();

    expect(calls.filter((call) => call.command === "add_group_members")).toEqual([
      { command: "add_group_members", payload: { request: { id: 1, modIds: IDS } } },
    ]);
    expect(summaries()).toEqual(["Added 2 mods to Weapons", "1 was already in the group"]);
    expect(currentDialog(wrapper)).toBeNull();
  });

  it("does not ask which groups hold the mods, since adding needs no source", async () => {
    mountDialogs({ kind: "bulkGroups", mode: "add", modIds: IDS, hiddenCount: 0 }, []);
    await flushPromises();

    expect(calls.some((call) => call.command === "group_membership_counts")).toBe(false);
  });

  it("keeps the dialog open and shows the refusal when the backend refuses", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "add", modIds: IDS, hiddenCount: 0 },
      [],
      {
        add_group_members: () => {
          throw { code: "organization_limit", message: "too many" };
        },
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="bulk-target-1"]').setValue(true);
    await wrapper.get('[data-testid="bulk-groups-submit"]').trigger("click");
    await flushPromises();

    expect(currentDialog(wrapper)?.kind).toBe("bulkGroups");
    expect(wrapper.find('[data-testid="organization-error"]').exists()).toBe(true);
    expect(summaries()).toEqual([]);
  });
});

describe("bulk remove from group", () => {
  it("lists only the groups that hold a selected mod, with how many", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "remove", modIds: IDS, hiddenCount: 0 },
      [{ id: 2, count: 2 }],
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="bulk-target-1"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="bulk-target-3"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="bulk-target-count-2"]').text()).toBe(
      "2 selected mods are in it",
    );
  });

  it("removes the selected ids from the chosen group", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "remove", modIds: IDS, hiddenCount: 0 },
      [{ id: 2, count: 2 }],
      { remove_group_members: () => ({ kind: "removed", removed: 2, absent: 1 }) },
    );
    await flushPromises();

    await wrapper.get('[data-testid="bulk-target-2"]').setValue(true);
    await wrapper.get('[data-testid="bulk-groups-submit"]').trigger("click");
    await flushPromises();

    expect(calls.filter((call) => call.command === "remove_group_members")).toEqual([
      { command: "remove_group_members", payload: { request: { id: 2, modIds: IDS } } },
    ]);
    expect(summaries()).toEqual(["Removed 2 mods from Armor", "1 was not in the group"]);
  });

  it("says when none of the selected mods are in a group", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "remove", modIds: IDS, hiddenCount: 0 },
      [],
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="bulk-groups-none-held"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="bulk-groups-submit"]').attributes("disabled")).toBeDefined();
  });

  it("never reads a failed count request as 'in no group'", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "remove", modIds: IDS, hiddenCount: 0 },
      new Error("down"),
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="bulk-groups-failed"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="bulk-groups-none-held"]').exists()).toBe(false);
  });
});

describe("bulk move to group", () => {
  it("moves from the chosen source to the chosen target and reports each count on its own", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "move", modIds: IDS, hiddenCount: 0 },
      [{ id: 1, count: 3 }],
      {
        move_group_members: () => ({
          kind: "moved",
          moved: 1,
          alreadyInTarget: 1,
          notInSource: 1,
        }),
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="bulk-source-1"]').setValue(true);
    await wrapper.get('[data-testid="bulk-target-2"]').setValue(true);
    await wrapper.get('[data-testid="bulk-groups-submit"]').trigger("click");
    await flushPromises();

    expect(calls.filter((call) => call.command === "move_group_members")).toEqual([
      {
        command: "move_group_members",
        payload: { request: { from: 1, to: 2, modIds: IDS } },
      },
    ]);
    expect(summaries()).toEqual([
      "Moved 2 mods from Weapons to Armor",
      "1 was not in Weapons, so it did not move",
      "1 was already in Armor",
    ]);
    // One toast for the one action, with the extra sentences as its detail lines.
    expect(toastAdd).toHaveBeenCalledTimes(1);
  });

  it("lists as sources only groups that hold a selected mod", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "move", modIds: IDS, hiddenCount: 0 },
      [{ id: 1, count: 3 }],
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="bulk-source-1"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="bulk-source-2"]').exists()).toBe(false);
  });

  it("disables the source as a target, with the reason in words", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "move", modIds: IDS, hiddenCount: 0 },
      [{ id: 1, count: 3 }],
    );
    await flushPromises();

    await wrapper.get('[data-testid="bulk-source-1"]').setValue(true);

    expect(wrapper.get('[data-testid="bulk-target-1"]').attributes("disabled")).toBeDefined();
    expect(wrapper.get('[data-testid="bulk-target-reason-1"]').text()).toBe("Same as the source");
  });

  it("drops a target that becomes the source, so the same group is never submitted", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "move", modIds: IDS, hiddenCount: 0 },
      [{ id: 1, count: 3 }],
    );
    await flushPromises();
    await wrapper.get('[data-testid="bulk-target-1"]').setValue(true);

    await wrapper.get('[data-testid="bulk-source-1"]').setValue(true);

    expect(wrapper.get('[data-testid="bulk-groups-submit"]').attributes("disabled")).toBeDefined();
  });

  it("cannot submit until a source and a target are chosen", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "move", modIds: IDS, hiddenCount: 0 },
      [{ id: 1, count: 3 }],
    );
    await flushPromises();

    await wrapper.get('[data-testid="bulk-target-2"]').setValue(true);

    expect(wrapper.get('[data-testid="bulk-groups-submit"]').attributes("disabled")).toBeDefined();
  });
});

describe("bulk colour", () => {
  it("colours exactly the selected ids and says how many it covered", async () => {
    const wrapper = mountDialogs({ kind: "bulkColor", modIds: IDS, hiddenCount: 2 }, [], {
      set_mod_colors: () => ({ kind: "colored", changed: 2, unchanged: 1 }),
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="color-scope"]').text()).toBe("3 selected (2 not shown)");
    await wrapper.get('[data-testid="color-option-teal"]').setValue(true);
    await wrapper.get('[data-testid="color-apply"]').trigger("click");
    await flushPromises();

    expect(calls.filter((call) => call.command === "set_mod_colors")).toEqual([
      {
        command: "set_mod_colors",
        payload: { request: { modIds: IDS, color: "teal" } },
      },
    ]);
    expect(summaries()).toEqual(["Colour set for 2 mods", "1 did not change"]);
  });

  it("starts with nothing picked and Set colour off, so one click cannot clear every colour", async () => {
    const wrapper = mountDialogs({ kind: "bulkColor", modIds: IDS, hiddenCount: 0 }, [], {
      set_mod_colors: () => ({ kind: "colored", changed: 3, unchanged: 0 }),
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="color-apply"]').attributes("disabled")).toBeDefined();
    expect(
      (wrapper.get('[data-testid="color-option-none"]').element as HTMLInputElement).checked,
    ).toBe(false);
    await wrapper.get('[data-testid="color-apply"]').trigger("click");
    await flushPromises();

    expect(calls.some((call) => call.command === "set_mod_colors")).toBe(false);
    await wrapper.get('[data-testid="color-option-teal"]').setValue(true);
    expect(wrapper.get('[data-testid="color-apply"]').attributes("disabled")).toBeUndefined();
  });

  it("resets the colour only after 'No colour' is chosen on purpose", async () => {
    const wrapper = mountDialogs({ kind: "bulkColor", modIds: IDS, hiddenCount: 0 }, [], {
      set_mod_colors: () => ({ kind: "colored", changed: 3, unchanged: 0 }),
    });
    await flushPromises();

    await wrapper.get('[data-testid="color-option-none"]').setValue(true);
    await wrapper.get('[data-testid="color-apply"]').trigger("click");
    await flushPromises();

    expect(calls.filter((call) => call.command === "set_mod_colors")).toEqual([
      {
        command: "set_mod_colors",
        payload: { request: { modIds: IDS, color: null } },
      },
    ]);
    expect(summaries()).toEqual(["Colour cleared for 3 mods"]);
  });
});

describe("bulk group counts", () => {
  it("shows a loading line, and no group to pick, until the counts arrive", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "remove", modIds: IDS, hiddenCount: 0 },
      "never",
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="bulk-groups-loading"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="bulk-groups-none-held"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="bulk-target-1"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="bulk-groups-submit"]').attributes("disabled")).toBeDefined();
  });

  it("drops a late answer for an older selection instead of showing its counts", async () => {
    const resolvers = new Map<string, (rows: GroupMembershipCountDto[]) => void>();
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "remove", modIds: ["old"], hiddenCount: 0 },
      (request) =>
        new Promise((resolve) => {
          resolvers.set(request.modIds.join(","), resolve);
        }),
    );
    await flushPromises();
    await wrapper.setProps({
      dialog: { kind: "bulkGroups", mode: "remove", modIds: ["new"], hiddenCount: 0 },
    });
    await flushPromises();

    resolvers.get("new")?.([{ id: 2, count: 1 }]);
    await flushPromises();
    resolvers.get("old")?.([{ id: 1, count: 1 }]);
    await flushPromises();

    expect(wrapper.find('[data-testid="bulk-target-2"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="bulk-target-1"]').exists()).toBe(false);
  });

  it("asks again when the organization changes, so a deleted group leaves the list", async () => {
    let rows: GroupMembershipCountDto[] = [
      { id: 1, count: 1 },
      { id: 2, count: 1 },
    ];
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "remove", modIds: IDS, hiddenCount: 0 },
      () => Promise.resolve(rows),
    );
    await flushPromises();
    expect(wrapper.find('[data-testid="bulk-target-2"]').exists()).toBe(true);

    rows = [{ id: 1, count: 1 }];
    await useQueryCache().invalidateQueries({ key: ["organization"] });
    await flushPromises();

    expect(wrapper.find('[data-testid="bulk-target-2"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="bulk-target-1"]').exists()).toBe(true);
  });
});

describe("bulk dialog while it leaves", () => {
  it("keeps its mode and selection sentence until it has finished leaving", async () => {
    const wrapper = mountDialogs(
      { kind: "bulkGroups", mode: "remove", modIds: IDS, hiddenCount: 1 },
      [{ id: 2, count: 2 }],
    );
    await flushPromises();
    const bulk = () => wrapper.findComponent(BulkGroupsDialog);

    await wrapper.setProps({ dialog: null });
    await flushPromises();

    expect(bulk().props("mode")).toBe("remove");
    expect(bulk().props("selectionText")).toBe("3 selected (1 not shown)");

    bulk().findComponent(Dialog).vm.$emit("after-hide");
    await flushPromises();

    expect(bulk().props("mode")).toBe("add");
    expect(bulk().props("selectionText")).toBe("");
  });
});
