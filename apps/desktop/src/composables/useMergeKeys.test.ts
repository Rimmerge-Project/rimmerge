import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";

import { useMergeKeys } from "@/composables/useMergeKeys";
import { useMergeStore } from "@/stores/merge";
import { asFieldPath } from "@/types/brands";
import type { MergeChoiceDto } from "@/types/generated/MergeChoiceDto";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import type { MergeOwnerDto } from "@/types/generated/MergeOwnerDto";
import type { MergeStateDto } from "@/types/generated/MergeStateDto";

function owner(modId: string, position: number): MergeOwnerDto {
  return { modId, name: modId, position };
}

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

const COMPLETE: MergeStateDto = { kind: "complete", opCount: 0 };

function press(target: Element, key: string): void {
  target.dispatchEvent(new KeyboardEvent("keydown", { key, cancelable: true, bubbles: true }));
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

function mountHarness(rows: MergeFieldDto[], owners: MergeOwnerDto[], winner = "a.mod") {
  const setChoice = vi.fn(
    (_path: unknown, _choice: MergeChoiceDto): Promise<MergeStateDto> => Promise.resolve(COMPLETE),
  );
  const clearChoice = vi.fn((_path: unknown): Promise<MergeStateDto> => Promise.resolve(COMPLETE));
  const onExit = vi.fn();
  const onToggleHelp = vi.fn();

  let composable!: ReturnType<typeof useMergeKeys>;
  const Harness = defineComponent({
    setup() {
      composable = useMergeKeys({
        rows: () => rows,
        owners: () => owners,
        winner: () => winner,
        choices: { setChoice, clearChoice },
        onExit,
        onToggleHelp,
      });
      return {};
    },
    template: `
      <div>
        <input data-testid="text-input" />
        <button type="button" data-testid="some-button">A button</button>
        <div role="radiogroup" data-testid="a-radiogroup">
          <button type="button" role="radio" data-testid="radio-button">An option</button>
        </div>
        <div role="dialog" data-testid="a-dialog">
          <button type="button" data-testid="dialog-button">Dialog button</button>
        </div>
      </div>
    `,
  });

  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(Harness, { attachTo: document.body, global: { plugins: [pinia] } });

  return {
    wrapper,
    setChoice,
    clearChoice,
    onExit,
    onToggleHelp,
    get keys() {
      return composable;
    },
  };
}

describe("useMergeKeys", () => {
  afterEach(() => {
    useMergeStore().reset();
  });

  it("j/k move the row cursor and clamp at the bounds", () => {
    const { wrapper } = mountHarness([field("a"), field("b"), field("c")], [owner("a.mod", 0)]);
    const store = useMergeStore();
    expect(store.rowCursor).toBe(0);

    press(document.body, "j");
    expect(store.rowCursor).toBe(1);
    press(document.body, "j");
    press(document.body, "j");
    expect(store.rowCursor).toBe(2);

    press(document.body, "k");
    expect(store.rowCursor).toBe(1);
    wrapper.unmount();
  });

  it("h/l move the column cursor across owners plus the trailing result column", () => {
    const { wrapper } = mountHarness([field("a")], [owner("a.mod", 0), owner("b.mod", 1)]);
    const store = useMergeStore();

    press(document.body, "l");
    expect(store.columnCursor).toBe(1);
    press(document.body, "l");
    expect(store.columnCursor).toBe(2); // the result column; clamps here
    press(document.body, "l");
    expect(store.columnCursor).toBe(2);

    press(document.body, "h");
    expect(store.columnCursor).toBe(1);
    wrapper.unmount();
  });

  it("c jumps to the next conflict row, skipping non-conflict rows", () => {
    const rows = [
      field("a", { class: "unchanged" }),
      field("b", { class: "conflict" }),
      field("c", { class: "unchanged" }),
      field("d", { class: "conflict" }),
    ];
    const { wrapper } = mountHarness(rows, [owner("a.mod", 0)]);
    const store = useMergeStore();

    press(document.body, "c");
    expect(store.rowCursor).toBe(1);
    press(document.body, "c");
    expect(store.rowCursor).toBe(3);
    // No further conflict after the last one.
    press(document.body, "c");
    expect(store.rowCursor).toBe(3);

    press(document.body, "C");
    expect(store.rowCursor).toBe(1);
    wrapper.unmount();
  });

  it("digit 2 chooses owner column 2 (0-based index 1) for the row at the cursor", async () => {
    const { wrapper, setChoice } = mountHarness(
      [field("statBases/MaxHitPoints")],
      [owner("a.mod", 0), owner("b.mod", 1)],
    );

    press(document.body, "2");
    await flush();

    expect(setChoice).toHaveBeenCalledWith(asFieldPath("statBases/MaxHitPoints"), {
      choice: "from",
      modId: "b.mod",
    });
    wrapper.unmount();
  });

  it("Enter chooses the column at the cursor; the trailing result column is a no-op", async () => {
    const { wrapper, setChoice } = mountHarness(
      [field("label")],
      [owner("a.mod", 0), owner("b.mod", 1)],
    );
    const store = useMergeStore();
    store.setColumnCursor(1);

    press(document.body, "Enter");
    await flush();
    expect(setChoice).toHaveBeenCalledWith(asFieldPath("label"), {
      choice: "from",
      modId: "b.mod",
    });

    setChoice.mockClear();
    store.setColumnCursor(2); // the result column
    press(document.body, "Enter");
    await flush();
    expect(setChoice).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("w chooses the selected-order winner", async () => {
    const { wrapper, setChoice } = mountHarness(
      [field("label")],
      [owner("a.mod", 0), owner("b.mod", 1)],
      "b.mod",
    );

    press(document.body, "w");
    await flush();

    expect(setChoice).toHaveBeenCalledWith(asFieldPath("label"), {
      choice: "from",
      modId: "b.mod",
    });
    wrapper.unmount();
  });

  it("d sets Drop, and pressing d again on a dropped row clears it", async () => {
    const rows = [field("label")];
    const { wrapper, setChoice, clearChoice } = mountHarness(rows, [owner("a.mod", 0)]);

    press(document.body, "d");
    await flush();
    expect(setChoice).toHaveBeenCalledWith(asFieldPath("label"), { choice: "drop" });

    rows[0] = field("label", { choice: { choice: "drop" } });
    press(document.body, "d");
    await flush();
    expect(clearChoice).toHaveBeenCalledWith(asFieldPath("label"));
    wrapper.unmount();
  });

  it("a reverts the row at the cursor to auto", async () => {
    const { wrapper, clearChoice } = mountHarness(
      [field("label", { choice: { choice: "from", modId: "a.mod" } })],
      [owner("a.mod", 0)],
    );

    press(document.body, "a");
    await flush();

    expect(clearChoice).toHaveBeenCalledWith(asFieldPath("label"));
    wrapper.unmount();
  });

  it("x toggles the auto section and X toggles unchanged rows", () => {
    const { wrapper } = mountHarness([field("label")], [owner("a.mod", 0)]);
    const store = useMergeStore();
    expect(store.collapsedAuto).toBe(true);
    expect(store.showUnchanged).toBe(false);

    press(document.body, "x");
    expect(store.collapsedAuto).toBe(false);

    press(document.body, "X");
    expect(store.showUnchanged).toBe(true);
    wrapper.unmount();
  });

  it("Esc/q call onExit, ? calls onToggleHelp", () => {
    const { wrapper, onExit, onToggleHelp } = mountHarness([field("label")], [owner("a.mod", 0)]);

    press(document.body, "Escape");
    expect(onExit).toHaveBeenCalledTimes(1);

    press(document.body, "q");
    expect(onExit).toHaveBeenCalledTimes(2);

    press(document.body, "?");
    expect(onToggleHelp).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it("e begins an edit, seeding the draft from the row's result; is inert while focus is in a text input", async () => {
    const { wrapper, setChoice, keys } = mountHarness(
      [field("label", { result: "current value" })],
      [owner("a.mod", 0)],
    );

    press(document.body, "e");
    expect(keys.editingDraft.value).toBe("current value");
    expect(keys.editingPath.value).toBe(asFieldPath("label"));

    const input = wrapper.get('[data-testid="text-input"]').element;
    press(input, "j"); // shortcuts must not fire while "editing" (focus in an input)
    await flush();
    expect(setChoice).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("Enter/Space on a focused button neither chooses the column nor prevents default (native activation wins)", async () => {
    const { wrapper, setChoice } = mountHarness(
      [field("label")],
      [owner("a.mod", 0), owner("b.mod", 1)],
    );
    const store = useMergeStore();
    store.setColumnCursor(1);
    const button = wrapper.get('[data-testid="some-button"]').element;

    const enterEvent = new KeyboardEvent("keydown", {
      key: "Enter",
      cancelable: true,
      bubbles: true,
    });
    button.dispatchEvent(enterEvent);
    await flush();
    expect(setChoice).not.toHaveBeenCalled();
    expect(enterEvent.defaultPrevented).toBe(false);

    const spaceEvent = new KeyboardEvent("keydown", { key: " ", cancelable: true, bubbles: true });
    button.dispatchEvent(spaceEvent);
    await flush();
    expect(setChoice).not.toHaveBeenCalled();
    expect(spaceEvent.defaultPrevented).toBe(false);
    wrapper.unmount();
  });

  it("is inert while focus is inside a role=dialog ancestor", async () => {
    const { wrapper, setChoice } = mountHarness(
      [field("a"), field("b")],
      [owner("a.mod", 0), owner("b.mod", 1)],
    );
    const store = useMergeStore();
    const dialogButton = wrapper.get('[data-testid="dialog-button"]').element;

    press(dialogButton, "j");
    expect(store.rowCursor).toBe(0);
    press(dialogButton, "Enter");
    await flush();
    expect(setChoice).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("j/k and h/l work from a clicked radio, while the arrows stay with the radio group", () => {
    const { wrapper } = mountHarness(
      [field("a"), field("b"), field("c")],
      [owner("a.mod", 0), owner("b.mod", 1)],
    );
    const store = useMergeStore();
    const radio = wrapper.get('[data-testid="radio-button"]').element;

    press(radio, "j");
    expect(store.rowCursor).toBe(1);
    press(radio, "ArrowDown");
    expect(store.rowCursor).toBe(1);
    press(radio, "l");
    expect(store.columnCursor).toBe(1);
    press(radio, "ArrowLeft");
    expect(store.columnCursor).toBe(1);
    wrapper.unmount();
  });

  it("commitEdit sends a Value choice and clears editingPath; cancelEdit only clears it", async () => {
    const { wrapper, setChoice, keys } = mountHarness([field("label")], [owner("a.mod", 0)]);

    press(document.body, "e");
    void keys.commitEdit("typed text");
    await flush();

    expect(setChoice).toHaveBeenCalledWith(asFieldPath("label"), {
      choice: "value",
      text: "typed text",
    });
    expect(keys.editingPath.value).toBe(null);

    press(document.body, "e");
    expect(keys.editingPath.value).toBe(asFieldPath("label"));
    keys.cancelEdit();
    expect(keys.editingPath.value).toBe(null);
    wrapper.unmount();
  });
});
