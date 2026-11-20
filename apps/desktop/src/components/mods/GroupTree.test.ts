import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import { ref } from "vue";

import GroupTree from "@/components/mods/GroupTree.vue";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
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

/** Weapons(1, 42 mods) > Melee(2) > Swords(3); Weapons > Ranged(4); Armor(5, teal). */
const TREE: GroupNodeDto[] = [
  node(1, "Weapons", null, 1, { childCount: 2, scopeCount: 42 }),
  node(2, "Melee", 1, 2, { childCount: 1, scopeCount: 5 }),
  node(3, "Swords", 2, 3, { scopeCount: 3 }),
  node(4, "Ranged", 1, 2, { scopeCount: 1 }),
  node(5, "Armor", null, 1, { color: "teal", scopeCount: 1 }),
];

let wrappers: ReturnType<typeof mount>[] = [];

function mountTree(
  options: { expanded?: number[]; shown?: Set<number> | null; scope?: GroupScopeChoice } = {},
) {
  const scope = ref<GroupScopeChoice>(options.scope ?? ALL_MODS);
  const expanded = ref<ReadonlySet<number>>(new Set(options.expanded ?? []));
  const wrapper = mount(GroupTree, {
    props: {
      nodes: TREE,
      shown: options.shown ?? null,
      scope: scope.value,
      "onUpdate:scope": (value: GroupScopeChoice) => {
        scope.value = value;
        void wrapper.setProps({ scope: value });
      },
      expanded: expanded.value,
      "onUpdate:expanded": (value: ReadonlySet<number>) => {
        expanded.value = value;
        void wrapper.setProps({ expanded: value });
      },
    },
    attachTo: document.body,
  });
  wrappers.push(wrapper);
  return { wrapper, scope, expanded };
}

function item(wrapper: ReturnType<typeof mount>, key: string) {
  return wrapper.get(`[data-key="${key}"]`);
}

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
});

describe("GroupTree", () => {
  it("is a tree whose entries carry level, selection and expansion state", () => {
    const { wrapper } = mountTree({ expanded: [1] });

    expect(wrapper.get('[role="tree"]').attributes("aria-label")).toBe("Groups");
    expect(item(wrapper, "all").attributes("aria-selected")).toBe("true");
    expect(item(wrapper, "ungrouped").attributes("aria-selected")).toBe("false");
    expect(item(wrapper, "group-1").attributes("aria-level")).toBe("1");
    expect(item(wrapper, "group-1").attributes("aria-expanded")).toBe("true");
    expect(item(wrapper, "group-2").attributes("aria-level")).toBe("2");
    expect(item(wrapper, "group-2").attributes("aria-expanded")).toBe("false");
    expect(item(wrapper, "group-4").attributes("aria-expanded")).toBeUndefined();
  });

  it("hides the children of a collapsed group", () => {
    const { wrapper } = mountTree();

    expect(wrapper.findAll('[role="treeitem"]')).toHaveLength(4);
    expect(wrapper.find('[data-key="group-2"]').exists()).toBe(false);
  });

  it("says in text how many mods a group holds including subgroups, its subgroups and its colour", () => {
    const { wrapper } = mountTree({ expanded: [1] });

    expect(item(wrapper, "group-1").text()).toContain("42 mods, including subgroups");
    expect(item(wrapper, "group-1").text()).toContain("2 subgroups");
    expect(item(wrapper, "group-5").text()).toContain("Colour: Teal");
    expect(item(wrapper, "group-5").get("[data-color]").attributes("data-color")).toBe("teal");
  });

  it("has one tab stop, which follows focus", async () => {
    const { wrapper } = mountTree({ expanded: [1] });

    expect(item(wrapper, "all").attributes("tabindex")).toBe("0");
    expect(item(wrapper, "group-1").attributes("tabindex")).toBe("-1");

    await item(wrapper, "group-4").trigger("focus");

    expect(item(wrapper, "group-4").attributes("tabindex")).toBe("0");
    expect(item(wrapper, "all").attributes("tabindex")).toBe("-1");
  });

  it("moves focus with Up, Down, Home and End", async () => {
    const { wrapper } = mountTree();
    await item(wrapper, "all").trigger("keydown", { key: "ArrowDown" });
    expect(document.activeElement?.getAttribute("data-key")).toBe("ungrouped");

    await item(wrapper, "ungrouped").trigger("keydown", { key: "End" });
    expect(document.activeElement?.getAttribute("data-key")).toBe("group-5");

    await item(wrapper, "group-5").trigger("keydown", { key: "ArrowUp" });
    expect(document.activeElement?.getAttribute("data-key")).toBe("group-1");

    await item(wrapper, "group-1").trigger("keydown", { key: "Home" });
    expect(document.activeElement?.getAttribute("data-key")).toBe("all");
  });

  it("expands a collapsed group on Right, then enters it; Left collapses, then goes to the parent", async () => {
    const { wrapper, expanded } = mountTree();

    await item(wrapper, "group-1").trigger("keydown", { key: "ArrowRight" });
    expect(expanded.value.has(1)).toBe(true);

    await item(wrapper, "group-1").trigger("keydown", { key: "ArrowRight" });
    expect(document.activeElement?.getAttribute("data-key")).toBe("group-2");

    await item(wrapper, "group-2").trigger("keydown", { key: "ArrowLeft" });
    expect(document.activeElement?.getAttribute("data-key")).toBe("group-1");

    await item(wrapper, "group-1").trigger("keydown", { key: "ArrowLeft" });
    expect(expanded.value.has(1)).toBe(false);
  });

  it("does nothing on Right for a group with no subgroups", async () => {
    const { wrapper, expanded } = mountTree();

    await item(wrapper, "group-5").trigger("keydown", { key: "ArrowRight" });

    expect(expanded.value.size).toBe(0);
  });

  it("scopes the lists on Enter and on Space, and on a click", async () => {
    const { wrapper, scope } = mountTree();

    await item(wrapper, "group-5").trigger("keydown", { key: "Enter" });
    expect(scope.value).toEqual({ kind: "group", id: 5 });

    await item(wrapper, "ungrouped").trigger("keydown", { key: " " });
    expect(scope.value).toEqual({ kind: "ungrouped" });

    await item(wrapper, "group-1").trigger("click");
    expect(scope.value).toEqual({ kind: "group", id: 1 });
  });

  it("jumps to a group by typing its name, cycling on a repeated letter", async () => {
    const { wrapper } = mountTree({ expanded: [1, 2] });

    await item(wrapper, "all").trigger("keydown", { key: "r" });
    expect(document.activeElement?.getAttribute("data-key")).toBe("group-4");

    await item(wrapper, "group-4").trigger("keydown", { key: "a" });
    // The typed text grows to "ra", which still names Ranged, so focus stays on it.
    expect(document.activeElement?.getAttribute("data-key")).toBe("group-4");
  });

  it("lets a space continue a name being typed, and scopes on a space only when none is", async () => {
    const { wrapper, scope } = mountTree({ expanded: [1] });

    // Plain space with no search going on: scopes, like Enter.
    await item(wrapper, "group-5").trigger("keydown", { key: " " });
    expect(scope.value).toEqual({ kind: "group", id: 5 });

    // "Melee" then "Mel" + space + "S" would name a two-word group: the space must not scope.
    await item(wrapper, "all").trigger("keydown", { key: "m" });
    expect(document.activeElement?.getAttribute("data-key")).toBe("group-2");
    await item(wrapper, "group-2").trigger("keydown", { key: " " });

    expect(scope.value).toEqual({ kind: "group", id: 5 });
    expect(document.activeElement?.getAttribute("data-key")).toBe("group-2");
  });

  it("moves the tab stop to the next sibling, else the parent, when its group is deleted", async () => {
    const { wrapper } = mountTree({ expanded: [1, 2] });
    await item(wrapper, "group-2").trigger("focus");

    // Melee (2) goes: its next sibling under Weapons is Ranged (4).
    await wrapper.setProps({ nodes: TREE.filter((entry) => entry.id !== 2 && entry.id !== 3) });
    expect(item(wrapper, "group-4").attributes("tabindex")).toBe("0");

    // Ranged (4), the last child, goes: the stop falls back to its parent Weapons (1).
    await wrapper.setProps({ nodes: TREE.filter((entry) => ![2, 3, 4].includes(entry.id)) });
    expect(item(wrapper, "group-1").attributes("tabindex")).toBe("0");
  });

  it("falls back to All mods when the last top-level group is deleted, and can then take focus", async () => {
    const { wrapper } = mountTree();
    await item(wrapper, "group-5").trigger("focus");

    await wrapper.setProps({ nodes: TREE.filter((entry) => entry.id !== 5) });
    (wrapper.vm as unknown as { focusCurrent: () => Promise<void> }).focusCurrent();
    await wrapper.vm.$nextTick();

    expect(item(wrapper, "all").attributes("tabindex")).toBe("0");
    expect(document.activeElement?.getAttribute("data-key")).toBe("all");
  });

  it("asks for rename on F2, delete on Delete and the menu on Shift+F10, only for a group", async () => {
    const { wrapper } = mountTree();

    await item(wrapper, "group-5").trigger("keydown", { key: "F2" });
    await item(wrapper, "group-5").trigger("keydown", { key: "Delete" });
    await item(wrapper, "group-5").trigger("keydown", { key: "F10", shiftKey: true });
    await item(wrapper, "all").trigger("keydown", { key: "F2" });
    await item(wrapper, "all").trigger("keydown", { key: "Delete" });
    await item(wrapper, "all").trigger("contextmenu");

    expect(wrapper.emitted("rename")).toEqual([[5]]);
    expect(wrapper.emitted("delete")).toEqual([[5]]);
    expect(wrapper.emitted("menu")?.map((call) => call[1])).toEqual([5]);
  });

  it("opens the group menu on a right click of a group", async () => {
    const { wrapper } = mountTree();

    await item(wrapper, "group-5").trigger("contextmenu");

    expect(wrapper.emitted("menu")?.map((call) => call[1])).toEqual([5]);
  });

  it("draws a tree search result expanded, keeping the ancestors of a match", () => {
    const { wrapper } = mountTree({ shown: new Set([1, 2, 3]) });

    expect(
      wrapper.findAll('[role="treeitem"]').map((entry) => entry.attributes("data-key")),
    ).toEqual(["all", "ungrouped", "group-1", "group-2", "group-3"]);
    expect(item(wrapper, "group-1").attributes("aria-expanded")).toBe("true");
  });

  it("marks the scoped group selected by more than colour", () => {
    const { wrapper } = mountTree({ scope: { kind: "group", id: 5 } });

    const selected = item(wrapper, "group-5");
    expect(selected.attributes("aria-selected")).toBe("true");
    expect(selected.classes()).toContain("font-semibold");
    expect(selected.classes()).toContain("inset-ring-2");
  });
});
