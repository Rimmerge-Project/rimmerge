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
    global: { plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]] },
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

  it("Space toggles the row's own checkbox when selectable", async () => {
    const wrapper = mountTable({ mods: [modRow({ modId: "a" })], selectable: true });
    await flushPromises();

    await wrapper.get('[data-testid="mod-row-a"]').trigger("keydown", { key: " " });

    expect(wrapper.emitted("update:selectedIds")?.at(-1)).toEqual([["a"]]);
  });
});
