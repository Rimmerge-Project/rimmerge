import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import MergeFieldTable from "@/components/merge/MergeFieldTable.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import type { MergeOwnerDto } from "@/types/generated/MergeOwnerDto";

const owners: MergeOwnerDto[] = [{ modId: "a.mod", name: "Mod A", position: 0 }];

function field(path: string, overrides: Partial<MergeFieldDto> = {}): MergeFieldDto {
  return {
    path,
    depth: 0,
    isListItem: false,
    entry: "leaf",
    container: null,
    class: "conflict",
    changedBy: [],
    confidence: 0,
    base: "base",
    candidates: {},
    result: null,
    choice: null,
    preselected: null,
    ...overrides,
  };
}

/**
 * Mounted attached to the document, with two ticks awaited: `@tanstack/vue-
 * virtual`'s first measurement pass runs after mount (an internal effect,
 * not the initial render), so `getVirtualItems()` is empty until it
 * settles — the same reason `InboxPage.test.ts` awaits a tick after
 * mounting `FindingList`. `list_mod_names` needs *some* registered
 * fixture too: `useModLabel` (via the owner header) always queries it,
 * and an unregistered command's rejection reaches Vitest as an
 * unhandled rejection that fails the whole run, not just this file (see
 * the identical note in `InboxPage.test.ts`).
 */
async function mountTable(props: Partial<InstanceType<typeof MergeFieldTable>["$props"]> = {}) {
  installMockIpc({ list_mod_names: {} });
  const wrapper = mount(MergeFieldTable, {
    attachTo: document.body,
    global: { plugins: [createPinia(), PiniaColada] },
    props: {
      conflictFields: [field("conflictField", { class: "conflict" })],
      autoFields: [field("autoField", { class: "oneSided" })],
      unchangedFields: [field("unchangedField", { class: "unchanged" })],
      conflictTotal: 1,
      autoTotal: 1,
      unchangedTotal: 1,
      owners,
      collapsedAuto: true,
      showUnchanged: false,
      rowCursor: 0,
      columnCursor: 0,
      editingPath: null,
      editingDraft: "",
      ...props,
    },
  });
  await wrapper.vm.$nextTick();
  await wrapper.vm.$nextTick();
  return wrapper;
}

describe("MergeFieldTable", () => {
  // `@tanstack/vue-virtual` measures its scroll container via
  // `offsetWidth`/`offsetHeight`, which happy-dom always reports as `0` —
  // stubbed so the virtualizer renders every row instead of an empty
  // visible range (same fix `InboxPage.test.ts` uses for `FindingList`).
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      value: 800,
    });
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, value: 600 });
  });

  afterEach(() => {
    clearMocks();
  });

  it("always shows conflict rows and collapses the auto section by default, hiding unchanged rows", async () => {
    const wrapper = await mountTable();

    expect(wrapper.find('[data-testid="merge-field-row"]').exists()).toBe(true);
    expect(wrapper.text()).toContain("conflictField");
    expect(wrapper.text()).not.toContain("autoField");
    expect(wrapper.text()).not.toContain("unchangedField");
    expect(wrapper.get('[data-testid="merge-section-auto"]').text()).toContain("Auto-resolved (1)");
    wrapper.unmount();
  });

  it("the auto section header count reflects the server total, not the currently-loaded page size", async () => {
    const wrapper = await mountTable({
      autoFields: [field("autoField", { class: "oneSided" })],
      autoTotal: 250,
    });

    expect(wrapper.get('[data-testid="merge-section-auto"]').text()).toContain(
      "Auto-resolved (250)",
    );
    wrapper.unmount();
  });

  it("shows auto rows once collapsedAuto is false", async () => {
    const wrapper = await mountTable({ collapsedAuto: false });
    expect(wrapper.text()).toContain("autoField");
    expect(wrapper.text()).not.toContain("unchangedField");
    wrapper.unmount();
  });

  it("shows unchanged rows once showUnchanged is true", async () => {
    const wrapper = await mountTable({ showUnchanged: true });
    expect(wrapper.text()).toContain("unchangedField");
    wrapper.unmount();
  });

  it("clicking the auto section header emits toggleAuto", async () => {
    const wrapper = await mountTable();
    await wrapper.get('[data-testid="merge-section-auto"]').trigger("click");
    expect(wrapper.emitted("toggleAuto")).toHaveLength(1);
    wrapper.unmount();
  });

  it("clicking the unchanged section header emits toggleUnchanged", async () => {
    const wrapper = await mountTable();
    await wrapper.get('[data-testid="merge-section-unchanged"]').trigger("click");
    expect(wrapper.emitted("toggleUnchanged")).toHaveLength(1);
    wrapper.unmount();
  });

  it("the conflicts header is not clickable (it is never collapsible)", async () => {
    const wrapper = await mountTable();
    await wrapper.get('[data-testid="merge-section-conflicts"]').trigger("click");
    expect(wrapper.emitted("toggleAuto")).toBeUndefined();
    expect(wrapper.emitted("toggleUnchanged")).toBeUndefined();
    wrapper.unmount();
  });

  it("forwards choose/drop/revert events tagged with the row's own field path", async () => {
    const wrapper = await mountTable();

    await wrapper.get('[data-testid="merge-radio-conflictField-a.mod"]').setValue(true);
    expect(wrapper.emitted("choose")).toEqual([
      ["conflictField", { choice: "from", modId: "a.mod" }],
    ]);

    await wrapper.get('[data-testid="merge-drop-button"]').trigger("click");
    expect(wrapper.emitted("drop")).toEqual([["conflictField"]]);
    wrapper.unmount();
  });

  // -- keyed-map container grouping --

  function mapEntry(path: string, overrides: Partial<MergeFieldDto> = {}): MergeFieldDto {
    return field(path, {
      entry: "mapEntry",
      container: "wildAnimals",
      class: "oneSided",
      changedBy: ["a.mod"],
      ...overrides,
    });
  }

  it("groups consecutive map-entry rows sharing a container under one header, and renders no header for a lone entry", async () => {
    const wrapper = await mountTable({
      conflictFields: [],
      conflictTotal: 0,
      autoFields: [mapEntry("wildAnimals/Allosaurus"), mapEntry("wildAnimals/Mammoth")],
      autoTotal: 2,
      unchangedFields: [field("loneUnchanged", { class: "unchanged" })],
      unchangedTotal: 1,
      collapsedAuto: false,
      showUnchanged: true,
    });

    const header = wrapper.get('[data-testid="merge-container-auto-wildAnimals"]');
    expect(header.text()).toContain("wildAnimals — 2 entries");
    expect(header.text()).not.toContain("conflict");
    expect(wrapper.text()).toContain("wildAnimals/Allosaurus");
    expect(wrapper.text()).toContain("wildAnimals/Mammoth");

    // `loneUnchanged` carries no `container` at all, so it never groups —
    // no header rendered for it.
    expect(wrapper.findAll('[data-testid^="merge-container-"]')).toHaveLength(1);
    wrapper.unmount();
  });

  it("a container header collapsed by default (more than 12 entries, no conflict) hides its member rows until toggled", async () => {
    const manyEntries = Array.from({ length: 13 }, (_, i) => mapEntry(`wildAnimals/Key${i}`));
    const wrapper = await mountTable({
      conflictFields: [],
      conflictTotal: 0,
      autoFields: manyEntries,
      autoTotal: 13,
      collapsedAuto: false,
    });

    const header = wrapper.get('[data-testid="merge-container-auto-wildAnimals"]');
    expect(header.text()).toContain("wildAnimals — 13 entries");
    expect(header.text()).toContain("▸");
    expect(wrapper.text()).not.toContain("wildAnimals/Key0");

    await header.trigger("click");
    expect(wrapper.emitted("toggleContainer")).toEqual([["wildAnimals", false]]);
    wrapper.unmount();
  });

  it("an explicit containerOverrides entry expands a container that would otherwise default-collapse", async () => {
    const manyEntries = Array.from({ length: 13 }, (_, i) => mapEntry(`wildAnimals/Key${i}`));
    const wrapper = await mountTable({
      conflictFields: [],
      conflictTotal: 0,
      autoFields: manyEntries,
      autoTotal: 13,
      collapsedAuto: false,
      containerOverrides: { wildAnimals: false },
    });

    expect(wrapper.text()).toContain("wildAnimals/Key0");
    const header = wrapper.get('[data-testid="merge-container-auto-wildAnimals"]');
    expect(header.text()).toContain("▾");
    wrapper.unmount();
  });

  it("the conflicts section's own container header is never collapsible and offers a one-click Accept suggested", async () => {
    const wrapper = await mountTable({
      conflictFields: [
        mapEntry("wildAnimals/Raptor", { class: "conflict", preselected: "c.mod" }),
        mapEntry("wildAnimals/Cobra", { class: "conflict", preselected: "b.mod" }),
      ],
      conflictTotal: 2,
    });

    const header = wrapper.get('[data-testid="merge-container-conflicts-wildAnimals"]');
    expect(header.text()).toContain("wildAnimals — 2 entries, 2 conflicts");
    // No collapse arrow — the conflicts section's own header never collapses.
    expect(header.text()).not.toContain("▸");
    expect(header.text()).not.toContain("▾");
    await header.trigger("click");
    expect(wrapper.emitted("toggleContainer")).toBeUndefined();

    const acceptButton = wrapper.get('[data-testid="merge-accept-union-wildAnimals"]');
    expect(acceptButton.text()).toBe("Accept suggested (2)");
    await acceptButton.trigger("click");

    expect(wrapper.emitted("choose")).toEqual([
      ["wildAnimals/Raptor", { choice: "from", modId: "c.mod" }],
      ["wildAnimals/Cobra", { choice: "from", modId: "b.mod" }],
    ]);
    wrapper.unmount();
  });
});
