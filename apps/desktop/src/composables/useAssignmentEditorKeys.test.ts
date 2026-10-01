import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";

import { useAssignmentEditorKeys } from "@/composables/useAssignmentEditorKeys";
import type { CoverageRowDto } from "@/types/generated/CoverageRowDto";

function row(defName: string, overrides: Partial<CoverageRowDto> = {}): CoverageRowDto {
  return {
    target: { keyField: "raceNames", def: { defType: "ThingDef", defName } },
    owner: "target.races",
    matches: [],
    intent: "cover",
    winner: null,
    hasRow: false,
    ...overrides,
  };
}

function press(target: Element, key: string): void {
  target.dispatchEvent(new KeyboardEvent("keydown", { key, cancelable: true, bubbles: true }));
}

function mountHarness(
  rows: CoverageRowDto[],
  initialSelected: CoverageRowDto["target"] | null = null,
) {
  const selected = ref(initialSelected);
  const onSelect = vi.fn((selectedRow: CoverageRowDto) => {
    selected.value = selectedRow.target;
  });

  const Harness = defineComponent({
    setup() {
      const editorContainer = ref<HTMLElement | null>(null);
      const keys = useAssignmentEditorKeys({
        rows: () => rows,
        selectedTarget: () => selected.value,
        onSelect,
        editorContainer: () => editorContainer.value,
      });
      return { editorContainer, keys };
    },
    template: `
      <div>
        <input data-testid="text-input" />
        <div ref="editorContainer" data-testid="row-editor">
          <input data-testid="item-picker-search-example.PartDef" placeholder="Search items…" />
          <input data-testid="row-editor-scalar-note" />
        </div>
        <button type="button" data-testid="some-button">A button</button>
        <div role="radiogroup" data-testid="a-radiogroup">
          <button type="button" role="radio" data-testid="radio-button">An option</button>
        </div>
      </div>
    `,
  });

  const wrapper = mount(Harness, { attachTo: document.body });
  return { wrapper, onSelect, selected };
}

describe("useAssignmentEditorKeys", () => {
  it("j/k step through the coverage queue by the currently selected target, clamping at the bounds", () => {
    const rows = [row("Race0"), row("Race1"), row("Race2")];
    const { wrapper, onSelect, selected } = mountHarness(rows, rows[0]?.target ?? null);

    press(document.body, "j");
    expect(selected.value?.def.defName).toBe("Race1");

    press(document.body, "j");
    press(document.body, "j");
    expect(selected.value?.def.defName).toBe("Race2"); // clamps at the last row

    press(document.body, "k");
    expect(selected.value?.def.defName).toBe("Race1");
    // j, j, j (the third clamps at the last row but still re-selects it), k.
    expect(onSelect).toHaveBeenCalledTimes(4);
    wrapper.unmount();
  });

  it("j/k work from a clicked radio, while the arrows stay with the radio group", () => {
    const rows = [row("Race0"), row("Race1"), row("Race2")];
    const { wrapper, selected } = mountHarness(rows, rows[0]?.target ?? null);
    const radio = wrapper.get('[data-testid="radio-button"]').element;

    press(radio, "j");
    expect(selected.value?.def.defName).toBe("Race1");
    press(radio, "ArrowDown");
    expect(selected.value?.def.defName).toBe("Race1");
    wrapper.unmount();
  });

  it("j selects the first row when nothing is selected yet", () => {
    const rows = [row("Race0"), row("Race1")];
    const { wrapper, selected } = mountHarness(rows, null);

    press(document.body, "j");

    expect(selected.value?.def.defName).toBe("Race0");
    wrapper.unmount();
  });

  it("ArrowDown/ArrowUp are accepted alongside j/k", () => {
    const rows = [row("Race0"), row("Race1")];
    const { wrapper, selected } = mountHarness(rows, rows[0]?.target ?? null);

    press(document.body, "ArrowDown");
    expect(selected.value?.def.defName).toBe("Race1");
    press(document.body, "ArrowUp");
    expect(selected.value?.def.defName).toBe("Race0");
    wrapper.unmount();
  });

  it("Enter focuses the row editor's own first item-picker search box", () => {
    const rows = [row("Race0")];
    const { wrapper } = mountHarness(rows, rows[0]?.target ?? null);

    press(document.body, "Enter");

    expect(document.activeElement).toBe(
      wrapper.get('[data-testid="item-picker-search-example.PartDef"]').element,
    );
    wrapper.unmount();
  });

  it("Enter falls back to the first focusable control when the section has no item-slot picker", () => {
    const rows = [row("Race0")];
    const Harness = defineComponent({
      setup() {
        const editorContainer = ref<HTMLElement | null>(null);
        useAssignmentEditorKeys({
          rows: () => rows,
          selectedTarget: () => rows[0]?.target ?? null,
          onSelect: () => {},
          editorContainer: () => editorContainer.value,
        });
        return { editorContainer };
      },
      template: `
        <div>
          <div ref="editorContainer" data-testid="row-editor">
            <input data-testid="row-editor-scalar-note" />
          </div>
        </div>
      `,
    });
    const wrapper = mount(Harness, { attachTo: document.body });

    press(document.body, "Enter");

    expect(document.activeElement).toBe(
      wrapper.get('[data-testid="row-editor-scalar-note"]').element,
    );
    wrapper.unmount();
  });

  it("is inert while focus is in a text input, and never fires on a focused button (native activation wins)", () => {
    const rows = [row("Race0"), row("Race1")];
    const { wrapper, onSelect } = mountHarness(rows, rows[0]?.target ?? null);

    const input = wrapper.get('[data-testid="text-input"]').element;
    press(input, "j");
    expect(onSelect).not.toHaveBeenCalled();

    const button = wrapper.get('[data-testid="some-button"]').element;
    const enterEvent = new KeyboardEvent("keydown", {
      key: "Enter",
      cancelable: true,
      bubbles: true,
    });
    button.dispatchEvent(enterEvent);
    expect(enterEvent.defaultPrevented).toBe(false);
    wrapper.unmount();
  });
});
