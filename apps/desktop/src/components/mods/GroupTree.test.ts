import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ref } from "vue";

import GroupTree from "@/components/mods/GroupTree.vue";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import { GROUP_DRAG_TYPE, ROW_DRAG_TYPE } from "@/utils/groupDrop";
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
  options: {
    nodes?: GroupNodeDto[];
    expanded?: number[];
    shown?: Set<number> | null;
    scope?: GroupScopeChoice;
    rowDrag?: { count: number; hiddenCount?: number } | null;
    openModGroupIds?: Set<number>;
  } = {},
) {
  const scope = ref<GroupScopeChoice>(options.scope ?? ALL_MODS);
  const expanded = ref<ReadonlySet<number>>(new Set(options.expanded ?? []));
  const wrapper = mount(GroupTree, {
    props: {
      nodes: options.nodes ?? TREE,
      shown: options.shown ?? null,
      openModGroupIds: options.openModGroupIds ?? new Set<number>(),
      rowDrag:
        options.rowDrag === undefined || options.rowDrag === null
          ? null
          : { hiddenCount: 0, ...options.rowDrag },
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

  it("finds a Greek group by a typed final sigma, which only the sigma fold matches", async () => {
    // "ΣΑ" lowercases to "σα" on its own, so a plain toLowerCase of the typed "ς" would not prefix it.
    const { wrapper } = mountTree({ nodes: [...TREE, node(6, "ΣΑ", null, 1)] });

    await item(wrapper, "all").trigger("keydown", { key: "ς" });

    expect(document.activeElement?.getAttribute("data-key")).toBe("group-6");
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

/**
 * Fires a drag event on `target` carrying `types` (what the browser lists for the drag); returns
 * whether the drop was accepted (the event cancelled).
 */
function drag(target: { element: Element }, type: string, types: string[] = []): boolean {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, "dataTransfer", { value: { types, setData: () => {} } });
  target.element.dispatchEvent(event);
  return event.defaultPrevented;
}

/** A drag of mods from a list. */
function dragRows(target: { element: Element }, type: string): boolean {
  return drag(target, type, [ROW_DRAG_TYPE]);
}

/** A drag of a group out of the tree. */
function dragGroup(target: { element: Element }, type: string): boolean {
  return drag(target, type, [GROUP_DRAG_TYPE]);
}

/** The `dropEffect` the tree set on a dragover carrying `types`. */
function dropEffectOf(target: { element: Element }, types: string[]): string | undefined {
  const dataTransfer = { types, dropEffect: undefined as string | undefined };
  const event = new Event("dragover", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "dataTransfer", { value: dataTransfer });
  target.element.dispatchEvent(event);
  return dataTransfer.dropEffect;
}

describe("GroupTree drops", () => {
  it("shows the copy cursor for mods (an add) and the move cursor for a group (a reparent)", () => {
    const { wrapper } = mountTree({ rowDrag: { count: 2 }, expanded: [1] });

    expect(dropEffectOf(item(wrapper, "group-5"), [ROW_DRAG_TYPE])).toBe("copy");
    drag(item(wrapper, "group-4"), "dragstart");
    expect(dropEffectOf(item(wrapper, "all"), [GROUP_DRAG_TYPE])).toBe("move");
  });

  it("accepts mods dragged onto a group, says what it would do, and emits the drop", async () => {
    const { wrapper } = mountTree({ rowDrag: { count: 12 } });

    expect(wrapper.get('[data-testid="group-drop-hint"]').text()).toBe(
      "Drop 12 mods on a group to add them",
    );
    expect(dragRows(item(wrapper, "group-5"), "dragover")).toBe(true);
    await wrapper.vm.$nextTick();

    expect(item(wrapper, "group-5").attributes("data-drop")).toBe("accept");
    // The cue is an icon and a sentence, not only an outline or a colour.
    expect(item(wrapper, "group-5").find('[data-testid="group-drop-icon"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="group-drop-hint"]').text()).toBe("Add 12 mods to Armor");

    expect(dragRows(item(wrapper, "group-5"), "drop")).toBe(true);
    expect(wrapper.emitted("dropMods")).toEqual([[5]]);
    expect(wrapper.emitted("dropGroup")).toBeUndefined();
  });

  it("refuses mods dragged onto All mods or Ungrouped, with the reason", async () => {
    const { wrapper } = mountTree({ rowDrag: { count: 2 } });

    expect(dragRows(item(wrapper, "all"), "dragover")).toBe(false);
    await wrapper.vm.$nextTick();
    expect(item(wrapper, "all").attributes("data-drop")).toBe("refuse");
    expect(wrapper.get('[data-testid="group-drop-hint"]').text()).toBe(
      "Mods can only be added to a group",
    );

    dragRows(item(wrapper, "ungrouped"), "drop");
    expect(wrapper.emitted("dropMods")).toBeUndefined();
  });

  it("clears the hover when the drag ends", async () => {
    const { wrapper } = mountTree({ rowDrag: { count: 1 } });
    dragRows(item(wrapper, "group-5"), "dragover");
    await wrapper.vm.$nextTick();

    await wrapper.setProps({ rowDrag: null });

    expect(wrapper.find("[data-drop]").exists()).toBe(false);
    expect(wrapper.find('[data-testid="group-drop-hint"]').exists()).toBe(false);
  });

  it("nests a dragged group on another group and makes it top level on All mods", async () => {
    const { wrapper } = mountTree({ expanded: [1] });

    drag(item(wrapper, "group-4"), "dragstart");
    expect(dragGroup(item(wrapper, "group-5"), "dragover")).toBe(true);
    await wrapper.vm.$nextTick();
    expect(wrapper.get('[data-testid="group-drop-hint"]').text()).toBe("Move Ranged inside Armor");
    dragGroup(item(wrapper, "group-5"), "drop");

    drag(item(wrapper, "group-4"), "dragstart");
    expect(dragGroup(item(wrapper, "all"), "dragover")).toBe(true);
    await wrapper.vm.$nextTick();
    expect(wrapper.get('[data-testid="group-drop-hint"]').text()).toBe(
      "Make Ranged a top-level group",
    );
    dragGroup(item(wrapper, "all"), "drop");

    expect(wrapper.emitted("dropGroup")).toEqual([
      [4, 5],
      [4, null],
    ]);
  });

  it("refuses a group dragged into itself or its own subgroup, saying why", async () => {
    const { wrapper } = mountTree({ expanded: [1, 2] });

    drag(item(wrapper, "group-1"), "dragstart");
    expect(dragGroup(item(wrapper, "group-3"), "dragover")).toBe(false);
    await wrapper.vm.$nextTick();
    expect(item(wrapper, "group-3").attributes("data-drop")).toBe("refuse");
    expect(wrapper.get('[data-testid="group-drop-hint"]').text()).toBe(
      "A group can't go inside itself or its own subgroups",
    );
    dragGroup(item(wrapper, "group-3"), "drop");
    dragGroup(item(wrapper, "group-1"), "drop");

    expect(wrapper.emitted("dropGroup")).toBeUndefined();
  });

  it("refuses a group dropped where it already is, and on Ungrouped", async () => {
    const { wrapper } = mountTree({ expanded: [1] });

    drag(item(wrapper, "group-4"), "dragstart");
    expect(dragGroup(item(wrapper, "group-1"), "dragover")).toBe(false);
    await wrapper.vm.$nextTick();
    expect(wrapper.get('[data-testid="group-drop-hint"]').text()).toBe(
      "The group is already there",
    );
    expect(dragGroup(item(wrapper, "ungrouped"), "dragover")).toBe(false);
    await wrapper.vm.$nextTick();
    expect(item(wrapper, "ungrouped").attributes("data-drop")).toBe("refuse");
  });

  it("opens a collapsed group after a drag hovers it for a moment", async () => {
    vi.useFakeTimers();
    try {
      const { wrapper, expanded } = mountTree({ rowDrag: { count: 1 } });

      dragRows(item(wrapper, "group-1"), "dragover");
      vi.advanceTimersByTime(799);
      expect(expanded.value.has(1)).toBe(false);
      vi.advanceTimersByTime(1);

      expect(expanded.value.has(1)).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not open a group whose drag moved on before the delay", () => {
    vi.useFakeTimers();
    try {
      const { wrapper, expanded } = mountTree({ rowDrag: { count: 1 } });

      dragRows(item(wrapper, "group-1"), "dragover");
      dragRows(item(wrapper, "group-1"), "dragleave");
      vi.advanceTimersByTime(2000);

      expect(expanded.value.has(1)).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("ignores a drag it does not know about", () => {
    const { wrapper } = mountTree();

    expect(dragRows(item(wrapper, "group-5"), "dragover")).toBe(false);
    dragRows(item(wrapper, "group-5"), "drop");

    expect(wrapper.emitted("dropMods")).toBeUndefined();
    expect(wrapper.find("[data-drop]").exists()).toBe(false);
  });

  it("ignores a file or text dragged over a group while a stale row drag is still recorded", async () => {
    const { wrapper } = mountTree({ rowDrag: { count: 3 } });

    expect(drag(item(wrapper, "group-5"), "dragover", ["Files"])).toBe(false);
    expect(drag(item(wrapper, "group-5"), "dragover", ["text/plain"])).toBe(false);
    drag(item(wrapper, "group-5"), "drop", ["text/plain"]);
    await wrapper.vm.$nextTick();

    expect(wrapper.emitted("dropMods")).toBeUndefined();
    expect(wrapper.find("[data-drop]").exists()).toBe(false);
  });

  it("ignores a group drag type while no group is being dragged out of this tree", () => {
    const { wrapper } = mountTree();

    expect(dragGroup(item(wrapper, "group-5"), "dragover")).toBe(false);
    dragGroup(item(wrapper, "all"), "drop");

    expect(wrapper.emitted("dropGroup")).toBeUndefined();
  });

  it("says how many dragged mods a filter hides, before the drop", async () => {
    const { wrapper } = mountTree({ rowDrag: { count: 12, hiddenCount: 3 } });

    expect(wrapper.get('[data-testid="group-drop-hint"]').text()).toBe(
      "Drop 12 mods on a group to add them (3 not shown)",
    );
    dragRows(item(wrapper, "group-5"), "dragover");
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="group-drop-hint"]').text()).toBe(
      "Add 12 mods to Armor (3 not shown)",
    );
  });

  it("does not refuse a move for depth: the backend owns that limit", () => {
    const { wrapper } = mountTree({ expanded: [1] });

    dragGroup(item(wrapper, "group-5"), "dragstart");

    expect(dragGroup(item(wrapper, "group-4"), "dragover")).toBe(true);
  });
  describe("folder icons and the open mod's groups", () => {
    it("draws a filled folder in the group's colour and an outlined one when it has none", () => {
      const { wrapper } = mountTree();

      const coloured = wrapper.get(
        '[data-testid="group-node-5"] [data-testid="group-folder-icon"]',
      );
      expect(coloured.attributes("fill")).toBe("currentColor");
      expect(coloured.classes()).toContain("text-mark-teal");
      expect(coloured.attributes("data-color")).toBe("teal");
      const plain = wrapper.get('[data-testid="group-node-1"] [data-testid="group-folder-icon"]');
      expect(plain.attributes("fill")).toBe("none");
      expect(plain.attributes("data-color")).toBeUndefined();
    });

    it("highlights the groups the open mod is in, with a screen-reader cue, and no others", () => {
      const { wrapper } = mountTree({ openModGroupIds: new Set([5]) });

      const member = wrapper.get('[data-testid="group-node-5"]');
      expect(member.find('[data-testid="group-node-open-dot"]').exists()).toBe(true);
      expect(member.classes()).not.toContain("bg-accent-soft");
      expect(member.get('[data-testid="group-node-open-mod"]').text()).toBe(
        "The open mod is in this group",
      );
      const other = wrapper.get('[data-testid="group-node-1"]');
      expect(other.find('[data-testid="group-node-open-dot"]').exists()).toBe(false);
      expect(other.find('[data-testid="group-node-open-mod"]').exists()).toBe(false);
    });

    it("highlights nothing while no mod is open", () => {
      const { wrapper } = mountTree();

      expect(wrapper.find('[data-testid="group-node-open-mod"]').exists()).toBe(false);
    });
  });
});
