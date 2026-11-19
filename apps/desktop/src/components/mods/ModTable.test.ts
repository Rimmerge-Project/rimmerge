import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import ModTable from "@/components/mods/ModTable.vue";
import { modRow as baseRow, type ModRowOverrides } from "@/components/mods/modListRow.test-support";
import { installMockIpc } from "@/services/ipc.mock";

function modRow(overrides: ModRowOverrides = {}) {
  return baseRow({ modId: "fixture.mod", name: "Fixture Mod", ...overrides });
}

let mountedWrappers: ReturnType<typeof mount>[] = [];

function mountTable(props: InstanceType<typeof ModTable>["$props"]) {
  installMockIpc({ list_mod_names: {} });
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

  it("renders a checkbox per row when selectable, toggling the selectedIds model", async () => {
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
    expect(wrapper.emitted("update:selectedIds")?.at(-1)).toEqual([["a", "b"]]);
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

  it("hides tags/hardDependents but still navigates when showActiveColumns is false", async () => {
    const wrapper = mountTable({
      mods: [modRow({ tags: ["framework"] })],
      showActiveColumns: false,
    });
    await flushPromises();

    const row = wrapper.get('[data-testid="mod-row-fixture.mod"]');
    expect(row.text()).not.toContain("framework");

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

    expect(wrapper.emitted("update:selectedIds")?.at(-1)).toEqual([["a"]]);
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

    it("goes straight to Activate at from an inactive row's button", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "activate" });
      await flushPromises();

      const button = wrapper.get('[data-testid="mod-row-menu-b"]');
      expect(button.attributes("aria-label")).toBe("Activate b at…");
      await button.trigger("click");

      expect(wrapper.emitted("menuAction")).toEqual([["b", "activateAt"]]);
      expect(wrapper.emitted("selectMod")).toBeUndefined();
    });

    it("opens a Move menu from an active row's button without opening the mod", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "move" });
      await flushPromises();

      expect(wrapper.get('[data-testid="mod-row-menu-b"]').attributes("aria-label")).toBe("Move b");
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

    it("goes to Activate at from the menu key on an inactive row", async () => {
      const wrapper = mountTable({ mods: THREE, rowMenu: "activate" });
      await flushPromises();

      await wrapper.get('[data-testid="mod-row-b"]').trigger("keydown", { key: "ContextMenu" });

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
});
