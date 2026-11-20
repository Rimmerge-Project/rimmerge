import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import Menu from "primevue/menu";
import { afterEach, describe, expect, it } from "vitest";

import GroupRail from "@/components/mods/GroupRail.vue";
import GroupScopeBar from "@/components/mods/GroupScopeBar.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { GroupScopePlanDto } from "@/types/generated/GroupScopePlanDto";
import { ALL_MODS, type GroupScopeChoice } from "@/utils/organization";

function node(
  id: number,
  name: string,
  parent: number | null,
  depth: number,
  extra: Partial<GroupNodeDto> = {},
): GroupNodeDto {
  return {
    id,
    name,
    color: null,
    parent,
    depth,
    childCount: 0,
    directCount: 0,
    scopeCount: 0,
    ...extra,
  };
}

const NODES = [
  node(1, "Weapons", null, 1, { childCount: 1, scopeCount: 4 }),
  node(2, "Melee", 1, 2, { scopeCount: 2 }),
  node(3, "Armor", null, 1),
];

let wrappers: ReturnType<typeof mount>[] = [];
const planCalls: number[] = [];

function plan(notInstalled: number): GroupScopePlanDto {
  return {
    groupCount: 1,
    modCount: 4,
    activeIds: [],
    inactiveIds: [],
    notInstalled,
    notInstalledIds: Array.from({ length: notInstalled }, (_, index) => `gone.${index}`),
  };
}

function install(
  notInstalled: number,
  planAnswer: () => GroupScopePlanDto | Promise<GroupScopePlanDto> = () => plan(notInstalled),
): void {
  planCalls.length = 0;
  installMockIpc({
    plan_group_scope: (payload: unknown) => {
      planCalls.push((payload as { request: { id: number } }).request.id);
      return planAnswer();
    },
  });
}

function mountComponent(component: object, props: Record<string, unknown>) {
  const wrapper = mount(component, {
    props,
    attachTo: document.body,
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true },
    },
  });
  wrappers.push(wrapper);
  return wrapper;
}

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
  clearMocks();
});

describe("GroupRail", () => {
  it("has a tree and an empty note, a search that keeps ancestors, and no close button as a column", async () => {
    install(0);
    const wrapper = mountComponent(GroupRail, { nodes: NODES, scope: ALL_MODS });
    await flushPromises();

    expect(wrapper.find('[data-testid="group-rail-close"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="group-empty"]').exists()).toBe(false);

    await wrapper.get('[data-testid="group-search"]').setValue("melee");
    expect(wrapper.find('[data-key="group-1"]').exists()).toBe(true);
    expect(wrapper.find('[data-key="group-2"]').exists()).toBe(true);
    expect(wrapper.find('[data-key="group-3"]').exists()).toBe(false);

    await wrapper.get('[data-testid="group-search"]').setValue("zzz");
    expect(wrapper.get('[data-testid="group-no-match"]').text()).toBe("No group matches.");
  });

  it("invites creating a first group when there are none", async () => {
    install(0);
    const wrapper = mountComponent(GroupRail, { nodes: [], scope: ALL_MODS });
    await flushPromises();

    expect(wrapper.find('[data-testid="group-empty"]').exists()).toBe(true);
    await wrapper.get('[data-testid="group-new"]').trigger("click");
    expect(wrapper.emitted("createGroup")).toEqual([[null]]);
  });

  it("is a drawer with a close button and closes on Escape from anywhere on the page", async () => {
    install(0);
    const wrapper = mountComponent(GroupRail, { nodes: NODES, scope: ALL_MODS, isOverlay: true });
    await flushPromises();

    expect(wrapper.get('[data-testid="group-rail"]').classes()).toContain("fixed");
    await wrapper.get('[data-testid="group-rail-close"]').trigger("click");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));

    expect(wrapper.emitted("close")).toHaveLength(2);
  });

  it("opens a group menu from the tree, with the not-installed count asked of the backend", async () => {
    install(3);
    const wrapper = mountComponent(GroupRail, { nodes: NODES, scope: ALL_MODS });
    await flushPromises();

    await wrapper.get('[data-key="group-1"]').trigger("contextmenu");
    await flushPromises();

    const items = wrapper.findAll('[data-testid^="group-menu-"]').map((item) => item.text());
    expect(items).toEqual([
      "New subgroup…",
      "Rename…",
      "Colour…",
      "Move group to…",
      "Remove 3 mods that aren't installed",
      "Delete…",
    ]);
    expect(planCalls).toContain(1);

    await wrapper.get('[data-testid="group-menu-delete"]').trigger("click");
    await wrapper.get('[data-key="group-1"]').trigger("contextmenu");
    await flushPromises();
    await wrapper.get('[data-testid="group-menu-new-subgroup"]').trigger("click");
    expect(wrapper.emitted("deleteGroup")).toEqual([[1]]);
    expect(wrapper.emitted("createGroup")).toEqual([[1]]);
  });

  it("disables the remove entry when every mod in the group is installed", async () => {
    install(0);
    const wrapper = mountComponent(GroupRail, { nodes: NODES, scope: ALL_MODS });
    await flushPromises();

    await wrapper.get('[data-key="group-3"]').trigger("contextmenu");
    await flushPromises();

    const item = wrapper.get('[data-testid="group-menu-remove-uninstalled"]');
    expect(item.text()).toBe("All mods in this group are installed");
    expect(item.element.closest("li")?.getAttribute("data-p-disabled")).toBe("true");
  });

  it("never says every mod is installed while the count is still loading or failed", async () => {
    install(0, () => new Promise(() => {}));
    const loading = mountComponent(GroupRail, { nodes: NODES, scope: ALL_MODS });
    await flushPromises();
    await loading.get('[data-key="group-3"]').trigger("contextmenu");
    await flushPromises();

    const pending = loading.get('[data-testid="group-menu-remove-uninstalled"]');
    expect(pending.text()).toBe("Checking for mods that aren't installed…");
    expect(pending.element.closest("li")?.getAttribute("data-p-disabled")).toBe("true");

    clearMocks();
    install(0, () => {
      throw { code: "internal", message: "boom" };
    });
    const failed = mountComponent(GroupRail, { nodes: NODES, scope: ALL_MODS });
    await flushPromises();
    await failed.get('[data-key="group-3"]').trigger("contextmenu");
    await flushPromises();

    const item = failed.get('[data-testid="group-menu-remove-uninstalled"]');
    expect(item.text()).toBe("Couldn't check for mods that aren't installed");
    expect(item.element.closest("li")?.getAttribute("data-p-disabled")).toBe("true");
  });

  it("keeps the drawer open when Escape closes the group menu", async () => {
    install(0);
    const wrapper = mountComponent(GroupRail, { nodes: NODES, scope: ALL_MODS, isOverlay: true });
    await flushPromises();
    await wrapper.get('[data-key="group-3"]').trigger("contextmenu");
    await flushPromises();

    await wrapper.get('[role="menu"]').trigger("keydown", { key: "Escape", code: "Escape" });
    expect(wrapper.emitted("close")).toBeUndefined();

    // Outside the menu, Escape still closes the drawer.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(wrapper.emitted("close")).toHaveLength(1);
  });

  it("marks the first item of a menu opened from the keyboard, and not of one opened by a right click", async () => {
    install(0);
    const wrapper = mountComponent(GroupRail, { nodes: NODES, scope: ALL_MODS });
    await flushPromises();
    // PrimeVue focuses the popup list, then emits `show`; happy-dom cannot lay the overlay out,
    // so its enter hook never runs and this stands in for it, in the same order.
    const showMenu = () => {
      (wrapper.get('[role="menu"]').element as HTMLElement).focus();
      wrapper.findComponent(Menu).vm.$emit("show");
    };

    await wrapper.get('[data-key="group-3"]').trigger("contextmenu");
    await flushPromises();
    showMenu();
    await flushPromises();
    expect(wrapper.get('[role="menu"]').attributes("aria-activedescendant")).toBeUndefined();

    await wrapper.get('[data-key="group-3"]').trigger("keydown", { key: "ContextMenu" });
    await flushPromises();
    await wrapper.get('[data-key="group-3"]').trigger("keydown", { key: "ContextMenu" });
    await flushPromises();
    showMenu();
    await flushPromises();
    const first = wrapper.get('[data-testid="group-menu-new-subgroup"]').element.closest("li");
    expect(wrapper.get('[role="menu"]').attributes("aria-activedescendant")).toBe(first?.id);
  });

  it("acts on the scoped group from the Actions button, which needs a scoped group", async () => {
    install(0);
    const unscoped = mountComponent(GroupRail, { nodes: NODES, scope: ALL_MODS });
    const scoped = mountComponent(GroupRail, {
      nodes: NODES,
      scope: { kind: "group", id: 3 } satisfies GroupScopeChoice,
    });
    await flushPromises();

    expect(unscoped.get('[data-testid="group-actions"]').attributes("disabled")).toBeDefined();
    expect(scoped.get('[data-testid="group-actions"]').attributes("aria-label")).toBe(
      "Actions for Armor",
    );
    await scoped.get('[data-testid="group-actions"]').trigger("click");
    await flushPromises();
    await scoped.get('[data-testid="group-menu-rename"]').trigger("click");
    expect(scoped.emitted("renameGroup")).toEqual([[3]]);
  });

  it("opens a group that gains its first subgroup, so the new subgroup is visible", async () => {
    install(0);
    const wrapper = mount(GroupRail, {
      props: { nodes: [node(3, "Armor", null, 1)], scope: ALL_MODS },
      attachTo: document.body,
      global: {
        plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
        stubs: { teleport: true },
      },
    });
    wrappers.push(wrapper);
    await flushPromises();

    await wrapper.setProps({
      nodes: [node(3, "Armor", null, 1, { childCount: 1 }), node(4, "Plate", 3, 2)],
    });

    expect(wrapper.find('[data-key="group-4"]').exists()).toBe(true);
  });
});

describe("GroupScopeBar", () => {
  it("renders nothing for every mod", async () => {
    install(0);
    const wrapper = mountComponent(GroupScopeBar, { scope: ALL_MODS, nodes: NODES });
    await flushPromises();

    expect(wrapper.find('[data-testid="group-scope-bar"]').exists()).toBe(false);
  });

  it("names the group path, its mod count with subgroups and how many are not installed", async () => {
    install(2);
    const wrapper = mountComponent(GroupScopeBar, {
      scope: { kind: "group", id: 2 },
      nodes: NODES,
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="group-scope-text"]').text()).toBe(
      "Showing Weapons / Melee: 2 mods, including subgroups",
    );
    expect(wrapper.get('[data-testid="group-scope-not-installed"]').text()).toBe(
      "2 mods aren't installed",
    );
    await wrapper.get('[data-testid="group-scope-clear"]').trigger("click");
    expect(wrapper.emitted("clear")).toEqual([[]]);
  });

  it("says Ungrouped plainly and asks the backend nothing", async () => {
    install(0);
    const wrapper = mountComponent(GroupScopeBar, { scope: { kind: "ungrouped" }, nodes: NODES });
    await flushPromises();

    expect(wrapper.get('[data-testid="group-scope-text"]').text()).toBe(
      "Showing the mods that are in no group",
    );
    expect(planCalls).toEqual([]);
  });
});
