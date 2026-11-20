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
import type { GroupScopePlanDto } from "@/types/generated/GroupScopePlanDto";
import type { OrganizationDialog } from "@/utils/organization";

/**
 * `OrganizationDialogs` is the host that runs each dialog's one edit: it shows a refusal inside the
 * dialog, toasts and closes on success, keeps the mod groups dialog open between toggles, and sends
 * the "remove uninstalled" request with exactly the ids its dialog lists.
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
    scopeCount: 3,
  };
}

const NODES = [node(1, "Weapons"), node(2, "Armor")];

function plan(notInstalledIds: string[]): GroupScopePlanDto {
  return {
    groupCount: 1,
    modCount: 3,
    activeIds: [],
    inactiveIds: [],
    notInstalled: notInstalledIds.length,
    notInstalledIds,
  };
}

interface Call {
  command: string;
  payload: unknown;
}

let wrappers: ReturnType<typeof mount>[] = [];
const calls: Call[] = [];

type Handler = (payload: unknown) => unknown;

/** Mounts the dialogs with `initial` open and a model that follows what they emit. */
function mountDialogs(initial: OrganizationDialog | null, handlers: Record<string, Handler> = {}) {
  calls.length = 0;
  installMockIpc({
    list_mod_names: { "gone.one": "Gone One" },
    get_organization: { kind: "loaded", groups: NODES, coloredModCount: 0 },
    list_working_mods: { items: [], total: 0, reorderable: true },
    list_inactive_mods: { items: [], total: 0, reorderable: false },
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

describe("OrganizationDialogs: running an edit", () => {
  it("closes the dialog and says so after a successful edit", async () => {
    const wrapper = mountDialogs(
      { kind: "createGroup", parent: null },
      { create_group: () => ({ kind: "created", id: 7 }) },
    );
    await flushPromises();

    await wrapper.get('[data-testid="group-name-input"]').setValue("Utility");
    await wrapper.get('[data-testid="group-name-submit"]').trigger("click");
    await flushPromises();

    expect(calls).toEqual([
      { command: "create_group", payload: { request: { name: "Utility", parent: null } } },
    ]);
    expect(currentDialog(wrapper)).toBeNull();
    expect(toastAdd).toHaveBeenCalledWith(expect.objectContaining({ summary: "Group created" }));
  });

  it("keeps the dialog open and shows the refusal inside it, with no toast", async () => {
    const wrapper = mountDialogs(
      { kind: "createGroup", parent: null },
      {
        create_group: () => {
          throw { code: "group_name_taken", message: "taken" };
        },
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="group-name-input"]').setValue("Weapons");
    await wrapper.get('[data-testid="group-name-submit"]').trigger("click");
    await flushPromises();

    expect(currentDialog(wrapper)).toEqual({ kind: "createGroup", parent: null });
    expect(wrapper.get('[data-testid="organization-error"]').text()).toContain(
      "That group name is already used",
    );
    expect(toastAdd).not.toHaveBeenCalled();
  });

  it("keeps the mod groups dialog open after a toggle and follows what was saved", async () => {
    const wrapper = mountDialogs(
      { kind: "modGroups", modId: "some.mod", groupIds: [2] },
      { add_group_members: () => ({ kind: "added", added: 1, already: 0 }) },
    );
    await flushPromises();

    await wrapper.get('[data-testid="mod-groups-toggle-1"]').trigger("click");
    await flushPromises();

    expect(calls[0]).toEqual({
      command: "add_group_members",
      payload: { request: { id: 1, modIds: ["some.mod"] } },
    });
    const dialog = currentDialog(wrapper);
    expect(dialog).toMatchObject({ kind: "modGroups", modId: "some.mod" });
    expect(dialog?.kind === "modGroups" && [...dialog.groupIds].sort()).toEqual([1, 2]);
    expect(
      wrapper.get<HTMLInputElement>('[data-testid="mod-groups-toggle-1"]').element.checked,
    ).toBe(true);
  });

  it("leaves the box as it was and shows the error when a toggle is refused", async () => {
    const wrapper = mountDialogs(
      { kind: "modGroups", modId: "some.mod", groupIds: [2] },
      {
        remove_group_members: () => {
          throw { code: "group_not_found", message: "gone" };
        },
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="mod-groups-toggle-2"]').trigger("click");
    await flushPromises();

    expect(currentDialog(wrapper)).toMatchObject({ groupIds: [2] });
    expect(wrapper.find('[data-testid="organization-error"]').exists()).toBe(true);
  });

  it("does not reopen or rewrite a mod groups dialog that was closed while its toggle was saving", async () => {
    let finishSave: (value: unknown) => void = () => {};
    const saving = new Promise((resolve) => {
      finishSave = resolve;
    });
    const wrapper = mountDialogs(
      { kind: "modGroups", modId: "some.mod", groupIds: [2] },
      { remove_group_members: () => saving },
    );
    await flushPromises();

    await wrapper.get('[data-testid="mod-groups-toggle-2"]').trigger("click");
    await wrapper.setProps({ dialog: null });
    finishSave({ kind: "removed", removed: 1, absent: 0 });
    await flushPromises();

    expect(currentDialog(wrapper)).toBeNull();
    const reopened = (wrapper.emitted("update:dialog") ?? []).filter((args) => args[0] !== null);
    expect(reopened).toEqual([]);
  });
});

describe("OrganizationDialogs: remove mods that are not installed", () => {
  const OPEN: OrganizationDialog = { kind: "removeUninstalled", id: 1 };

  it("lists the mods by name as of the last scan and sends exactly those ids", async () => {
    const wrapper = mountDialogs(OPEN, {
      plan_group_scope: () => plan(["gone.one", "gone.two"]),
      remove_uninstalled_members: () => ({ kind: "removed", removed: 2, absent: 0 }),
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="remove-uninstalled-dialog"]').text()).toContain(
      "As of the last scan, 2 mods in Weapons and its subgroups aren't installed.",
    );
    expect(
      wrapper.findAll('[data-testid="remove-uninstalled-list"] li').map((entry) => entry.text()),
    ).toEqual(["Gone One", "gone.two"]);

    await wrapper.get('[data-testid="remove-uninstalled-confirm"]').trigger("click");
    await flushPromises();

    const sent = calls.find((call) => call.command === "remove_uninstalled_members");
    expect(sent?.payload).toEqual({ request: { id: 1, modIds: ["gone.one", "gone.two"] } });
    expect(currentDialog(wrapper)).toBeNull();
    expect(toastAdd).toHaveBeenCalledWith(
      expect.objectContaining({ summary: "Removed 2 mods that aren't installed" }),
    );
  });

  it("says how many listed mods were kept because they are installed again", async () => {
    const wrapper = mountDialogs(OPEN, {
      plan_group_scope: () => plan(["gone.one", "gone.two", "gone.three"]),
      remove_uninstalled_members: () => ({ kind: "removed", removed: 1, absent: 1 }),
    });
    await flushPromises();

    await wrapper.get('[data-testid="remove-uninstalled-confirm"]').trigger("click");
    await flushPromises();

    expect(toastAdd).toHaveBeenCalledWith(
      expect.objectContaining({
        summary: "Removed 1 mod that isn't installed",
        detail: "1 mod is installed again, so it stayed in the group",
      }),
    );
  });

  it("adds no note when every listed mod was removed", async () => {
    const wrapper = mountDialogs(OPEN, {
      plan_group_scope: () => plan(["gone.one", "gone.two"]),
      remove_uninstalled_members: () => ({ kind: "removed", removed: 2, absent: 0 }),
    });
    await flushPromises();

    await wrapper.get('[data-testid="remove-uninstalled-confirm"]').trigger("click");
    await flushPromises();

    expect(toastAdd.mock.calls.at(-1)?.[0]).not.toHaveProperty("detail");
  });

  it("says it is checking, and cannot be confirmed, while the plan is loading", async () => {
    const wrapper = mountDialogs(OPEN, { plan_group_scope: () => new Promise(() => {}) });
    await flushPromises();

    const dialog = wrapper.get('[data-testid="remove-uninstalled-dialog"]');
    expect(dialog.get('[data-testid="remove-uninstalled-loading"]').text()).toBe(
      "Checking which mods aren't installed…",
    );
    expect(dialog.text()).not.toContain("Remove 0 mods");
    expect(
      wrapper.get('[data-testid="remove-uninstalled-confirm"]').attributes("disabled"),
    ).toBeDefined();
  });

  it("says the check failed, and cannot be confirmed, when the plan failed", async () => {
    const wrapper = mountDialogs(OPEN, {
      plan_group_scope: () => {
        throw { code: "group_not_found", message: "gone" };
      },
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="remove-uninstalled-failed"]').text()).toBe(
      "Couldn't check which mods aren't installed. Close this and try again.",
    );
    expect(
      wrapper.get('[data-testid="remove-uninstalled-confirm"]').attributes("disabled"),
    ).toBeDefined();
  });

  it("says every mod is installed, with nothing to confirm, when none is missing", async () => {
    const wrapper = mountDialogs(OPEN, { plan_group_scope: () => plan([]) });
    await flushPromises();

    expect(wrapper.get('[data-testid="remove-uninstalled-none"]').text()).toBe(
      "All mods in this group are installed",
    );
    expect(
      wrapper.get('[data-testid="remove-uninstalled-confirm"]').attributes("disabled"),
    ).toBeDefined();
  });
});
