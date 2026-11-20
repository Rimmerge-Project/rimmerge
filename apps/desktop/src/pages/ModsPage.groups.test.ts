import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import Dialog from "primevue/dialog";
import ToastService from "primevue/toastservice";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import { dashboardFixture } from "@/components/apply/ApplyDialog.test-support";
import { modList, modRow } from "@/components/mods/modListRow.test-support";
import GroupDeleteDialog from "@/components/mods/organization/GroupDeleteDialog.vue";
import ModsPage from "@/pages/ModsPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";
import type { OrganizationDto } from "@/types/generated/OrganizationDto";

const toastAddSpy = vi.hoisted(() => vi.fn());
vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: toastAddSpy }) }));

function node(id: number, name: string, extra: Partial<GroupNodeDto> = {}): GroupNodeDto {
  return {
    id,
    name,
    color: null,
    parent: null,
    depth: 1,
    childCount: 0,
    directCount: 0,
    scopeCount: 2,
    ...extra,
  };
}

const WEAPONS = node(1, "Weapons");
const ARMOR = node(2, "Armor");

let organization: OrganizationDto;
const activeQueries: ModListQueryDto[] = [];
const inactiveQueries: ModListQueryDto[] = [];
const calls: { command: string; payload: unknown }[] = [];
/** Makes the Active list refuse a group filter, as the backend does for a deleted group. */
let refuseGroupFilter: "group_not_found" | "organization_unavailable" | null = null;
/** What a rescan leaves the organization as; `null` leaves it alone. */
let afterRescan: OrganizationDto | null = null;

function record(command: string, result: unknown): (payload: unknown) => unknown {
  return (payload) => {
    calls.push({ command, payload });
    return result;
  };
}

function mountPage() {
  activeQueries.length = 0;
  inactiveQueries.length = 0;
  calls.length = 0;
  installMockIpc({
    list_working_mods: (payload: unknown) => {
      const query = (payload as { query: ModListQueryDto }).query;
      activeQueries.push(query);
      if (refuseGroupFilter !== null && query.group !== undefined && query.group !== null) {
        throw { code: refuseGroupFilter, message: "refused" };
      }
      return modList([
        modRow({
          modId: "active.mod",
          name: "Active Mod",
          position: 0,
          groups: [{ id: 1, name: "Weapons", color: "red" }],
          color: "teal",
        }),
      ]);
    },
    list_inactive_mods: (payload: unknown) => {
      inactiveQueries.push((payload as { query: ModListQueryDto }).query);
      return modList([
        modRow({ modId: "idle.mod", name: "Idle Mod", pendingScan: true, manualTags: ["pet"] }),
      ]);
    },
    list_mod_names: {},
    get_organization: () => organization,
    rescan_project: () => {
      if (afterRescan !== null) {
        organization = afterRescan;
      }
      return { selected: "current", warnings: [], ruleWarnings: [] };
    },
    get_dashboard: dashboardFixture(0),
    list_findings: { total: 0, items: [] },
    get_mod_info: () => new Promise(() => {}),
    create_group: record("create_group", { kind: "created", id: 9 }),
    delete_group: (payload: unknown) => {
      calls.push({ command: "delete_group", payload });
      const { id } = (payload as { request: { id: number } }).request;
      if (organization.kind === "loaded") {
        organization = {
          ...organization,
          groups: organization.groups.filter((group) => group.id !== id),
        };
      }
      return { kind: "deleted", groupsRemoved: 1, childrenLifted: 0 };
    },
    add_group_members: record("add_group_members", { kind: "added", added: 1, already: 0 }),
    remove_group_members: record("remove_group_members", {
      kind: "removed",
      removed: 1,
      absent: 0,
    }),
    set_mod_colors: record("set_mod_colors", { kind: "colored", changed: 1, unchanged: 0 }),
    plan_group_scope: () => ({
      groupCount: 1,
      modCount: 2,
      activeIds: [],
      inactiveIds: [],
      notInstalled: 0,
      notInstalledIds: [],
    }),
    activate_mods: record("activate_mods", null),
    deactivate_mods: record("deactivate_mods", null),
    move_mods: record("move_mods", null),
    apply: record("apply", null),
  });
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/mods", name: "mods", component: ModsPage },
      { path: "/mods/:modId", name: "mod-detail", component: ModsPage },
    ],
  });
  return mount(
    { template: "<router-view />" },
    {
      attachTo: document.body,
      global: {
        plugins: [
          createPinia(),
          PiniaColada,
          router,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
        stubs: { teleport: true },
      },
    },
  );
}

async function mountAt(width: number) {
  setViewport(width, 900);
  const wrapper = mountPage();
  await wrapper.vm.$router.push({ name: "mods" });
  await flushPromises();
  return wrapper;
}

function setViewport(width: number, height: number): void {
  const { happyDOM } = window as unknown as {
    happyDOM?: { setViewport: (size: { width: number; height: number }) => void };
  };
  happyDOM?.setViewport({ width, height });
}

describe("ModsPage groups", () => {
  beforeEach(() => {
    organization = { kind: "loaded", groups: [WEAPONS, ARMOR], coloredModCount: 1 };
    refuseGroupFilter = null;
    afterRescan = null;
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      value: 400,
    });
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
      configurable: true,
      value: 600,
    });
  });

  afterEach(() => {
    clearMocks();
    toastAddSpy.mockClear();
    setViewport(1024, 768);
    Reflect.deleteProperty(HTMLElement.prototype, "offsetHeight");
    Reflect.deleteProperty(HTMLElement.prototype, "offsetWidth");
    document.body.innerHTML = "";
  });

  it("is a rail beside the lists on a very wide window, with no Groups button", async () => {
    const wrapper = await mountAt(1900);

    const rail = wrapper.get('[data-testid="group-rail"]');
    expect(rail.classes()).not.toContain("fixed");
    expect(wrapper.find('[data-testid="groups-open"]').exists()).toBe(false);
  });

  it("is a left drawer opened by a Groups button below that, closed by Escape", async () => {
    const wrapper = await mountAt(1400);

    expect(wrapper.find('[data-testid="group-rail"]').exists()).toBe(false);
    await wrapper.get('[data-testid="groups-open"]').trigger("click");
    await flushPromises();
    expect(wrapper.get('[data-testid="group-rail"]').classes()).toContain("fixed");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await flushPromises();

    expect(wrapper.find('[data-testid="group-rail"]').exists()).toBe(false);
  });

  describe("keyboard focus", () => {
    async function openDrawer() {
      const wrapper = await mountAt(1400);
      await wrapper.get('[data-testid="groups-open"]').trigger("click");
      await flushPromises();
      return wrapper;
    }

    it.each([
      [
        "Escape",
        async () => {
          window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
        },
      ],
      [
        "the close button",
        async (wrapper: Awaited<ReturnType<typeof mountAt>>) => {
          await wrapper.get('[data-testid="group-rail-close"]').trigger("click");
        },
      ],
      [
        "picking a scope",
        async (wrapper: Awaited<ReturnType<typeof mountAt>>) => {
          await wrapper.get('[data-key="group-1"]').trigger("click");
        },
      ],
    ])(
      "hands focus back to the Groups button when the drawer closes by %s",
      async (_how, close) => {
        const wrapper = await openDrawer();
        expect(document.activeElement).not.toBe(wrapper.get('[data-testid="groups-open"]').element);

        await close(wrapper);
        await flushPromises();

        expect(wrapper.find('[data-testid="group-rail"]').exists()).toBe(false);
        expect(document.activeElement).toBe(wrapper.get('[data-testid="groups-open"]').element);
      },
    );

    it("does not pull focus to the Groups button when the scope is cleared while the drawer is shut", async () => {
      const wrapper = await openDrawer();
      await wrapper.get('[data-key="group-1"]').trigger("click");
      await flushPromises();
      const other = wrapper.get('[data-testid="mods-search"]').element as HTMLElement;
      other.focus();

      await wrapper.get('[data-testid="group-scope-clear"]').trigger("click");
      await flushPromises();

      expect(document.activeElement).toBe(other);
    });

    it("says the Groups button controls a panel, not a dialog", async () => {
      const wrapper = await mountAt(1400);

      const button = wrapper.get('[data-testid="groups-open"]');
      expect(button.attributes("aria-haspopup")).toBeUndefined();
      expect(button.attributes("aria-controls")).toBe("group-rail");
    });

    it("forgets an open drawer when groups turn unusable, so it does not reappear when they work again", async () => {
      const wrapper = await openDrawer();

      afterRescan = { kind: "recovered", issue: { kind: "malformed" } };
      await wrapper.get('[data-testid="mods-rescan-button"]').trigger("click");
      await flushPromises();
      expect(wrapper.find('[data-testid="group-rail"]').exists()).toBe(false);

      afterRescan = { kind: "loaded", groups: [WEAPONS, ARMOR], coloredModCount: 1 };
      await wrapper.get('[data-testid="mods-rescan-button"]').trigger("click");
      await flushPromises();

      expect(wrapper.find('[data-testid="group-rail"]').exists()).toBe(false);
      expect(wrapper.find('[data-testid="groups-open"]').exists()).toBe(true);
    });

    it("moves focus to the next sibling after a group is deleted", async () => {
      const wrapper = await mountAt(1900);
      await wrapper.get('[data-key="group-1"]').trigger("focus");

      await wrapper.get('[data-key="group-1"]').trigger("contextmenu");
      await flushPromises();
      await wrapper.get('[data-testid="group-menu-delete"]').trigger("click");
      await flushPromises();
      await wrapper.get('[data-testid="group-delete-submit"]').trigger("click");
      await flushPromises();
      // happy-dom has no transition to end, so the dialog's own after-hide is stood in for.
      wrapper.findComponent(GroupDeleteDialog).findComponent(Dialog).vm.$emit("after-hide");
      await flushPromises();

      expect(wrapper.find('[data-key="group-1"]').exists()).toBe(false);
      expect(document.activeElement?.getAttribute("data-key")).toBe("group-2");
    });

    it("does not move focus when the delete dialog is cancelled", async () => {
      const wrapper = await mountAt(1900);
      await wrapper.get('[data-key="group-1"]').trigger("focus");
      await wrapper.get('[data-key="group-1"]').trigger("contextmenu");
      await flushPromises();
      await wrapper.get('[data-testid="group-menu-delete"]').trigger("click");
      await flushPromises();
      await wrapper.get('[data-testid="group-delete-cancel"]').trigger("click");
      await flushPromises();
      const before = document.activeElement;

      wrapper.findComponent(GroupDeleteDialog).findComponent(Dialog).vm.$emit("after-hide");
      await flushPromises();

      expect(document.activeElement).toBe(before);
      expect(wrapper.find('[data-key="group-1"]').exists()).toBe(true);
    });
  });

  it("scopes both lists to a group (descendants are the backend's), and back to every mod", async () => {
    const wrapper = await mountAt(1900);

    await wrapper.get('[data-key="group-1"]').trigger("click");
    await flushPromises();

    const filter = { kind: "inScope", groupId: 1 };
    expect(activeQueries.at(-1)?.group).toEqual(filter);
    expect(inactiveQueries.at(-1)?.group).toEqual(filter);
    expect(wrapper.get('[data-testid="group-scope-text"]').text()).toContain("Weapons");

    await wrapper.get('[data-testid="group-scope-clear"]').trigger("click");
    await flushPromises();
    // The unscoped query is cached from the first load, so the proof is the page state.
    expect(wrapper.find('[data-testid="group-scope-bar"]').exists()).toBe(false);
    expect(wrapper.get('[data-key="all"]').attributes("aria-selected")).toBe("true");
  });

  it("scopes both lists to the ungrouped mods", async () => {
    const wrapper = await mountAt(1900);

    await wrapper.get('[data-key="ungrouped"]').trigger("click");
    await flushPromises();

    expect(activeQueries.at(-1)?.group).toEqual({ kind: "ungrouped" });
    expect(inactiveQueries.at(-1)?.group).toEqual({ kind: "ungrouped" });
  });

  it("runs a search inside the scope, so the query carries both", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      const wrapper = await mountAt(1900);
      await wrapper.get('[data-key="group-2"]').trigger("click");
      await flushPromises();

      await wrapper
        .get('[data-testid="mods-pane-active"] [data-testid="mods-search"]')
        .setValue("sword");
      vi.advanceTimersByTime(400);
      await flushPromises();

      expect(activeQueries.at(-1)).toMatchObject({
        search: "sword",
        group: { kind: "inScope", groupId: 2 },
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it("drops a scope whose group is gone, quietly, with no error pane", async () => {
    const wrapper = await mountAt(1900);
    refuseGroupFilter = "group_not_found";

    await wrapper.get('[data-key="group-1"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="mods-error"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="group-scope-bar"]').exists()).toBe(false);
    expect(wrapper.get('[data-key="all"]').attributes("aria-selected")).toBe("true");
    expect(wrapper.find('[data-testid="mod-row-active.mod"]').exists()).toBe(true);
  });

  it("shows the notice and no group panel when groups could not be read", async () => {
    organization = { kind: "recovered", issue: { kind: "malformed" } };

    const wrapper = await mountAt(1900);

    expect(wrapper.get('[data-testid="organization-notice"]').text()).toContain("damaged");
    expect(wrapper.find('[data-testid="group-rail"]').exists()).toBe(false);
    await wrapper.get('[data-testid="mod-row-menu-active.mod"]').trigger("click");
    await flushPromises();
    expect(wrapper.find('[data-testid="mod-move-top"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="mod-menu-groups"]').exists()).toBe(false);
  });

  it("shows chips, colour and tags on rows, manual tags included on an inactive row", async () => {
    const wrapper = await mountAt(1900);

    const active = wrapper.get('[data-testid="mod-row-active.mod"]');
    expect(active.get('[data-testid="mod-row-group-1"]').text()).toBe("Weapons");
    expect(active.get('[data-testid="mod-row-color"]').attributes("data-color")).toBe("teal");
    expect(
      wrapper.get('[data-testid="mod-row-idle.mod"] [data-testid="mod-row-tags"]').text(),
    ).toBe("pet");
  });

  it("filters the Inactive list by tag too", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      const wrapper = await mountAt(1900);

      await wrapper
        .get('[data-testid="mods-pane-inactive"] [data-testid="mods-tag-filter"]')
        .setValue("pet");
      vi.advanceTimersByTime(400);
      await flushPromises();

      expect(inactiveQueries.at(-1)?.tag).toBe("pet");
    } finally {
      vi.useRealTimers();
    }
  });

  it("changes a mod's groups and colour from its row menu without touching activation, order or selection", async () => {
    const wrapper = await mountAt(1900);
    await wrapper.get('[data-testid="mod-row-select-active.mod"] input').setValue(true);
    await wrapper.get('[data-testid="mod-row-menu-active.mod"]').trigger("click");
    await flushPromises();

    await wrapper.get('[data-testid="mod-menu-groups"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-testid="mod-groups-toggle-2"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-testid="mod-groups-done"]').trigger("click");
    await flushPromises();

    await wrapper.get('[data-testid="mod-row-menu-active.mod"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-testid="mod-menu-color"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-testid="color-option-pink"]').setValue(true);
    await wrapper.get('[data-testid="color-apply"]').trigger("click");
    await flushPromises();

    expect(calls.map((call) => call.command)).toEqual(["add_group_members", "set_mod_colors"]);
    expect(calls[0]?.payload).toEqual({ request: { id: 2, modIds: ["active.mod"] } });
    expect(calls[1]?.payload).toEqual({ request: { modIds: ["active.mod"], color: "pink" } });
    // The row stayed selected: nothing organization does changes the selection.
    expect(
      wrapper.get<HTMLInputElement>('[data-testid="mod-row-select-active.mod"] input').element
        .checked,
    ).toBe(true);
  });

  it("creates a group from the rail and sends only create_group", async () => {
    const wrapper = await mountAt(1900);

    await wrapper.get('[data-testid="group-new"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-testid="group-name-input"]').setValue("Utility");
    await wrapper.get('[data-testid="group-name-submit"]').trigger("click");
    await flushPromises();

    expect(calls).toEqual([
      { command: "create_group", payload: { request: { name: "Utility", parent: null } } },
    ]);
    expect(wrapper.find('[data-testid="group-name-dialog"]').exists()).toBe(false);
  });
});
