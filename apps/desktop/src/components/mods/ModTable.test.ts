import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import ModTable from "@/components/mods/ModTable.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ModSummaryDto } from "@/types/generated/ModSummaryDto";

function mod(overrides: Partial<ModSummaryDto> = {}): ModSummaryDto {
  return {
    modId: "fixture.mod",
    name: "Fixture Mod",
    source: "local",
    tags: [],
    hardDependents: 0,
    missing: false,
    generated: null,
    workshopId: null,
    ...overrides,
  };
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
    const wrapper = mountTable({ mods: [mod()] });
    await flushPromises();

    expect(wrapper.find('[data-testid="mod-row-select-fixture.mod"]').exists()).toBe(false);
    await wrapper.get('[data-testid="mod-row-fixture.mod"]').trigger("click");
    expect(wrapper.emitted("selectMod")).toEqual([["fixture.mod"]]);
  });

  it("renders a checkbox per row when selectable, toggling the selectedIds model", async () => {
    const wrapper = mountTable({
      mods: [mod({ modId: "a" }), mod({ modId: "b" })],
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
    const wrapper = mountTable({ mods: [mod()], selectable: true });
    await flushPromises();

    await wrapper.get('[data-testid="mod-row-select-fixture.mod"]').trigger("click");
    expect(wrapper.emitted("selectMod")).toBeUndefined();
  });

  it("renders a missing pill only for rows whose own missing field is true", async () => {
    const wrapper = mountTable({
      mods: [mod({ modId: "a", missing: true, source: null }), mod({ modId: "b" })],
    });
    await flushPromises();

    const rowA = wrapper.get('[data-testid="mod-row-a"]');
    expect(rowA.find('[data-testid="mod-row-missing-pill"]').exists()).toBe(true);
    const rowB = wrapper.get('[data-testid="mod-row-b"]');
    expect(rowB.find('[data-testid="mod-row-missing-pill"]').exists()).toBe(false);
  });

  it("hides tags/hardDependents but still navigates when showActiveColumns is false", async () => {
    const wrapper = mountTable({
      mods: [mod({ tags: ["framework"] })],
      showActiveColumns: false,
    });
    await flushPromises();

    const row = wrapper.get('[data-testid="mod-row-fixture.mod"]');
    expect(row.text()).not.toContain("framework");

    await row.trigger("click");
    expect(wrapper.emitted("selectMod")).toEqual([["fixture.mod"]]);
  });

  it("only the first row is in the tab order before anything has been focused", async () => {
    const wrapper = mountTable({ mods: [mod({ modId: "a" }), mod({ modId: "b" })] });
    await flushPromises();

    expect(wrapper.get('[data-testid="mod-row-a"]').attributes("tabindex")).toBe("0");
    expect(wrapper.get('[data-testid="mod-row-b"]').attributes("tabindex")).toBe("-1");
  });

  it("ArrowDown moves roving focus to the next row and emits focusMod", async () => {
    const wrapper = mountTable({ mods: [mod({ modId: "a" }), mod({ modId: "b" })] });
    await flushPromises();

    await wrapper.get('[data-testid="mod-row-a"]').trigger("keydown", { key: "ArrowDown" });
    await flushPromises();

    expect(wrapper.emitted("focusMod")?.at(-1)).toEqual(["b"]);
    expect(wrapper.get('[data-testid="mod-row-a"]').attributes("tabindex")).toBe("-1");
    expect(wrapper.get('[data-testid="mod-row-b"]').attributes("tabindex")).toBe("0");
  });

  it("Enter selects the row immediately", async () => {
    const wrapper = mountTable({ mods: [mod({ modId: "a" })] });
    await flushPromises();

    await wrapper.get('[data-testid="mod-row-a"]').trigger("keydown", { key: "Enter" });

    expect(wrapper.emitted("selectMod")).toEqual([["a"]]);
  });

  it("Space toggles the row's own checkbox when selectable", async () => {
    const wrapper = mountTable({ mods: [mod({ modId: "a" })], selectable: true });
    await flushPromises();

    await wrapper.get('[data-testid="mod-row-a"]').trigger("keydown", { key: " " });

    expect(wrapper.emitted("update:selectedIds")?.at(-1)).toEqual([["a"]]);
  });
});
