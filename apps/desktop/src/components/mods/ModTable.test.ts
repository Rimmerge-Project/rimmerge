import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import Menu from "primevue/menu";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import ModTable from "@/components/mods/ModTable.vue";
import { modRow as baseRow, type ModRowOverrides } from "@/components/mods/modListRow.test-support";
import { installMockIpc } from "@/services/ipc.mock";

function modRow(overrides: ModRowOverrides = {}) {
  return baseRow({ modId: "fixture.mod", name: "Fixture Mod", ...overrides });
}

let mountedWrappers: ReturnType<typeof mount>[] = [];

function mountTable(
  props: InstanceType<typeof ModTable>["$props"],
  names: Record<string, string> = {},
) {
  installMockIpc({ list_mod_names: names });
  const wrapper = mount(ModTable, {
    props,
    // Connected to `document.body` — happy-dom only dispatches `focus`
    // events (and updates `document.activeElement`) for elements that
    // are actually in the document, which `moveFocus`'s own roving-focus
    // tests below rely on.
    attachTo: document.body,
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      // The row Move menu is a popup PrimeVue teleports to the body.
      stubs: { teleport: true },
    },
  });
  mountedWrappers.push(wrapper);
  return wrapper;
}

describe("ModTable", () => {
  // `@tanstack/vue-virtual` measures the scroll container via
  // `offsetWidth`/`offsetHeight`, which happy-dom always reports as `0`
  // with no layout engine behind it — see `OrderTable.test.ts`'s
  // identical comment.
  beforeEach(() => {
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
    vi.restoreAllMocks();
    clearMocks();
    Reflect.deleteProperty(HTMLElement.prototype, "offsetHeight");
    Reflect.deleteProperty(HTMLElement.prototype, "offsetWidth");
    for (const wrapper of mountedWrappers) {
      wrapper.unmount();
    }
    mountedWrappers = [];
  });

  it("emits selectMod when a row is clicked, and renders no checkbox by default", async () => {
    const wrapper = mountTable({ mods: [modRow()] });
    await flushPromises();

    expect(wrapper.find('[data-testid="mod-row-select-fixture.mod"]').exists()).toBe(false);
    await wrapper.get('[data-testid="mod-row-fixture.mod"]').trigger("click");
    expect(wrapper.emitted("selectMod")).toEqual([["fixture.mod"]]);
  });

  it("marks the search match in the name the row is labelled with, and only there", async () => {
    const row = {
      ...modRow({ modId: "author.alpha", name: "Alpha Tools" }),
      search: { name: [{ start: 0, end: 5 }], id: [{ start: 7, end: 12 }] },
    };
    const wrapper = mountTable({ mods: [row] }, { "author.alpha": "Alpha Tools" });
    await flushPromises();

    const label = wrapper.get('[data-testid="mod-row-name-author.alpha"]');
    expect(label.text()).toBe("Alpha Tools");
    expect(label.findAll('[data-testid="search-match"]').map((mark) => mark.text())).toEqual([
      "Alpha",
    ]);
  });

  it("marks the id's match when the label falls back to the id", async () => {
    const row = {
      ...modRow({ modId: "author.alpha", name: "Alpha Tools" }),
      search: { name: [{ start: 0, end: 5 }], id: [{ start: 7, end: 12 }] },
    };
    // No resolved name, so the list labels the row with its id.
    const wrapper = mountTable({ mods: [row] });
    await flushPromises();

    const label = wrapper.get('[data-testid="mod-row-name-author.alpha"]');
    expect(label.text()).toBe("author.alpha");
    expect(label.findAll('[data-testid="search-match"]').map((mark) => mark.text())).toEqual([
      "alpha",
    ]);
  });

  it("draws no mark on a row without a search", async () => {
    const wrapper = mountTable({ mods: [modRow()] });
    await flushPromises();

    expect(wrapper.find('[data-testid="search-match"]').exists()).toBe(false);
  });

  it("renders a source badge per row instead of the raw source enum", async () => {
    const wrapper = mountTable({
      mods: [
        modRow({ modId: "a", source: "workshop" }),
        modRow({ modId: "b", source: "dlc" }),
        modRow({ modId: "c", source: null, missing: true }),
      ],
    });
    await flushPromises();

    const badgeTexts = wrapper
      .findAll('[data-testid="source-badge"]')
      .map((badge) => badge.attributes("aria-label"));
    expect(badgeTexts).toEqual([
      "Steam Workshop",
      "RimWorld DLC",
      "Origin unknown (not installed)",
    ]);
    // No element's text is the bare wire value, in any case.
    const elementTexts = wrapper
      .findAll('[role="row"] *')
      .map((element) => element.text().toLowerCase());
    expect(elementTexts).not.toContain("workshop");
    expect(elementTexts).not.toContain("dlc");
  });

  it("renders a checkbox per row when selectable, asking the owner to toggle it", async () => {
    const wrapper = mountTable({
      mods: [modRow({ modId: "a" }), modRow({ modId: "b" })],
      selectable: true,
      selectedIds: ["a"],
    });
    await flushPromises();

    const checkboxA = wrapper.get('[data-testid="mod-row-select-a"] input[type="checkbox"]');
    expect((checkboxA.element as HTMLInputElement).checked).toBe(true);
    const checkboxB = wrapper.get('[data-testid="mod-row-select-b"] input[type="checkbox"]');
    expect((checkboxB.element as HTMLInputElement).checked).toBe(false);

    await checkboxB.setValue(true);
    expect(wrapper.emitted("toggleSelect")).toEqual([["b"]]);
  });

  it("clicking a row's checkbox does not also emit selectMod", async () => {
    const wrapper = mountTable({ mods: [modRow()], selectable: true });
    await flushPromises();

    await wrapper.get('[data-testid="mod-row-select-fixture.mod"]').trigger("click");
    expect(wrapper.emitted("selectMod")).toBeUndefined();
  });

  it("shows the membership mark beside the source badge, and nothing on a row that matches the file", async () => {
    const wrapper = mountTable({
      mods: [
        modRow({ modId: "a", source: "workshop", membership: "added" }),
        modRow({ modId: "b", membership: "removed" }),
        modRow({ modId: "c" }),
      ],
    });
    await flushPromises();

    const rowA = wrapper.get('[data-testid="mod-row-a"]');
    expect(rowA.get('[data-testid="membership-badge"]').attributes("data-mark")).toBe("added");
    expect(rowA.find('[data-testid="source-badge"]').exists()).toBe(true);
    expect(
      wrapper.get('[data-testid="mod-row-b"]').get('[data-testid="membership-badge"]').text(),
    ).toContain("Removed");
    expect(
      wrapper.get('[data-testid="mod-row-c"]').find('[data-testid="membership-badge"]').exists(),
    ).toBe(false);
  });

  it("renders a missing pill only for rows whose own missing field is true", async () => {
    const wrapper = mountTable({
      mods: [modRow({ modId: "a", missing: true, source: null }), modRow({ modId: "b" })],
    });
    await flushPromises();

    const rowA = wrapper.get('[data-testid="mod-row-a"]');
    expect(rowA.find('[data-testid="mod-row-missing-pill"]').exists()).toBe(true);
    const rowB = wrapper.get('[data-testid="mod-row-b"]');
    expect(rowB.find('[data-testid="mod-row-missing-pill"]').exists()).toBe(false);
  });

  it("marks a row activated since the last scan, only where the active columns show", async () => {
    const wrapper = mountTable({ mods: [modRow({ modId: "a", pendingScan: true })] });
    await flushPromises();
    expect(wrapper.get('[data-testid="mod-row-pending-scan-pill"]').text()).toBe("not scanned yet");

    const inactive = mountTable({
      mods: [modRow({ modId: "a", pendingScan: true })],
      showActiveColumns: false,
    });
    await flushPromises();
    expect(inactive.find('[data-testid="mod-row-pending-scan-pill"]').exists()).toBe(false);
  });

  it("shows a dash, not 0, for a Hard-dependent count no scan has produced", async () => {
    const wrapper = mountTable({
      mods: [
        modRow({ modId: "scanned.mod", hardDependents: 0 }),
        modRow({ modId: "fresh.mod", pendingScan: true }),
        modRow({ modId: "gone.mod", source: null, missing: true }),
      ],
    });
    await flushPromises();

    const cellText = (modId: string) => wrapper.get(`[data-testid="mod-row-${modId}"]`).text();
    expect(cellText("scanned.mod")).toContain("0");
    expect(cellText("fresh.mod")).toContain("—");
    expect(cellText("gone.mod")).toContain("—");
  });

  it("hides the dependent count but still navigates when showActiveColumns is false", async () => {
    const wrapper = mountTable({
      mods: [modRow({ hardDependents: 7 })],
      showActiveColumns: false,
    });
    await flushPromises();

    const row = wrapper.get('[data-testid="mod-row-fixture.mod"]');
    expect(row.text()).not.toContain("7");

    await row.trigger("click");
    expect(wrapper.emitted("selectMod")).toEqual([["fixture.mod"]]);
  });

  it("only the first row is in the tab order before anything has been focused", async () => {
    const wrapper = mountTable({ mods: [modRow({ modId: "a" }), modRow({ modId: "b" })] });
    await flushPromises();

    expect(wrapper.get('[data-testid="mod-row-a"]').attributes("tabindex")).toBe("0");
    expect(wrapper.get('[data-testid="mod-row-b"]').attributes("tabindex")).toBe("-1");
  });

  it("ArrowDown moves roving focus to the next row and emits focusMod", async () => {
    const wrapper = mountTable({ mods: [modRow({ modId: "a" }), modRow({ modId: "b" })] });
    await flushPromises();

    await wrapper.get('[data-testid="mod-row-a"]').trigger("keydown", { key: "ArrowDown" });
    await flushPromises();

    expect(wrapper.emitted("focusMod")?.at(-1)).toEqual(["b"]);
    expect(wrapper.get('[data-testid="mod-row-a"]').attributes("tabindex")).toBe("-1");
    expect(wrapper.get('[data-testid="mod-row-b"]').attributes("tabindex")).toBe("0");
  });

  it("Enter selects the row immediately", async () => {
    const wrapper = mountTable({ mods: [modRow({ modId: "a" })] });
    await flushPromises();

    await wrapper.get('[data-testid="mod-row-a"]').trigger("keydown", { key: "Enter" });

    expect(wrapper.emitted("selectMod")).toEqual([["a"]]);
  });

  it("shows an update time past the date range as unknown instead of failing the render", async () => {
    const wrapper = mountTable({
      mods: [modRow({ modId: "a", updated: { kind: "known", seconds: 1e19 } })],
      showUpdated: true,
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="mod-row-updated-a"]').text()).toBe("Update time unknown");
  });

  it("titles the update date so a clipped one can be read", async () => {
    const wrapper = mountTable({
      mods: [modRow({ modId: "a", updated: { kind: "known", seconds: 1_700_000_000 } })],
      showUpdated: true,
    });
    await flushPromises();

    const cell = wrapper.get('[data-testid="mod-row-updated-a"]');
    expect(cell.attributes("title")).toBe(cell.text());
    expect(cell.text()).not.toBe("");
  });

  it("labels each row's load-order position when asked to show it", async () => {
    const wrapper = mountTable({
      mods: [modRow({ modId: "a", position: 4 })],
      showPosition: true,
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="mod-row-position-a"]').text()).toBe("#5");
    expect(wrapper.get('[data-testid="mod-row-a"]').text()).toContain("Load order position 5");
  });

  it("shows no position unless asked", async () => {
    const wrapper = mountTable({ mods: [modRow({ modId: "a", position: 4 })] });
    await flushPromises();

    expect(wrapper.find('[data-testid="mod-row-position-a"]').exists()).toBe(false);
  });

  it("Space toggles the row's own checkbox when selectable", async () => {
    const wrapper = mountTable({ mods: [modRow({ modId: "a" })], selectable: true });
    await flushPromises();

    await wrapper.get('[data-testid="mod-row-a"]').trigger("keydown", { key: " " });

    expect(wrapper.emitted("toggleSelect")).toEqual([["a"]]);
  });

  describe("multi-select gestures", () => {
    const THREE = ["a", "b", "c"].map((modId) => modRow({ modId }));

    it("a plain click opens the row and leaves the selection alone", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true, selectedIds: ["c"] });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-a"]').trigger("click");

      expect(wrapper.emitted("selectMod")).toEqual([["a"]]);
      expect(wrapper.emitted("anchorRow")).toEqual([["a"]]);
      expect(wrapper.emitted("toggleSelect")).toBeUndefined();
      expect(wrapper.emitted("rangeSelect")).toBeUndefined();
    });

    it("is a multi-select grid whose rows hold grid cells", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true, rowMenu: "move" });
      await flushPromises();

      const grid = wrapper.get('[data-testid="mod-table"]');
      expect(grid.attributes("role")).toBe("grid");
      expect(grid.attributes("aria-multiselectable")).toBe("true");
      expect(grid.attributes("aria-rowcount")).toBe("3");
      expect(wrapper.get('[data-testid="mod-row-c"]').attributes("aria-rowindex")).toBe("3");
      const row = wrapper.get('[data-testid="mod-row-a"]');
      const cells = row.findAll(':scope > [role="gridcell"]');
      expect(cells).toHaveLength(3);
      expect(row.findAll(":scope > *")).toHaveLength(3);
    });

    it("is not a multi-select grid when rows cannot be selected", async () => {
      const wrapper = mountTable({ mods: THREE });
      await flushPromises();

      expect(
        wrapper.get('[data-testid="mod-table"]').attributes("aria-multiselectable"),
      ).toBeUndefined();
    });

    it("Shift+click on a row clears the browser text selection", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true });
      await flushPromises();
      const removeAllRanges = vi.fn();
      vi.spyOn(window, "getSelection").mockReturnValue({ removeAllRanges } as unknown as Selection);

      await wrapper.get('[data-testid="mod-row-b"]').trigger("click", { shiftKey: true });

      expect(removeAllRanges).toHaveBeenCalledTimes(1);
    });

    it("Shift+click on a row checkbox extends a range instead of toggling", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true });
      await flushPromises();

      await wrapper
        .get('[data-testid="mod-row-select-c"] input[type="checkbox"]')
        .trigger("click", { shiftKey: true });

      expect(wrapper.emitted("rangeSelect")).toEqual([["c"]]);
      expect(wrapper.emitted("toggleSelect")).toBeUndefined();
    });

    it("plain arrows, Home and End move the anchor; Shift+Arrow does not", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true });
      await flushPromises();
      const rowA = wrapper.get('[data-testid="mod-row-a"]');

      await rowA.trigger("keydown", { key: "ArrowDown" });
      await flushPromises();
      await rowA.trigger("keydown", { key: "End" });
      await flushPromises();
      await rowA.trigger("keydown", { key: "Home" });
      await flushPromises();
      await rowA.trigger("keydown", { key: "ArrowDown", shiftKey: true });
      await flushPromises();

      expect(wrapper.emitted("anchorRow")).toEqual([["b"], ["c"], ["a"]]);
    });

    it("Ctrl+A and Escape work while a row checkbox has focus", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true, selectedIds: ["a"] });
      await flushPromises();
      const checkbox = wrapper.get('[data-testid="mod-row-select-a"] input[type="checkbox"]');

      await checkbox.trigger("keydown", { key: "a", ctrlKey: true });
      await checkbox.trigger("keydown", { key: "Escape" });

      expect(wrapper.emitted("selectAll")).toHaveLength(1);
      expect(wrapper.emitted("clearSelection")).toHaveLength(1);
    });

    it("Ctrl+A and Escape are left alone inside a dialog", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true, selectedIds: ["a"] });
      await flushPromises();
      const dialog = document.createElement("div");
      dialog.setAttribute("role", "dialog");
      const button = document.createElement("button");
      dialog.append(button);
      wrapper.get('[data-testid="mod-row-a"]').element.append(dialog);

      button.dispatchEvent(
        new KeyboardEvent("keydown", { key: "a", ctrlKey: true, bubbles: true }),
      );
      button.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));

      expect(wrapper.emitted("selectAll")).toBeUndefined();
      expect(wrapper.emitted("clearSelection")).toBeUndefined();
    });

    it("Ctrl+click and Cmd+click toggle the row without opening it", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-a"]').trigger("click", { ctrlKey: true });
      await wrapper.get('[data-testid="mod-row-b"]').trigger("click", { metaKey: true });

      expect(wrapper.emitted("toggleSelect")).toEqual([["a"], ["b"]]);
      expect(wrapper.emitted("selectMod")).toBeUndefined();
    });

    it("Shift+click asks for a range without opening the row", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-c"]').trigger("click", { shiftKey: true });

      expect(wrapper.emitted("rangeSelect")).toEqual([["c"]]);
      expect(wrapper.emitted("selectMod")).toBeUndefined();
    });

    it("a modifier click opens the row when the table is not selectable", async () => {
      const wrapper = mountTable({ mods: THREE });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-a"]').trigger("click", { ctrlKey: true });

      expect(wrapper.emitted("selectMod")).toEqual([["a"]]);
      expect(wrapper.emitted("toggleSelect")).toBeUndefined();
    });

    it("marks selected rows with aria-selected, and carries none on a non-selectable table", async () => {
      const selectable = mountTable({ mods: THREE, selectable: true, selectedIds: ["b"] });
      const plain = mountTable({ mods: THREE });
      await flushPromises();

      const flags = selectable
        .findAll('[role="row"]')
        .map((row) => row.attributes("aria-selected"));
      expect(flags).toEqual(["false", "true", "false"]);
      expect(plain.get('[data-testid="mod-row-a"]').attributes("aria-selected")).toBeUndefined();
    });

    it("Shift+Space asks for a range, plain Space for a toggle", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true });
      await flushPromises();

      await wrapper
        .get('[data-testid="mod-row-b"]')
        .trigger("keydown", { key: " ", shiftKey: true });

      expect(wrapper.emitted("rangeSelect")).toEqual([["b"]]);
      expect(wrapper.emitted("toggleSelect")).toBeUndefined();
    });

    it("Shift+ArrowDown moves focus and extends from the row it left", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true });
      await flushPromises();

      await wrapper
        .get('[data-testid="mod-row-a"]')
        .trigger("keydown", { key: "ArrowDown", shiftKey: true });
      await flushPromises();

      expect(wrapper.emitted("extendSelect")).toEqual([["a", "b"]]);
      expect(wrapper.emitted("focusMod")).toEqual([["b"]]);
    });

    it("Shift+ArrowDown on the last row extends nothing", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-c"]').trigger("focus");
      await wrapper
        .get('[data-testid="mod-row-c"]')
        .trigger("keydown", { key: "ArrowDown", shiftKey: true });
      await flushPromises();

      expect(wrapper.emitted("extendSelect")).toBeUndefined();
    });

    it("Ctrl+A and Cmd+A select every shown row", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true });
      await flushPromises();

      await wrapper
        .get('[data-testid="mod-row-a"]')
        .trigger("keydown", { key: "a", ctrlKey: true });
      await wrapper
        .get('[data-testid="mod-row-a"]')
        .trigger("keydown", { key: "A", metaKey: true });

      expect(wrapper.emitted("selectAll")).toHaveLength(2);
    });

    it("Ctrl+A from a text field inside the table is left to the field", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true });
      await flushPromises();
      const input = document.createElement("input");
      wrapper.get('[data-testid="mod-row-a"]').element.append(input);

      input.dispatchEvent(new KeyboardEvent("keydown", { key: "a", ctrlKey: true, bubbles: true }));

      expect(wrapper.emitted("selectAll")).toBeUndefined();
    });

    it("Escape clears a non-empty selection and does not bubble on to close the panel", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true, selectedIds: ["a"] });
      await flushPromises();
      let reachedWindow = false;
      const listener = () => {
        reachedWindow = true;
      };
      window.addEventListener("keydown", listener);

      await wrapper.get('[data-testid="mod-row-a"]').trigger("keydown", { key: "Escape" });
      window.removeEventListener("keydown", listener);

      expect(wrapper.emitted("clearSelection")).toHaveLength(1);
      expect(reachedWindow).toBe(false);
    });

    it("Escape with nothing selected is left to bubble", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true });
      await flushPromises();
      let reachedWindow = false;
      const listener = () => {
        reachedWindow = true;
      };
      window.addEventListener("keydown", listener);

      await wrapper.get('[data-testid="mod-row-a"]').trigger("keydown", { key: "Escape" });
      window.removeEventListener("keydown", listener);

      expect(wrapper.emitted("clearSelection")).toBeUndefined();
      expect(reachedWindow).toBe(true);
    });

    it("Enter on a focused row menu button emits no selectMod", async () => {
      const wrapper = mountTable({ mods: THREE, selectable: true, rowMenu: "move" });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-menu-a"]').trigger("keydown", { key: "Enter" });

      expect(wrapper.emitted("selectMod")).toBeUndefined();
    });
  });

  describe("dragging and dropping", () => {
    const FOUR = ["a", "b", "c", "d"].map((modId) => modRow({ modId }));

    /** A cancelable drag event at `clientY`, dispatched on `element`; returned to read `defaultPrevented`. */
    function dragEvent(element: Element, type: string, clientY: number): Event {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "clientY", { value: clientY });
      element.dispatchEvent(event);
      return event;
    }

    it("makes rows draggable only when asked", async () => {
      const off = mountTable({ mods: FOUR });
      const on = mountTable({ mods: FOUR, draggable: true });
      await flushPromises();

      expect(off.get('[data-testid="mod-row-a"]').attributes("draggable")).toBe("false");
      expect(on.get('[data-testid="mod-row-a"]').attributes("draggable")).toBe("true");
    });

    it("emits dragStart with the row's id and dragEnd when it ends", async () => {
      const wrapper = mountTable({ mods: FOUR, draggable: true });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-b"]').trigger("dragstart");
      await wrapper.get('[data-testid="mod-row-b"]').trigger("dragend");

      expect(wrapper.emitted("dragStart")).toEqual([["b"]]);
      expect(wrapper.emitted("dragEnd")).toHaveLength(1);
    });

    it("shows a drop line in the gap nearest the pointer and accepts the drop there", async () => {
      const wrapper = mountTable({ mods: FOUR, dropMode: "position" });
      await flushPromises();
      const table = wrapper.get('[data-testid="mod-table"]').element;

      // 40px rows: y = 61 is nearest the gap after row 1 (slot 2).
      const over = dragEvent(table, "dragover", 61);
      await flushPromises();

      expect(over.defaultPrevented).toBe(true);
      expect(wrapper.get('[data-testid="mod-table-drop-indicator"]').attributes("data-slot")).toBe(
        "2",
      );

      dragEvent(table, "drop", 61);
      await flushPromises();
      expect(wrapper.emitted("drop")).toEqual([[2]]);
      expect(wrapper.find('[data-testid="mod-table-drop-indicator"]').exists()).toBe(false);
    });

    it("rounds the pointer to the gap above a row's top half and below its bottom half", async () => {
      const wrapper = mountTable({ mods: FOUR, dropMode: "position" });
      await flushPromises();
      const table = wrapper.get('[data-testid="mod-table"]').element;

      dragEvent(table, "drop", 19);
      dragEvent(table, "drop", 21);
      dragEvent(table, "drop", 9999);

      expect(wrapper.emitted("drop")).toEqual([[0], [1], [4]]);
    });

    it("accepts a drop anywhere on a pane that has no order of its own, with no line", async () => {
      const wrapper = mountTable({ mods: FOUR, dropMode: "pane" });
      await flushPromises();
      const table = wrapper.get('[data-testid="mod-table"]').element;

      const over = dragEvent(table, "dragover", 50);
      dragEvent(table, "drop", 50);

      expect(over.defaultPrevented).toBe(true);
      expect(wrapper.find('[data-testid="mod-table-drop-indicator"]').exists()).toBe(false);
      expect(wrapper.emitted("drop")).toEqual([[4]]);
    });

    it("refuses a drag it must not take: no cancel, no drop, the reason shown", async () => {
      const wrapper = mountTable({
        mods: FOUR,
        dropMode: "refused",
        refusedText: "Core can't be deactivated.",
      });
      await flushPromises();
      const table = wrapper.get('[data-testid="mod-table"]').element;

      const over = dragEvent(table, "dragover", 50);
      await flushPromises();
      dragEvent(table, "drop", 50);

      expect(over.defaultPrevented).toBe(false);
      expect(wrapper.emitted("drop")).toBeUndefined();
      expect(wrapper.get('[data-testid="mod-table-refused-text"]').text()).toBe(
        "Core can't be deactivated.",
      );
    });

    it("ignores a drag while none is in progress", async () => {
      const wrapper = mountTable({ mods: FOUR, dropMode: "off" });
      await flushPromises();
      const table = wrapper.get('[data-testid="mod-table"]').element;

      const over = dragEvent(table, "dragover", 50);
      dragEvent(table, "drop", 50);

      expect(over.defaultPrevented).toBe(false);
      expect(wrapper.emitted("drop")).toBeUndefined();
    });
  });

  describe("moving a row from the keyboard and the row menu", () => {
    /** PrimeVue puts `aria-disabled` on the menu item the entry's link sits in. */
    function isMenuItemDisabled(wrapper: ReturnType<typeof mountTable>, id: string): boolean {
      const link = wrapper.get(`[data-testid="mod-move-${id}"]`).element;
      return link.closest("li")?.getAttribute("aria-disabled") === "true";
    }

    const THREE = ["a", "b", "c"].map((modId) => modRow({ modId }));

    it("Alt+ArrowUp and Alt+ArrowDown ask to move the row, and leave the roving focus alone", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move", selectable: true });
      await flushPromises();

      await wrapper
        .get('[data-testid="mod-row-b"]')
        .trigger("keydown", { key: "ArrowUp", altKey: true });
      await wrapper
        .get('[data-testid="mod-row-b"]')
        .trigger("keydown", { key: "ArrowDown", altKey: true });

      expect(wrapper.emitted("moveStep")).toEqual([
        ["b", "up"],
        ["b", "down"],
      ]);
      expect(wrapper.emitted("focusMod")).toBeUndefined();
    });

    it("plain arrows still move the roving focus and never ask to move the row", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move" });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-a"]').trigger("keydown", { key: "ArrowDown" });

      expect(wrapper.emitted("moveStep")).toBeUndefined();
      expect(wrapper.emitted("focusMod")).toEqual([["b"]]);
    });

    it("does nothing on Alt+Arrow in a list with no Move menu", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "activate" });
      await flushPromises();

      await wrapper
        .get('[data-testid="mod-row-b"]')
        .trigger("keydown", { key: "ArrowUp", altKey: true });

      expect(wrapper.emitted("moveStep")).toBeUndefined();
      expect(wrapper.emitted("focusMod")).toBeUndefined();
    });

    it("leaves Alt+Arrow to a text field or dialog inside the row (the inert rule)", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move", selectable: true });
      await flushPromises();

      await wrapper
        .get('[data-testid="mod-row-select-b"] input[type="checkbox"]')
        .trigger("keydown", { key: "ArrowUp", altKey: true });

      expect(wrapper.emitted("moveStep")).toBeUndefined();
    });

    it("opens a menu with Activate at from an inactive row's button while organization is usable", async () => {
      const wrapper = mountTable({
        mods: THREE,
        rowMenu: "activate",
        hasOrganizationActions: true,
      });
      await flushPromises();

      const button = wrapper.get('[data-testid="mod-row-menu-b"]');
      expect(button.attributes("aria-label")).toBe("Actions for b");
      await button.trigger("click");
      await flushPromises();
      await wrapper.get('[data-testid="mod-activate-at"]').trigger("click");

      expect(wrapper.emitted("menuAction")).toEqual([["b", "activateAt"]]);
      expect(wrapper.emitted("selectMod")).toBeUndefined();
    });

    it("runs Activate at straight from an inactive row's button while the menu would hold only it", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "activate" });
      await flushPromises();

      const button = wrapper.get('[data-testid="mod-row-menu-b"]');
      expect(button.attributes("aria-label")).toBe("Activate b at…");
      expect(button.attributes("aria-haspopup")).toBe("dialog");
      await button.trigger("click");
      await flushPromises();

      expect(wrapper.emitted("menuAction")).toEqual([["b", "activateAt"]]);
      expect(wrapper.find('[data-testid="mod-activate-at"]').exists()).toBe(false);
      expect(wrapper.emitted("selectMod")).toBeUndefined();
    });

    it("runs Activate at straight from the menu key on an inactive row while organization is off", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "activate" });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-b"]').trigger("keydown", { key: "ContextMenu" });
      await flushPromises();

      expect(wrapper.emitted("menuAction")).toEqual([["b", "activateAt"]]);
    });

    it("offers Show in load order on an Active row only when told to, and emits it", async () => {
      const without = mountTable({ mods: THREE, rowMenu: "move" });
      const offered = mountTable({ mods: THREE, rowMenu: "move", canShowInLoadOrder: true });
      const inactive = mountTable({
        mods: THREE,
        rowMenu: "activate",
        hasOrganizationActions: true,
        canShowInLoadOrder: true,
      });
      await flushPromises();

      await without.get('[data-testid="mod-row-menu-b"]').trigger("click");
      await flushPromises();
      expect(without.find('[data-testid="mod-show-in-load-order"]').exists()).toBe(false);
      await offered.get('[data-testid="mod-row-menu-b"]').trigger("click");
      await flushPromises();
      await offered.get('[data-testid="mod-show-in-load-order"]').trigger("click");

      expect(offered.emitted("menuAction")).toEqual([["b", "showInLoadOrder"]]);
      // An Inactive row has no load-order position, so its menu never carries the entry.
      await inactive.get('[data-testid="mod-row-menu-b"]').trigger("click");
      await flushPromises();
      expect(inactive.find('[data-testid="mod-show-in-load-order"]').exists()).toBe(false);
    });

    it("adds Groups and Colour to the row menu only while organization is usable", async () => {
      const without = mountTable({ mods: THREE, rowMenu: "move" });
      const withOrganization = mountTable({
        mods: THREE,
        rowMenu: "move",
        hasOrganizationActions: true,
      });
      await flushPromises();

      await without.get('[data-testid="mod-row-menu-b"]').trigger("click");
      await withOrganization.get('[data-testid="mod-row-menu-b"]').trigger("click");
      await flushPromises();
      expect(without.find('[data-testid="mod-menu-groups"]').exists()).toBe(false);
      await withOrganization.get('[data-testid="mod-menu-groups"]').trigger("click");
      await withOrganization.get('[data-testid="mod-row-menu-b"]').trigger("click");
      await flushPromises();
      await withOrganization.get('[data-testid="mod-menu-color"]').trigger("click");

      expect(withOrganization.emitted("menuAction")).toEqual([
        ["b", "groups"],
        ["b", "color"],
      ]);
      expect(withOrganization.emitted("moveStep")).toBeUndefined();
    });

    it("gives a list without position actions a menu for the organization entries alone", async () => {
      const wrapper = mountTable({ mods: THREE, hasOrganizationActions: true });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-menu-a"]').trigger("click");
      await flushPromises();

      expect(wrapper.find('[data-testid="mod-move-up"]').exists()).toBe(false);
      expect(wrapper.find('[data-testid="mod-menu-groups"]').exists()).toBe(true);
    });

    it("opens a Move menu from an active row's button without opening the mod", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move" });
      await flushPromises();

      expect(wrapper.get('[data-testid="mod-row-menu-b"]').attributes("aria-label")).toBe(
        "Actions for b",
      );
      await wrapper.get('[data-testid="mod-row-menu-b"]').trigger("click");
      await flushPromises();

      const texts = wrapper.findAll('[data-testid^="mod-move-"]').map((item) => item.text());
      expect(texts).toEqual(
        expect.arrayContaining([
          "Move up",
          "Move down",
          "Move to top",
          "Move to bottom",
          "Move after…",
        ]),
      );
      expect(wrapper.emitted("selectMod")).toBeUndefined();
    });

    it("emits the chosen menu entry", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move" });
      await flushPromises();
      await wrapper.get('[data-testid="mod-row-menu-b"]').trigger("click");
      await flushPromises();

      await wrapper.get('[data-testid="mod-move-top"]').trigger("click");
      await wrapper.get('[data-testid="mod-row-menu-b"]').trigger("click");
      await flushPromises();
      await wrapper.get('[data-testid="mod-move-up"]').trigger("click");

      expect(wrapper.emitted("menuAction")).toEqual([["b", "top"]]);
      expect(wrapper.emitted("moveStep")).toEqual([["b", "up"]]);
    });

    it("keeps the row buttons out of the Tab order, one stop per row", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move" });
      await flushPromises();

      expect(wrapper.get('[data-testid="mod-row-menu-b"]').attributes("tabindex")).toBe("-1");
    });

    it.each([
      ["ContextMenu", { key: "ContextMenu" }],
      ["Shift+F10", { key: "F10", shiftKey: true }],
    ])("opens a row's Move menu from the %s key", async (_name, key) => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move" });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-b"]').trigger("keydown", key);
      await flushPromises();

      expect(wrapper.find('[data-testid="mod-move-top"]').exists()).toBe(true);
      await wrapper.get('[data-testid="mod-move-top"]').trigger("click");
      expect(wrapper.emitted("menuAction")).toEqual([["b", "top"]]);
    });

    /**
     * PrimeVue focuses the popup list and then emits `show` from its enter hook. happy-dom cannot
     * lay the overlay out, so the hook never runs; this stands in for it, in the same order.
     */
    function showMenu(wrapper: ReturnType<typeof mountTable>): void {
      (wrapper.get('[role="menu"]').element as HTMLElement).focus();
      wrapper.findComponent(Menu).vm.$emit("show");
    }

    it("marks the first menu item when the menu opens from the keyboard, so Enter works at once", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move" });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-b"]').trigger("keydown", { key: "ContextMenu" });
      showMenu(wrapper);
      await flushPromises();

      const list = wrapper.get('[role="menu"]');
      const first = wrapper.get('[data-testid="mod-move-up"]').element.closest("li");
      expect(list.attributes("aria-activedescendant")).toBe(first?.id);
      await list.trigger("keydown", { key: "Enter", code: "Enter" });
      expect(wrapper.emitted("moveStep")).toEqual([["b", "up"]]);
    });

    it("leaves the menu unmarked when a mouse click opens it", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move" });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-menu-b"]').trigger("click", { detail: 1 });
      showMenu(wrapper);
      await flushPromises();

      expect(wrapper.get('[role="menu"]').attributes("aria-activedescendant")).toBeUndefined();
    });

    it("opens the menu with Activate at from the menu key on an inactive row while organization is usable", async () => {
      const wrapper = mountTable({
        mods: THREE,
        rowMenu: "activate",
        hasOrganizationActions: true,
      });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-b"]').trigger("keydown", { key: "ContextMenu" });
      await flushPromises();
      await wrapper.get('[data-testid="mod-activate-at"]').trigger("click");

      expect(wrapper.emitted("menuAction")).toEqual([["b", "activateAt"]]);
    });

    it("leaves a plain F10 and a list with no row menu alone", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move" });
      const bare = mountTable({ mods: THREE });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-b"]').trigger("keydown", { key: "F10" });
      await bare.get('[data-testid="mod-row-b"]').trigger("keydown", { key: "ContextMenu" });
      await flushPromises();

      expect(wrapper.find('[data-testid="mod-move-top"]').exists()).toBe(false);
      expect(bare.emitted("menuAction")).toBeUndefined();
    });

    it("leaves the menu key to a text field or dialog inside the row (the inert rule)", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "activate", selectable: true });
      await flushPromises();

      await wrapper
        .get('[data-testid="mod-row-select-b"] input[type="checkbox"]')
        .trigger("keydown", { key: "ContextMenu" });

      expect(wrapper.emitted("menuAction")).toBeUndefined();
    });

    it("disables a step off the first row", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move" });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-menu-a"]').trigger("click");
      await flushPromises();

      expect(isMenuItemDisabled(wrapper, "up")).toBe(true);
      expect(isMenuItemDisabled(wrapper, "down")).toBe(false);
    });

    it("disables every move and says why while reordering is off", async () => {
      const wrapper = mountTable({
        mods: THREE,
        rowMenu: "move",
        reorderBlockedReason: "Sort by load order to move mods.",
      });
      await flushPromises();
      await wrapper.get('[data-testid="mod-row-menu-b"]').trigger("click");
      await flushPromises();

      for (const id of ["up", "down", "top", "bottom", "after"]) {
        expect(isMenuItemDisabled(wrapper, id)).toBe(true);
      }
      expect(wrapper.get('[data-testid="mod-move-menu-reason"]').text()).toBe(
        "Sort by load order to move mods.",
      );
    });
  });

  describe("organization on a row", () => {
    const WEAPONS = { id: 1, name: "Weapons", color: "red" as const };
    const ARMOR = { id: 2, name: "Armor", color: null };
    const LOOT = { id: 3, name: "Loot", color: "blue" as const };

    it("draws the mod's own colour as a stripe and also names it in text", async () => {
      const wrapper = mountTable({ mods: [modRow({ modId: "a", color: "teal" })] });
      await flushPromises();

      const row = wrapper.get('[data-testid="mod-row-a"]');
      expect(row.get('[data-testid="mod-row-color"]').attributes("data-color")).toBe("teal");
      expect(row.get('[data-testid="mod-row-color"]').classes()).toContain("bg-mark-teal");
      expect(row.text()).toContain("Colour: Teal");
    });

    it("puts the colour name inside the name cell, never as text straight in the row", async () => {
      const wrapper = mountTable({ mods: [modRow({ modId: "a", color: "teal" })] });
      await flushPromises();

      const row = wrapper.get('[data-testid="mod-row-a"]');
      const name = row.get('[data-testid="mod-row-color-name"]');
      expect(name.element.closest('[role="gridcell"]')).not.toBeNull();
      expect(name.element.parentElement?.closest('[role="row"]')).toBe(row.element);
      expect(name.text()).toBe("Colour: Teal");
      expect(row.get('[data-testid="mod-row-color"]').attributes("aria-hidden")).toBe("true");
      const strayChildren = [...row.element.children].filter(
        (child) =>
          child.getAttribute("role") === null && child.getAttribute("aria-hidden") !== "true",
      );
      expect(strayChildren).toEqual([]);
    });

    it("draws no stripe for an uncoloured mod", async () => {
      const wrapper = mountTable({ mods: [modRow({ modId: "a" })] });
      await flushPromises();

      expect(wrapper.find('[data-testid="mod-row-color"]').exists()).toBe(false);
    });

    it("shows at most two group chips by name and the rest as +N with every name in the tooltip", async () => {
      const wrapper = mountTable({
        mods: [modRow({ modId: "a", groups: [WEAPONS, ARMOR, LOOT] })],
      });
      await flushPromises();

      const chips = wrapper.findAll('[data-testid^="mod-row-group-"]:not([data-testid$="more"])');
      expect(chips.map((chip) => chip.text())).toEqual(["Weapons", "Armor"]);
      expect(chips[0]?.attributes("title")).toBe("Show only the mods in the group Weapons (Red).");
      expect(chips[1]?.attributes("title")).toBe("Show only the mods in the group Armor.");
      const more = wrapper.get('[data-testid="mod-row-groups-more"]');
      expect(more.text()).toBe("+1");
      expect(more.attributes("title")).toBe("In groups: Weapons, Armor, and Loot");
    });

    it("names every group as text for screen readers at every width, once", async () => {
      const wrapper = mountTable({
        mods: [modRow({ modId: "a", groups: [WEAPONS, ARMOR, LOOT] })],
      });
      await flushPromises();

      const text = wrapper.get('[data-testid="mod-row-groups-text"]');
      expect(text.text()).toBe("In groups: Weapons, Armor, and Loot");
      expect(text.element.closest('[role="gridcell"]')).not.toBeNull();
      expect(wrapper.get('[data-testid="mod-row-groups"]').attributes("aria-hidden")).toBe("true");
    });

    it("names the mod's current colour in the row menu entry", async () => {
      const coloured = mountTable({
        mods: [modRow({ modId: "a", color: "teal" })],
        rowMenu: "move",
        hasOrganizationActions: true,
      });
      const plain = mountTable({
        mods: [modRow({ modId: "b" })],
        rowMenu: "move",
        hasOrganizationActions: true,
      });
      await flushPromises();

      await coloured.get('[data-testid="mod-row-menu-a"]').trigger("click");
      await plain.get('[data-testid="mod-row-menu-b"]').trigger("click");
      await flushPromises();

      expect(coloured.get('[data-testid="mod-menu-color"]').text()).toBe("Colour (Teal)…");
      expect(plain.get('[data-testid="mod-menu-color"]').text()).toBe("Colour…");
    });

    it("shows no chips for a mod in no group", async () => {
      const wrapper = mountTable({ mods: [modRow({ modId: "a" })] });
      await flushPromises();

      expect(wrapper.find('[data-testid="mod-row-groups"]').exists()).toBe(false);
    });

    it("shows manual tags on an unscanned row even where the dependent column is off", async () => {
      const wrapper = mountTable({
        mods: [modRow({ modId: "idle", pendingScan: true, manualTags: ["favourite"] })],
        showActiveColumns: false,
      });
      await flushPromises();

      expect(wrapper.get('[data-testid="mod-row-tags"]').text()).toBe("favourite");
    });

    it("keeps the dependent count off the Inactive list and on the Active list", async () => {
      const inactive = mountTable({
        mods: [modRow({ modId: "a", tags: ["x"] })],
        showActiveColumns: false,
      });
      const active = mountTable({
        mods: [modRow({ modId: "a", tags: ["x"], hardDependents: 3 })],
      });
      await flushPromises();

      expect(inactive.get('[data-testid="mod-row-tags"]').text()).toBe("x");
      expect(inactive.get('[data-testid="mod-row-a"]').text()).not.toContain("3");
      expect(active.get('[data-testid="mod-row-a"]').text()).toContain("3");
    });
  });
  describe("open row, double click, swatches and group scope", () => {
    const GROUPS = [
      { id: 1, name: "Weapons", color: "red" as const },
      { id: 2, name: "Armor", color: null },
    ];

    it("draws the open mod's row with the accent ring and tint, and marks it current", async () => {
      const wrapper = mountTable({
        mods: [modRow({ modId: "a" }), modRow({ modId: "b" })],
        selectable: true,
        openModId: "a",
      });
      await flushPromises();

      const open = wrapper.get('[data-testid="mod-row-a"]');
      expect(open.attributes("aria-current")).toBe("true");
      expect(open.classes()).toEqual(expect.arrayContaining(["bg-accent-soft", "inset-ring-2"]));
      const other = wrapper.get('[data-testid="mod-row-b"]');
      expect(other.attributes("aria-current")).toBeUndefined();
      expect(other.classes()).not.toContain("inset-ring-2");
    });

    it("follows the open mod, not the last focused row", async () => {
      const wrapper = mountTable({
        mods: [modRow({ modId: "a" }), modRow({ modId: "b" })],
        openModId: "a",
      });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-b"]').trigger("focus");

      expect(wrapper.get('[data-testid="mod-row-a"]').attributes("aria-current")).toBe("true");
      expect(wrapper.get('[data-testid="mod-row-b"]').attributes("aria-current")).toBeUndefined();
    });

    it("keeps the ring on a selected row and the tint only on the open one", async () => {
      const wrapper = mountTable({
        mods: [modRow({ modId: "a" }), modRow({ modId: "b" })],
        selectable: true,
        selectedIds: ["b"],
        openModId: "a",
      });
      await flushPromises();

      const selected = wrapper.get('[data-testid="mod-row-b"]');
      expect(selected.classes()).toContain("inset-ring-2");
      expect(selected.classes()).not.toContain("bg-accent-soft");
    });

    it("asks to toggle activation on a plain double click", async () => {
      const wrapper = mountTable({ mods: [modRow({ modId: "a" })], selectable: true });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-a"]').trigger("dblclick");

      expect(wrapper.emitted("toggleActive")).toEqual([["a"]]);
    });

    it("leaves a Ctrl or Shift double click to selection", async () => {
      const wrapper = mountTable({ mods: [modRow({ modId: "a" })], selectable: true });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-a"]').trigger("dblclick", { ctrlKey: true });
      await wrapper.get('[data-testid="mod-row-a"]').trigger("dblclick", { shiftKey: true });

      expect(wrapper.emitted("toggleActive")).toBeUndefined();
    });

    it("ignores a double click on the row checkbox", async () => {
      const wrapper = mountTable({ mods: [modRow({ modId: "a" })], selectable: true });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-select-a"]').trigger("dblclick");
      await wrapper.get('[data-testid="mod-row-select-a"] input').trigger("dblclick");

      expect(wrapper.emitted("toggleActive")).toBeUndefined();
    });

    it("ignores a double click on the row menu button", async () => {
      const wrapper = mountTable({
        mods: [modRow({ modId: "a" })],
        selectable: true,
        rowMenu: "move",
      });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-menu-a"]').trigger("dblclick");

      expect(wrapper.emitted("toggleActive")).toBeUndefined();
    });

    it("sets a colour from a swatch in the row menu with one click", async () => {
      const wrapper = mountTable({
        mods: [modRow({ modId: "a", color: "teal" })],
        rowMenu: "move",
        hasOrganizationActions: true,
      });
      await flushPromises();
      await wrapper.get('[data-testid="mod-row-menu-a"]').trigger("click");
      await flushPromises();

      const swatch = wrapper.get('[data-testid="mod-menu-swatch-blue"]');
      expect(swatch.attributes("aria-label")).toBe("Colour Blue");
      await swatch.trigger("click");

      expect(wrapper.emitted("setColor")).toEqual([["a", "blue"]]);
    });

    it("clears the colour from the no-colour swatch and marks the current one", async () => {
      const wrapper = mountTable({
        mods: [modRow({ modId: "a", color: "teal" })],
        rowMenu: "move",
        hasOrganizationActions: true,
      });
      await flushPromises();
      await wrapper.get('[data-testid="mod-row-menu-a"]').trigger("click");
      await flushPromises();

      expect(wrapper.get('[data-testid="mod-menu-swatch-teal"]').attributes("aria-pressed")).toBe(
        "true",
      );
      await wrapper.get('[data-testid="mod-menu-swatch-none"]').trigger("click");

      expect(wrapper.emitted("setColor")).toEqual([["a", null]]);
    });

    it("offers no swatches while organization is not usable", async () => {
      const wrapper = mountTable({ mods: [modRow({ modId: "a" })], rowMenu: "move" });
      await flushPromises();
      await wrapper.get('[data-testid="mod-row-menu-a"]').trigger("click");
      await flushPromises();

      expect(wrapper.find('[data-testid="mod-menu-swatches"]').exists()).toBe(false);
    });

    it("scopes to a group on a chip or dot click without opening the row", async () => {
      const wrapper = mountTable({ mods: [modRow({ modId: "a", groups: GROUPS })] });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-group-2"]').trigger("click");
      await wrapper.get('[data-testid="mod-row-dot-1"]').trigger("click");

      expect(wrapper.emitted("scopeGroup")).toEqual([[2], [1]]);
      expect(wrapper.emitted("selectMod")).toBeUndefined();
    });

    it("draws an inherited stripe faded and names the group as its source", async () => {
      const wrapper = mountTable({ mods: [modRow({ modId: "a", groups: GROUPS })] });
      await flushPromises();

      const stripe = wrapper.get('[data-testid="mod-row-color"]');
      expect(stripe.attributes("data-color")).toBe("red");
      expect(stripe.attributes("data-inherited")).toBe("true");
      expect(stripe.classes()).toContain("opacity-40");
      expect(wrapper.get('[data-testid="mod-row-color-name"]').text()).toBe(
        "Colour from group Weapons: Red",
      );
    });

    it("prefers the mod's own colour over its groups' and draws it solid", async () => {
      const wrapper = mountTable({
        mods: [modRow({ modId: "a", color: "teal", groups: GROUPS })],
      });
      await flushPromises();

      const stripe = wrapper.get('[data-testid="mod-row-color"]');
      expect(stripe.attributes("data-color")).toBe("teal");
      expect(stripe.attributes("data-inherited")).toBeUndefined();
      expect(wrapper.get('[data-testid="mod-row-color-name"]').text()).toBe("Colour: Teal");
    });

    it("draws no stripe when no group of the mod has a colour", async () => {
      const wrapper = mountTable({
        mods: [modRow({ modId: "a", groups: [{ id: 2, name: "Armor", color: null }] })],
      });
      await flushPromises();

      expect(wrapper.find('[data-testid="mod-row-color"]').exists()).toBe(false);
    });

    it("collapses chips to dots with an outlined one for an uncoloured group", async () => {
      const wrapper = mountTable({ mods: [modRow({ modId: "a", groups: GROUPS })] });
      await flushPromises();

      const dots = wrapper.get('[data-testid="mod-row-dots"]');
      expect(dots.attributes("aria-hidden")).toBe("true");
      expect(dots.get('[data-testid="mod-row-dot-1"] span').classes()).toContain("bg-mark-red");
      expect(dots.get('[data-testid="mod-row-dot-2"] span').classes()).toContain("border");
    });
  });
});
