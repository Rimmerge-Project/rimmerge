import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import ItemPicker from "@/components/assignments/ItemPicker.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ListItemDto } from "@/types/generated/ListItemDto";
import type { ListItemsPageDto } from "@/types/generated/ListItemsPageDto";

function ownItem(defName: string): ListItemDto {
  return {
    def: { defType: "example.PartDef", defName },
    owner: "mypatch.parts",
    hint: null,
    own: true,
  };
}

function activeItem(defName: string): ListItemDto {
  return {
    def: { defType: "example.PartDef", defName },
    owner: "fixture.parts",
    hint: null,
    own: false,
  };
}

function mountPicker(props: InstanceType<typeof ItemPicker>["$props"], page: ListItemsPageDto) {
  installMockIpc({
    list_mod_names: {},
    list_assignment_items: () => page,
  });
  return mount(ItemPicker, {
    props,
    global: { plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]] },
  });
}

describe("ItemPicker", () => {
  afterEach(() => {
    clearMocks();
  });

  it("groups the project's own rows first under a 'This project' heading, ahead of the active list's matches", async () => {
    const wrapper = mountPicker(
      {
        defType: "example.PartDef",
        selected: [],
        cardinality: "list",
        assignmentId: "abc123def456",
      },
      { total: 2, items: [ownItem("mypatch_parts_Part0"), activeItem("Part0")] },
    );
    await flushPromises();

    const heading = wrapper.get('[data-testid="item-picker-own-heading-example.PartDef"]');
    expect(heading.text()).toBe("This project");

    const listText = wrapper.get('[data-testid="item-picker-list-example.PartDef"]').text();
    // The own row's own name appears before the active list's row —
    // `ListItemDto.own` decides the group, never `owner`.
    expect(listText.indexOf("mypatch_parts_Part0")).toBeLessThan(listText.indexOf("Part0"));
  });

  it("shows no 'This project' heading when nothing came back own", async () => {
    const wrapper = mountPicker(
      { defType: "example.PartDef", selected: [], cardinality: "list", assignmentId: null },
      { total: 1, items: [activeItem("Part0")] },
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="item-picker-own-heading-example.PartDef"]').exists()).toBe(
      false,
    );
  });

  it("passes assignmentId through to list_assignment_items", async () => {
    const calls: unknown[] = [];
    installMockIpc({
      list_mod_names: {},
      list_assignment_items: (payload: unknown) => {
        calls.push(payload);
        return { total: 0, items: [] } satisfies ListItemsPageDto;
      },
    });
    const wrapper = mount(ItemPicker, {
      props: {
        defType: "example.PartDef",
        selected: [],
        cardinality: "list",
        assignmentId: "abc123def456",
      },
      global: { plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]] },
    });
    await flushPromises();

    expect(calls).toHaveLength(1);
    expect(calls[0]).toMatchObject({ request: { assignmentId: "abc123def456" } });
    wrapper.unmount();
  });

  it("renders a single-select (radio) list for a scalar cardinality slot, never letting a second name join the selection", async () => {
    const wrapper = mountPicker(
      {
        defType: "example.PartDef",
        selected: ["Part0"],
        cardinality: "scalar",
        assignmentId: null,
      },
      { total: 2, items: [activeItem("Part0"), activeItem("Part1")] },
    );
    await flushPromises();

    const part0 = wrapper.get('[data-testid="item-picker-checkbox-example.PartDef-Part0"]');
    const part1 = wrapper.get('[data-testid="item-picker-checkbox-example.PartDef-Part1"]');
    expect((part0.element as HTMLInputElement).type).toBe("radio");
    expect((part0.element as HTMLInputElement).checked).toBe(true);
    expect((part1.element as HTMLInputElement).checked).toBe(false);

    await part1.setValue(true);

    // Picking a second item replaces the selection rather than adding to
    // it: only one `update:selected` name ever comes back, never both.
    const emitted = wrapper.emitted("update:selected");
    expect(emitted).toBeTruthy();
    expect(emitted?.at(-1)).toEqual([["Part1"]]);
    for (const [names] of emitted ?? []) {
      expect(names).toHaveLength(1);
    }
  });

  it("offers a checkbox list (not radios) for a list cardinality slot", async () => {
    const wrapper = mountPicker(
      { defType: "example.PartDef", selected: [], cardinality: "list", assignmentId: null },
      { total: 1, items: [activeItem("Part0")] },
    );
    await flushPromises();

    const checkbox = wrapper.get('[data-testid="item-picker-checkbox-example.PartDef-Part0"]');
    expect((checkbox.element as HTMLInputElement).type).toBe("checkbox");
  });

  it("shows a decorative thumbnail on every active def row and none on the project's own new rows", async () => {
    const wrapper = mountPicker(
      {
        defType: "example.PartDef",
        selected: [],
        cardinality: "list",
        assignmentId: "abc123def456",
      },
      {
        total: 3,
        items: [ownItem("mypatch_parts_Part0"), activeItem("Part0"), activeItem("Part1")],
      },
    );
    await flushPromises();

    const own = wrapper.get(
      '[data-testid="item-picker-checkbox-example.PartDef-mypatch_parts_Part0"]',
    );
    expect(own.element.closest("label")?.querySelector('[data-testid="def-thumbnail"]')).toBeNull();
    for (const name of ["Part0", "Part1"]) {
      const row = wrapper
        .get(`[data-testid="item-picker-checkbox-example.PartDef-${name}"]`)
        .element.closest("label");
      const thumbnail = row?.querySelector('[data-testid="def-thumbnail"]');
      expect(thumbnail?.getAttribute("aria-hidden")).toBe("true");
      expect(thumbnail?.classList.contains("h-6")).toBe(true);
    }
    expect(wrapper.findAll('[data-testid="def-thumbnail"]')).toHaveLength(2);
  });
});
