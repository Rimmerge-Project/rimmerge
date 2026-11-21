import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import ModGroupsTable from "@/components/mods/ModGroupsTable.vue";
import { modRow } from "@/components/mods/modListRow.test-support";
import { installMockIpc } from "@/services/ipc.mock";
import type { GroupedEntryDto } from "@/types/generated/GroupedEntryDto";
import type { GroupedModListDto } from "@/types/generated/GroupedModListDto";
import type { GroupLabelDto } from "@/types/generated/GroupLabelDto";
import { ROW_DRAG_TYPE } from "@/utils/groupDrop";

const WEAPONS: GroupLabelDto = { id: 1, name: "Weapons", color: null };
const MELEE: GroupLabelDto = { id: 2, name: "Melee", color: null };

function groupFolder(
  id: number,
  name: string,
  depth: number,
  matched: number,
  isCollapsed = false,
) {
  return {
    kind: "folder",
    folder: { kind: "group", id, name, color: null },
    depth,
    matched,
    isCollapsed,
  } satisfies GroupedEntryDto;
}

function modEntry(modId: string, groupId: number | null, depth: number, alsoIn = 0) {
  return {
    kind: "mod",
    modId,
    folder: groupId === null ? { kind: "ungrouped" } : { kind: "group", groupId },
    depth,
    alsoIn,
  } satisfies GroupedEntryDto;
}

const UNGROUPED = {
  kind: "folder",
  folder: { kind: "ungrouped" },
  depth: 1,
  matched: 1,
  isCollapsed: false,
} satisfies GroupedEntryDto;

/**
 * Weapons (a, b) holds Melee (b, c) and `b` is in both; "Not in a group" holds d.
 *
 *   f:1  Weapons          level 1
 *     f:2  Melee          level 2
 *       m:2:b, m:2:c      level 3
 *     m:1:a, m:1:b        level 2
 *   f:u  Not in a group   level 1
 *     m:u:d               level 2
 */
function grouped(overrides: Partial<GroupedModListDto> = {}): GroupedModListDto {
  const entries: GroupedEntryDto[] = [
    groupFolder(1, "Weapons", 1, 3),
    groupFolder(2, "Melee", 2, 2),
    modEntry("b", 2, 3, 1),
    modEntry("c", 2, 3),
    modEntry("a", 1, 2),
    modEntry("b", 1, 2, 1),
    UNGROUPED,
    modEntry("d", null, 2),
  ];
  return {
    total: 4,
    items: [
      modRow({ modId: "a", name: "Mod A", groups: [WEAPONS] }),
      modRow({ modId: "b", name: "Mod B", groups: [WEAPONS, MELEE] }),
      modRow({ modId: "c", name: "Mod C", groups: [MELEE] }),
      modRow({ modId: "d", name: "Mod D" }),
    ],
    reorderable: false,
    entries,
    totalEntries: entries.length,
    ...overrides,
  };
}

let mountedWrappers: ReturnType<typeof mount>[] = [];

function mountTable(props: Partial<InstanceType<typeof ModGroupsTable>["$props"]> = {}) {
  installMockIpc({ list_mod_names: {} });
  const wrapper = mount(ModGroupsTable, {
    props: { grouped: grouped(), ...props },
    attachTo: document.body,
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true },
    },
  });
  mountedWrappers.push(wrapper);
  return wrapper;
}

const row = (wrapper: ReturnType<typeof mountTable>, key: string) =>
  wrapper.get<HTMLElement>(`[data-entry-key="${key}"]`);

/** What a drag of mod rows carries; `types` is what a drop target reads to tell it from a file. */
const MOD_DRAG = { dataTransfer: { types: [ROW_DRAG_TYPE], dropEffect: "move" } };

async function press(
  wrapper: ReturnType<typeof mountTable>,
  key: string,
  keyName: string,
  init: KeyboardEventInit = {},
) {
  await row(wrapper, key).trigger("keydown", { key: keyName, ...init });
  await flushPromises();
}

describe("ModGroupsTable", () => {
  beforeEach(() => {
    // The virtualizer measures its scroller through these, which happy-dom reports as 0.
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      value: 600,
    });
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, value: 600 });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    clearMocks();
    Reflect.deleteProperty(HTMLElement.prototype, "offsetHeight");
    Reflect.deleteProperty(HTMLElement.prototype, "offsetWidth");
    for (const wrapper of mountedWrappers) {
      wrapper.unmount();
    }
    mountedWrappers = [];
  });

  describe("treegrid semantics", () => {
    it("is a treegrid with a level on every row, a row index, and the rows' count", async () => {
      const wrapper = mountTable();
      await flushPromises();

      const table = wrapper.get('[data-testid="mod-groups-table"]');
      expect(table.attributes("role")).toBe("treegrid");
      expect(table.attributes("aria-rowcount")).toBe("8");
      const levels = wrapper
        .findAll("[data-entry-key]")
        .map((element) => [element.attributes("data-entry-key"), element.attributes("aria-level")]);
      expect(levels).toEqual([
        ["f:1", "1"],
        ["f:2", "2"],
        ["m:2:b", "3"],
        ["m:2:c", "3"],
        ["m:1:a", "2"],
        ["m:1:b", "2"],
        ["f:u", "1"],
        ["m:u:d", "2"],
      ]);
      expect(row(wrapper, "m:1:a").attributes("aria-rowindex")).toBe("5");
    });

    it("puts aria-expanded on headers only and aria-selected on mod rows only", async () => {
      const wrapper = mountTable({
        grouped: grouped({
          entries: [groupFolder(1, "Weapons", 1, 3, true), UNGROUPED, modEntry("d", null, 2)],
        }),
        selectedIds: ["d"],
      });
      await flushPromises();

      expect(row(wrapper, "f:1").attributes("aria-expanded")).toBe("false");
      expect(row(wrapper, "f:u").attributes("aria-expanded")).toBe("true");
      expect(row(wrapper, "f:1").attributes("aria-selected")).toBeUndefined();
      expect(row(wrapper, "m:u:d").attributes("aria-expanded")).toBeUndefined();
      expect(row(wrapper, "m:u:d").attributes("aria-selected")).toBe("true");
    });

    it("names a header by its name, its count and its state", async () => {
      const wrapper = mountTable({
        grouped: grouped({
          entries: [groupFolder(1, "Weapons", 1, 1, true), groupFolder(2, "Armor", 1, 12)],
        }),
      });
      await flushPromises();

      expect(row(wrapper, "f:1").attributes("aria-label")).toBe("Weapons, 1 mod, collapsed");
      expect(row(wrapper, "f:2").attributes("aria-label")).toBe("Armor, 12 mods, open");
    });

    it("gives every row its own entry key and test id, the two copies of a mod included", async () => {
      const wrapper = mountTable();
      await flushPromises();

      const keys = wrapper.findAll("[data-entry-key]").map((el) => el.attributes("data-entry-key"));
      const testIds = wrapper.findAll("[data-entry-key]").map((el) => el.attributes("data-testid"));
      expect(new Set(keys).size).toBe(keys.length);
      expect(new Set(testIds).size).toBe(testIds.length);
      expect(wrapper.find('[data-testid="mod-entry-2-b"]').exists()).toBe(true);
      expect(wrapper.find('[data-testid="mod-entry-1-b"]').exists()).toBe(true);
      expect(wrapper.find('[data-testid="mods-folder-ungrouped"]').exists()).toBe(true);
      expect(wrapper.find('[data-testid="mod-row-b"]').exists()).toBe(false);
    });
  });

  describe("copies of a mod", () => {
    it("shows both copies of a selected mod as selected", async () => {
      const wrapper = mountTable({ selectedIds: ["b"] });
      await flushPromises();

      expect(row(wrapper, "m:2:b").attributes("aria-selected")).toBe("true");
      expect(row(wrapper, "m:1:b").attributes("aria-selected")).toBe("true");
      expect(row(wrapper, "m:1:a").attributes("aria-selected")).toBe("false");
    });

    it("marks a mod in other groups with the also-in count and names them", async () => {
      const wrapper = mountTable();
      await flushPromises();

      const inWeapons = row(wrapper, "m:1:b").get('[data-testid="mod-also-in"]');
      expect(inWeapons.text()).toContain("also in 1");
      expect(inWeapons.attributes("title")).toBe("Also in: Melee");
      expect(row(wrapper, "m:2:b").get('[data-testid="mod-also-in"]').attributes("title")).toBe(
        "Also in: Weapons",
      );
      expect(row(wrapper, "m:1:a").find('[data-testid="mod-also-in"]').exists()).toBe(false);
    });

    it("toggling either copy's checkbox reports the mod and the row it was done on", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await row(wrapper, "m:2:b").get('[data-testid="mod-row-select-b"] input').setValue(true);

      expect(wrapper.emitted("toggleSelect")).toEqual([["b", "m:2:b"]]);
    });
  });

  describe("pointer", () => {
    it("opens a mod on a plain click and moves the anchor to that row", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await row(wrapper, "m:1:b").trigger("click");

      expect(wrapper.emitted("anchorRow")).toEqual([["m:1:b"]]);
      expect(wrapper.emitted("selectMod")).toEqual([["b"]]);
    });

    it("reports a Shift+click as a range to that row's key, opening nothing", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await row(wrapper, "m:2:c").trigger("click", { shiftKey: true });

      expect(wrapper.emitted("rangeSelect")).toEqual([["m:2:c"]]);
      expect(wrapper.emitted("selectMod")).toBeUndefined();
    });

    it("a click on a header folds or opens it, and opens no mod", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await row(wrapper, "f:1").trigger("click");

      expect(wrapper.emitted("setFolded")).toEqual([[{ kind: "group", groupId: 1 }, true]]);
      expect(wrapper.emitted("selectMod")).toBeUndefined();
    });

    it("a Ctrl, Shift or second click of a double click on a header does not fold it", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await row(wrapper, "f:1").trigger("click", { ctrlKey: true });
      await row(wrapper, "f:1").trigger("click", { shiftKey: true });
      await row(wrapper, "f:1").trigger("click", { detail: 2 });

      expect(wrapper.emitted("setFolded")).toBeUndefined();
      expect(wrapper.emitted("anchorRow")).toBeUndefined();
    });

    it("keeps a double click on a header from selecting its text", async () => {
      const wrapper = mountTable();
      await flushPromises();

      const first = new MouseEvent("mousedown", { detail: 1, bubbles: true, cancelable: true });
      const second = new MouseEvent("mousedown", { detail: 2, bubbles: true, cancelable: true });
      row(wrapper, "f:1").element.dispatchEvent(first);
      row(wrapper, "f:1").element.dispatchEvent(second);

      expect(first.defaultPrevented).toBe(false);
      expect(second.defaultPrevented).toBe(true);
    });

    it("a click on a folded header opens it", async () => {
      const wrapper = mountTable({
        grouped: grouped({ entries: [groupFolder(1, "Weapons", 1, 3, true)] }),
      });
      await flushPromises();

      await row(wrapper, "f:1").trigger("click");

      expect(wrapper.emitted("setFolded")).toEqual([[{ kind: "group", groupId: 1 }, false]]);
    });
  });

  describe("double click on a mod", () => {
    const inRow = (wrapper: ReturnType<typeof mountTable>, selector: string) =>
      wrapper.get(`[data-entry-key="m:1:a"] ${selector}`);

    it("a plain double click on a mod row asks to toggle its activation", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await row(wrapper, "m:1:a").trigger("dblclick");

      expect(wrapper.emitted("toggleActive")).toEqual([["a"]]);
    });

    it("a double click on the checkbox or the menu button is theirs, not the row's", async () => {
      const wrapper = mountTable({ rowMenu: "move" });
      await flushPromises();

      await inRow(wrapper, '[data-testid="mod-row-select-a"]').trigger("dblclick");
      await inRow(wrapper, '[data-testid="mod-row-select-a"] input').trigger("dblclick");
      await inRow(wrapper, '[data-testid="mod-row-menu-a"]').trigger("dblclick");

      expect(wrapper.emitted("toggleActive")).toBeUndefined();
    });

    it("leaves a Ctrl or Shift double click to selection", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await row(wrapper, "m:1:a").trigger("dblclick", { ctrlKey: true });
      await row(wrapper, "m:1:a").trigger("dblclick", { shiftKey: true });

      expect(wrapper.emitted("toggleActive")).toBeUndefined();
    });
  });

  describe("keyboard", () => {
    it("Left folds an open header and Right opens a folded one", async () => {
      const open = mountTable();
      await flushPromises();
      await press(open, "f:1", "ArrowLeft");
      expect(open.emitted("setFolded")).toEqual([[{ kind: "group", groupId: 1 }, true]]);

      const folded = mountTable({
        grouped: grouped({ entries: [groupFolder(1, "Weapons", 1, 3, true)] }),
      });
      await flushPromises();
      await press(folded, "f:1", "ArrowRight");
      expect(folded.emitted("setFolded")).toEqual([[{ kind: "group", groupId: 1 }, false]]);
    });

    it("Right on an open header moves focus to its first child", async () => {
      const wrapper = mountTable();
      await flushPromises();
      row(wrapper, "f:1").element.focus();

      await press(wrapper, "f:1", "ArrowRight");

      expect(document.activeElement).toBe(row(wrapper, "f:2").element);
      expect(wrapper.emitted("setFolded")).toBeUndefined();
    });

    it("Right on an open folder with nothing inside does not enter its next sibling", async () => {
      const wrapper = mountTable({
        grouped: grouped({
          entries: [groupFolder(1, "Weapons", 1, 0), groupFolder(2, "Melee", 1, 0), UNGROUPED],
        }),
      });
      await flushPromises();
      row(wrapper, "f:1").element.focus();

      await press(wrapper, "f:1", "ArrowRight");

      expect(document.activeElement).toBe(row(wrapper, "f:1").element);
      expect(wrapper.emitted("setFolded")).toBeUndefined();
      expect(wrapper.emitted("anchorRow")).toBeUndefined();
    });

    it("Left on a folded header goes to its parent, and on a top-level one does nothing", async () => {
      const wrapper = mountTable({
        grouped: grouped({
          entries: [groupFolder(1, "Weapons", 1, 3), groupFolder(2, "Melee", 2, 2, true)],
        }),
      });
      await flushPromises();
      row(wrapper, "f:2").element.focus();

      await press(wrapper, "f:2", "ArrowLeft");
      expect(document.activeElement).toBe(row(wrapper, "f:1").element);
      expect(wrapper.emitted("setFolded")).toBeUndefined();

      const topFolded = mountTable({
        grouped: grouped({ entries: [groupFolder(1, "Weapons", 1, 3, true)] }),
      });
      await flushPromises();
      await press(topFolded, "f:1", "ArrowLeft");
      expect(topFolded.emitted("setFolded")).toBeUndefined();
    });

    it("Left on a mod row goes to the folder header it sits in", async () => {
      const wrapper = mountTable();
      await flushPromises();
      row(wrapper, "m:2:c").element.focus();

      await press(wrapper, "m:2:c", "ArrowLeft");

      expect(document.activeElement).toBe(row(wrapper, "f:2").element);
    });

    it("Right on a mod row does nothing", async () => {
      const wrapper = mountTable();
      await flushPromises();
      row(wrapper, "m:1:a").element.focus();

      await press(wrapper, "m:1:a", "ArrowRight");

      expect(document.activeElement).toBe(row(wrapper, "m:1:a").element);
      expect(wrapper.emitted("setFolded")).toBeUndefined();
    });

    it.each(["Enter", " "])("%j on a header toggles it and opens no mod", async (keyName) => {
      const wrapper = mountTable();
      await flushPromises();

      await press(wrapper, "f:1", keyName);

      expect(wrapper.emitted("setFolded")).toEqual([[{ kind: "group", groupId: 1 }, true]]);
      expect(wrapper.emitted("selectMod")).toBeUndefined();
      expect(wrapper.emitted("toggleSelect")).toBeUndefined();
    });

    it("Enter on a mod row opens it and Space toggles it, reporting the row", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await press(wrapper, "m:1:b", "Enter");
      await press(wrapper, "m:1:b", " ");

      expect(wrapper.emitted("selectMod")).toEqual([["b"]]);
      expect(wrapper.emitted("toggleSelect")).toEqual([["b", "m:1:b"]]);
      expect(wrapper.emitted("setFolded")).toBeUndefined();
    });

    it("Shift+Space on a mod row extends a range to that row", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await press(wrapper, "m:1:a", " ", { shiftKey: true });

      expect(wrapper.emitted("rangeSelect")).toEqual([["m:1:a"]]);
    });

    it("arrows move focus across headers and mods alike, and move the anchor with it", async () => {
      const wrapper = mountTable();
      await flushPromises();
      row(wrapper, "f:1").element.focus();

      await press(wrapper, "f:1", "ArrowDown");
      expect(document.activeElement).toBe(row(wrapper, "f:2").element);
      await press(wrapper, "f:2", "End");
      expect(document.activeElement).toBe(row(wrapper, "m:u:d").element);
      await press(wrapper, "m:u:d", "Home");
      expect(document.activeElement).toBe(row(wrapper, "f:1").element);

      expect(wrapper.emitted("anchorRow")).toEqual([["f:2"], ["m:u:d"], ["f:1"]]);
    });

    it("Shift+Arrow extends from the row focus left to the row it reached", async () => {
      const wrapper = mountTable();
      await flushPromises();
      row(wrapper, "m:2:c").element.focus();

      await press(wrapper, "m:2:c", "ArrowDown", { shiftKey: true });

      expect(wrapper.emitted("extendSelect")).toEqual([["m:2:c", "m:1:a"]]);
    });

    it("Ctrl+A selects all and Escape clears, only with something selected", async () => {
      const wrapper = mountTable({ selectedIds: ["a"] });
      await flushPromises();

      await press(wrapper, "m:1:a", "a", { ctrlKey: true });
      await press(wrapper, "m:1:a", "Escape");

      expect(wrapper.emitted("selectAll")).toHaveLength(1);
      expect(wrapper.emitted("clearSelection")).toHaveLength(1);
    });

    it("Alt+Arrow moves nothing: no reorder is asked for and focus stays", async () => {
      const wrapper = mountTable({ rowMenu: "move" });
      await flushPromises();
      row(wrapper, "m:1:a").element.focus();

      await press(wrapper, "m:1:a", "ArrowDown", { altKey: true });

      expect(wrapper.emitted("moveStep")).toBeUndefined();
      expect(document.activeElement).toBe(row(wrapper, "m:1:a").element);
    });

    it("has exactly one tab stop: the first row until a row is focused, then that row", async () => {
      const wrapper = mountTable();
      await flushPromises();
      const stops = () =>
        wrapper
          .findAll("[data-entry-key]")
          .filter((element) => element.attributes("tabindex") === "0")
          .map((element) => element.attributes("data-entry-key"));
      expect(stops()).toEqual(["f:1"]);

      await row(wrapper, "m:1:b").trigger("focus");

      expect(stops()).toEqual(["m:1:b"]);
    });

    it("hands roving focus to the folder header when a fold hides the focused copy", async () => {
      const wrapper = mountTable();
      await flushPromises();
      await row(wrapper, "m:2:c").trigger("focus");

      await wrapper.setProps({
        grouped: grouped({
          entries: [
            groupFolder(1, "Weapons", 1, 3),
            groupFolder(2, "Melee", 2, 2, true),
            UNGROUPED,
          ],
        }),
      });
      await flushPromises();

      expect(row(wrapper, "f:2").attributes("tabindex")).toBe("0");
      expect(row(wrapper, "f:1").attributes("tabindex")).toBe("-1");
    });

    it("moves DOM focus to the folder when focus was inside the copy a fold hid", async () => {
      const wrapper = mountTable();
      await flushPromises();
      row(wrapper, "m:2:c").element.focus();

      await wrapper.setProps({
        grouped: grouped({
          entries: [
            groupFolder(1, "Weapons", 1, 3),
            groupFolder(2, "Melee", 2, 2, true),
            UNGROUPED,
          ],
        }),
      });
      await flushPromises();

      expect(document.activeElement).toBe(row(wrapper, "f:2").element);
    });

    it("leaves DOM focus alone when it was somewhere else (only the tab stop moves)", async () => {
      const wrapper = mountTable();
      await flushPromises();
      await row(wrapper, "m:2:c").trigger("focus");
      const elsewhere = document.createElement("button");
      document.body.append(elsewhere);
      elsewhere.focus();

      await wrapper.setProps({
        grouped: grouped({
          entries: [
            groupFolder(1, "Weapons", 1, 3),
            groupFolder(2, "Melee", 2, 2, true),
            UNGROUPED,
          ],
        }),
      });
      await flushPromises();

      expect(row(wrapper, "f:2").attributes("tabindex")).toBe("0");
      expect(document.activeElement).toBe(elsewhere);
      elsewhere.remove();
    });

    it("falls back to the first row when the focused row is gone with its folder", async () => {
      const wrapper = mountTable();
      await flushPromises();
      await row(wrapper, "m:u:d").trigger("focus");

      await wrapper.setProps({ grouped: grouped({ entries: [groupFolder(1, "Weapons", 1, 3)] }) });
      await flushPromises();

      expect(row(wrapper, "f:1").attributes("tabindex")).toBe("0");
    });
  });

  describe("dropping mods", () => {
    it("a drop on a group header reports that group, in any drop mode but off", async () => {
      const wrapper = mountTable({ dropMode: "refused" });
      await flushPromises();

      await row(wrapper, "f:2").trigger("dragover", MOD_DRAG);
      await row(wrapper, "f:2").trigger("drop", MOD_DRAG);

      expect(wrapper.emitted("dropOnFolder")).toEqual([[2]]);
      expect(wrapper.emitted("drop")).toBeUndefined();
    });

    it("a drop on Not in a group reports nothing and refuses with a reason on screen", async () => {
      const wrapper = mountTable({ dropMode: "pane" });
      await flushPromises();

      await row(wrapper, "f:u").trigger("dragover", MOD_DRAG);
      expect(wrapper.find('[data-testid="mods-groups-ungrouped-refused"]').exists()).toBe(true);
      await row(wrapper, "f:u").trigger("drop", MOD_DRAG);

      expect(wrapper.emitted("dropOnFolder")).toBeUndefined();
      expect(wrapper.emitted("drop")).toBeUndefined();
    });

    it("takes no drop on a header for a drag that is not of mods", async () => {
      const wrapper = mountTable({ dropMode: "pane" });
      await flushPromises();

      await row(wrapper, "f:2").trigger("dragover", { dataTransfer: { types: ["Files"] } });
      await row(wrapper, "f:2").trigger("drop", { dataTransfer: { types: ["Files"] } });

      expect(wrapper.emitted("dropOnFolder")).toBeUndefined();
    });

    it("takes no drop on a header while no drag is in progress", async () => {
      const wrapper = mountTable({ dropMode: "off" });
      await flushPromises();

      await row(wrapper, "f:2").trigger("drop", MOD_DRAG);

      expect(wrapper.emitted("dropOnFolder")).toBeUndefined();
    });

    it("a drop on a mod row is the pane's own drop, not a folder's", async () => {
      const wrapper = mountTable({ dropMode: "pane" });
      await flushPromises();

      await row(wrapper, "m:1:a").trigger("dragover");
      await row(wrapper, "m:1:a").trigger("drop");

      expect(wrapper.emitted("dropOnFolder")).toBeUndefined();
      expect(wrapper.emitted("drop")).toHaveLength(1);
    });
  });

  describe("notes", () => {
    it("says nothing about a cut when every line is shown", async () => {
      const wrapper = mountTable();
      await flushPromises();

      expect(wrapper.find('[data-testid="mods-groups-cut-note"]').exists()).toBe(false);
    });

    it("says how many lines are not shown when the backend cut the view", async () => {
      const wrapper = mountTable({ grouped: grouped({ totalEntries: 8 + 3 }) });
      await flushPromises();

      expect(wrapper.get('[data-testid="mods-groups-cut-note"]').text()).toContain(
        "3 lines are not shown",
      );
    });

    it("says one line, in the singular, for a cut of one", async () => {
      const wrapper = mountTable({ grouped: grouped({ totalEntries: 9 }) });
      await flushPromises();

      expect(wrapper.get('[data-testid="mods-groups-cut-note"]').text()).toContain(
        "1 line is not shown",
      );
    });

    it("explains an empty answer, in different words under a filter", async () => {
      const unfiltered = mountTable({ grouped: grouped({ entries: [], totalEntries: 0 }) });
      const filtered = mountTable({
        grouped: grouped({ entries: [], totalEntries: 0 }),
        isFiltered: true,
      });
      await flushPromises();

      const texts = [unfiltered, filtered].map((wrapper) =>
        wrapper.get('[data-testid="mods-groups-empty"]').text(),
      );
      expect(texts[0]).toBe("There are no mods to show here.");
      expect(texts[1]).toBe("No mods match the current search, tag, source or group.");
      // A status line is not a row: it sits outside the treegrid.
      expect(
        unfiltered
          .get('[data-testid="mod-groups-table"]')
          .find('[data-testid="mods-groups-empty"]')
          .exists(),
      ).toBe(false);
    });
  });

  describe("folder menu", () => {
    it("offers select, activate and deactivate on a group", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await row(wrapper, "f:1").get('[data-testid="folder-menu-button"]').trigger("click");
      await flushPromises();
      const groupEntries = [...document.querySelectorAll('[data-testid^="folder-menu-"]')]
        .map((element) => element.getAttribute("data-testid"))
        .filter((id) => id !== "folder-menu-button");
      expect(groupEntries).toEqual([
        "folder-menu-select",
        "folder-menu-activate",
        "folder-menu-deactivate",
      ]);
    });

    it("offers only the selection on Not in a group", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await row(wrapper, "f:u").get('[data-testid="folder-menu-button"]').trigger("click");
      await flushPromises();
      const ungroupedEntries = [...document.querySelectorAll('[data-testid^="folder-menu-"]')]
        .map((element) => element.getAttribute("data-testid"))
        .filter((id) => id !== "folder-menu-button");
      expect(ungroupedEntries).toEqual(["folder-menu-select"]);
    });

    it("reports the folder and the action chosen", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await row(wrapper, "f:2").get('[data-testid="folder-menu-button"]').trigger("click");
      await flushPromises();
      (document.querySelector('[data-testid="folder-menu-activate"]') as HTMLElement).click();
      await flushPromises();

      expect(wrapper.emitted("folderAction")).toEqual([
        [{ kind: "group", groupId: 2 }, "activate"],
      ]);
    });

    it("Shift+F10 on a header opens its menu", async () => {
      const wrapper = mountTable();
      await flushPromises();

      await press(wrapper, "f:1", "F10", { shiftKey: true });

      expect(document.querySelector('[data-testid="folder-menu-select"]')).not.toBeNull();
    });
  });
});
