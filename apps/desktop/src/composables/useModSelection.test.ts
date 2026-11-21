import { afterEach, describe, expect, it, vi } from "vitest";
import { effectScope, nextTick, ref } from "vue";

import { modRow } from "@/components/mods/modListRow.test-support";
import { useModSelection } from "@/composables/useModSelection";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";

function setup(ids: string[], selected: string[] = []) {
  const rows = ref<ModListRowDto[]>(ids.map((modId) => modRow({ modId })));
  const selectedIds = ref<string[]>(selected);
  const selection = useModSelection({ selectedIds, rows });
  return { rows, selectedIds, selection };
}

describe("useModSelection", () => {
  it("selects every shown row", () => {
    const { selectedIds, selection } = setup(["a", "b", "c"]);

    selection.selectAllShown();

    expect(selectedIds.value).toEqual(["a", "b", "c"]);
    expect(selection.areAllShownSelected.value).toBe(true);
    expect(selection.areSomeShownSelected.value).toBe(false);
  });

  it("is tri-state: none, some, all of the shown rows", () => {
    const { selectedIds, selection } = setup(["a", "b"]);
    expect(selection.areAllShownSelected.value).toBe(false);
    expect(selection.areSomeShownSelected.value).toBe(false);

    selectedIds.value = ["a"];
    expect(selection.areSomeShownSelected.value).toBe(true);
    expect(selection.areAllShownSelected.value).toBe(false);

    selectedIds.value = ["a", "b"];
    expect(selection.areAllShownSelected.value).toBe(true);
  });

  it("never reports all selected for an empty list", () => {
    const { selection } = setup([]);

    expect(selection.areAllShownSelected.value).toBe(false);
    expect(selection.shownCount.value).toBe(0);
  });

  it("counts a selected row a filter hid, and keeps it selected", () => {
    const { rows, selectedIds, selection } = setup(["a", "b"], ["a", "b"]);

    rows.value = [modRow({ modId: "a" })];

    expect(selection.hiddenSelectedCount.value).toBe(1);
    expect(selection.shownSelectedCount.value).toBe(1);
    expect(selectedIds.value).toEqual(["a", "b"]);
  });

  it("select all keeps the hidden selection and adds only what is missing", () => {
    const { rows, selectedIds, selection } = setup(["a", "b"], ["b"]);
    rows.value = [modRow({ modId: "a" }), modRow({ modId: "c" })];

    selection.selectAllShown();

    expect(selectedIds.value).toEqual(["b", "a", "c"]);
  });

  it("deselecting the shown rows keeps the hidden ones", () => {
    const { rows, selectedIds, selection } = setup(["a", "b"], ["a", "b"]);
    rows.value = [modRow({ modId: "a" })];

    selection.deselectShown();

    expect(selectedIds.value).toEqual(["b"]);
  });

  it("clear removes every selected id, hidden ones included", () => {
    const { rows, selectedIds, selection } = setup(["a", "b"], ["a", "b"]);
    rows.value = [modRow({ modId: "a" })];

    selection.clear();

    expect(selectedIds.value).toEqual([]);
    expect(selection.hiddenSelectedCount.value).toBe(0);
  });

  it("prunes ids that are no longer in the list", () => {
    const { rows, selectedIds, selection } = setup(["a", "b"], ["a", "b"]);
    rows.value = [modRow({ modId: "a" })];

    selection.pruneToShown();

    expect(selectedIds.value).toEqual(["a"]);
  });

  it("counts every selected id, hidden ones included", () => {
    const { rows, selection } = setup(["a", "b"], ["a", "b"]);
    rows.value = [modRow({ modId: "a" })];

    expect(selection.selectedCount.value).toBe(2);
  });

  describe("anchor and ranges", () => {
    const sorted = (ids: string[]) => ids.toSorted();

    it("toggle selects the row and toggles it off again", () => {
      const { selectedIds, selection } = setup(["a", "b", "c"]);

      selection.toggle("b");
      expect(selectedIds.value).toEqual(["b"]);

      selection.toggle("b");
      expect(selectedIds.value).toEqual([]);
    });

    it("a range runs from the anchor to the row, in either direction", () => {
      const { selectedIds, selection } = setup(["a", "b", "c", "d", "e"]);
      selection.setAnchor("d");

      selection.selectRange("b");

      expect(sorted(selectedIds.value)).toEqual(["b", "c", "d"]);
    });

    it("setAnchor moves the anchor without changing the selection", () => {
      const { selectedIds, selection } = setup(["a", "b", "c", "d"]);
      selection.toggle("a");

      selection.setAnchor("c");
      selection.selectRange("d");

      expect(selectedIds.value).toEqual(["a", "c", "d"]);
    });

    it("a distant anchor: toggle a, focus moves on to c, Shift+Down selects exactly a, c, d", () => {
      const { selectedIds, selection } = setup(["a", "b", "c", "d", "e"]);
      selection.toggle("a");
      selection.setAnchor("b");
      selection.setAnchor("c");

      selection.extendSelection("c", "d");

      expect(sorted(selectedIds.value)).toEqual(["a", "c", "d"]);
    });

    it("keeps what was selected before the range, even when the range crosses it", () => {
      const { selectedIds, selection } = setup(["a", "b", "c", "d"], ["b"]);
      selection.setAnchor("a");

      selection.selectRange("c");

      expect(sorted(selectedIds.value)).toEqual(["a", "b", "c"]);
    });

    it("moving back toward the anchor shrinks the range but keeps the earlier selection", () => {
      const { selectedIds, selection } = setup(["a", "b", "c", "d", "e"], ["e"]);
      selection.setAnchor("a");

      selection.extendSelection("a", "b");
      selection.extendSelection("a", "c");
      selection.extendSelection("a", "d");
      selection.extendSelection("a", "c");

      expect(sorted(selectedIds.value)).toEqual(["a", "b", "c", "e"]);
      expect(selection.selectedCount.value).toBe(4);
    });

    it("shrinking with Shift+Up after three Shift+Downs leaves exactly 3 selected", () => {
      const { selectedIds, selection } = setup(["a", "b", "c", "d", "e"]);
      selection.setAnchor("a");

      selection.extendSelection("a", "b");
      selection.extendSelection("b", "c");
      selection.extendSelection("c", "d");
      selection.extendSelection("d", "c");

      expect(sorted(selectedIds.value)).toEqual(["a", "b", "c"]);
      expect(selection.selectedCount.value).toBe(3);
    });

    it("without an anchor a range selects just that row", () => {
      const { selectedIds, selection } = setup(["a", "b", "c"]);

      selection.selectRange("b");

      expect(selectedIds.value).toEqual(["b"]);
    });

    it("a range covers only shown rows: a hidden selected id stays, a hidden row is never added", () => {
      const { rows, selectedIds, selection } = setup(["a", "b", "c", "d"], ["b"]);
      selection.setAnchor("a");
      rows.value = [modRow({ modId: "a" }), modRow({ modId: "c" }), modRow({ modId: "d" })];

      selection.selectRange("d");

      expect(sorted(selectedIds.value)).toEqual(["a", "b", "c", "d"]);
      expect(selection.hiddenSelectedCount.value).toBe(1);
    });

    it("does not add a hidden unselected row inside the range", () => {
      const { rows, selectedIds, selection } = setup(["a", "b", "c"]);
      selection.setAnchor("a");
      rows.value = [modRow({ modId: "a" }), modRow({ modId: "c" })];

      selection.selectRange("c");

      expect(sorted(selectedIds.value)).toEqual(["a", "c"]);
    });

    it("a row that is not shown is ignored", () => {
      const { selectedIds, selection } = setup(["a", "b"]);

      selection.selectRange("zzz");

      expect(selectedIds.value).toEqual([]);
    });

    it("an anchor whose row left the list counts as no anchor", () => {
      const { rows, selectedIds, selection } = setup(["a", "b", "c"]);
      selection.setAnchor("a");
      rows.value = [modRow({ modId: "b" }), modRow({ modId: "c" })];

      selection.selectRange("c");

      expect(selectedIds.value).toEqual(["c"]);
    });

    it("extending with no anchor selects the row focus left and the one it reached", () => {
      const { selectedIds, selection } = setup(["a", "b", "c"]);

      selection.extendSelection("a", "b");

      expect(sorted(selectedIds.value)).toEqual(["a", "b"]);
    });

    it("a filter change ends the range: the hidden selected rows stay selected", async () => {
      const { rows, selectedIds, selection } = setup(["a", "b", "c", "d", "e", "f"]);
      selection.setAnchor("a");
      selection.selectRange("d");
      rows.value = [modRow({ modId: "a" }), modRow({ modId: "e" }), modRow({ modId: "f" })];
      await nextTick();
      expect(selection.hiddenSelectedCount.value).toBe(3);

      selection.selectRange("e");

      expect(sorted(selectedIds.value)).toEqual(["a", "b", "c", "d", "e"]);
      expect(selection.hiddenSelectedCount.value).toBe(3);
    });

    it("a range right after the rows changed, with no tick between, still keeps the earlier selection", () => {
      const { rows, selectedIds, selection } = setup(["a", "b", "c", "d", "e", "f"]);
      selection.setAnchor("a");
      selection.selectRange("d");
      rows.value = [modRow({ modId: "a" }), modRow({ modId: "e" }), modRow({ modId: "f" })];

      selection.selectRange("e");

      expect(sorted(selectedIds.value)).toEqual(["a", "b", "c", "d", "e"]);
    });

    it("when the anchor row is filtered out, the clicked row is added to the selection", async () => {
      const { rows, selectedIds, selection } = setup(["a", "b", "c", "d", "e", "f"]);
      selection.setAnchor("a");
      selection.selectRange("c");
      rows.value = [modRow({ modId: "d" }), modRow({ modId: "e" }), modRow({ modId: "f" })];
      await nextTick();

      selection.selectRange("f");

      expect(sorted(selectedIds.value)).toEqual(["a", "b", "c", "f"]);
    });

    describe("count repeat announcement", () => {
      type Selection = ReturnType<typeof setup>["selection"];

      afterEach(() => {
        vi.useRealTimers();
      });

      it("empties the count for a moment when a range leaves the count unchanged, then restores it", () => {
        vi.useFakeTimers();
        const { selection } = setup(["a", "b", "c"]);
        selection.setAnchor("a");
        selection.selectRange("b");
        expect(selection.isCountPaused.value).toBe(false);

        selection.selectRange("b");
        expect(selection.isCountPaused.value).toBe(true);

        vi.advanceTimersByTime(100);
        expect(selection.isCountPaused.value).toBe(false);
      });

      it("does not pause when the count changed, since the text itself changes", () => {
        vi.useFakeTimers();
        const { selection } = setup(["a", "b", "c"]);
        selection.setAnchor("a");

        selection.selectRange("b");
        selection.selectRange("c");

        expect(selection.isCountPaused.value).toBe(false);
      });

      it("any other gesture cancels the pause at once", () => {
        const gestures: Record<string, (selection: Selection) => void> = {
          toggle: (selection) => selection.toggle("c"),
          setAnchor: (selection) => selection.setAnchor("c"),
          clear: (selection) => selection.clear(),
          selectAllShown: (selection) => selection.selectAllShown(),
          deselectShown: (selection) => selection.deselectShown(),
        };
        for (const [name, gesture] of Object.entries(gestures)) {
          vi.useFakeTimers();
          const { selection } = setup(["a", "b", "c"]);
          selection.setAnchor("a");
          selection.selectRange("b");
          selection.selectRange("b");
          expect(selection.isCountPaused.value, name).toBe(true);

          gesture(selection);

          expect(selection.isCountPaused.value, name).toBe(false);
          vi.useRealTimers();
        }
      });

      it("a repeat within the pause keeps one timer, and disposing the scope leaves none", () => {
        vi.useFakeTimers();
        const scope = effectScope();
        const selection = scope.run(() => setup(["a", "b", "c"]).selection) as Selection;
        selection.setAnchor("a");
        selection.selectRange("b");
        selection.selectRange("b");
        vi.advanceTimersByTime(50);
        selection.selectRange("b");
        vi.advanceTimersByTime(50);

        expect(selection.isCountPaused.value).toBe(true);
        expect(vi.getTimerCount()).toBe(1);

        scope.stop();

        expect(vi.getTimerCount()).toBe(0);
      });

      it("a change of the shown rows cancels the pause", async () => {
        vi.useFakeTimers();
        const { rows, selection } = setup(["a", "b", "c"]);
        selection.setAnchor("a");
        selection.selectRange("b");
        selection.selectRange("b");

        rows.value = [modRow({ modId: "a" })];
        await nextTick();

        expect(selection.isCountPaused.value).toBe(false);
      });

      it("the caller replacing the selection cancels the pause", async () => {
        vi.useFakeTimers();
        const { selectedIds, selection } = setup(["a", "b", "c"]);
        selection.setAnchor("a");
        selection.selectRange("b");
        selection.selectRange("b");

        selectedIds.value = [];
        await nextTick();

        expect(selection.isCountPaused.value).toBe(false);
      });
    });

    it("starts a fresh range base after an outside change", async () => {
      const { selectedIds, selection } = setup(["a", "b", "c", "d"]);
      selection.setAnchor("a");
      selection.selectRange("c");
      selectedIds.value = ["d"];
      await nextTick();

      selection.selectRange("b");

      expect(sorted(selectedIds.value)).toEqual(["a", "b", "d"]);
    });
  });
});

describe("useModSelection over a range line", () => {
  /** Two folders: `a b` then a header then `b c`, so `b` has a copy in each. */
  const LINE = [
    { key: "f:1", modId: null },
    { key: "m:1:a", modId: "a" },
    { key: "m:1:b", modId: "b" },
    { key: "f:2", modId: null },
    { key: "m:2:b", modId: "b" },
    { key: "m:2:c", modId: "c" },
  ];

  function setupLine(selected: string[] = []) {
    const rows = ref<ModListRowDto[]>(["a", "b", "c"].map((modId) => modRow({ modId })));
    const selectedIds = ref<string[]>(selected);
    const selection = useModSelection({ selectedIds, rows, rangeLine: () => LINE });
    return { rows, selectedIds, selection };
  }

  it("counts mods, not rows: three mods are shown though the line has six rows", () => {
    const { selection } = setupLine();

    expect(selection.shownCount.value).toBe(3);
  });

  it("selects the mods between two rows once each, and a header adds nothing", () => {
    const { selectedIds, selection } = setupLine();
    selection.setAnchor("m:1:a");

    selection.selectRange("m:2:c");

    expect(selectedIds.value).toEqual(["a", "b", "c"]);
  });

  it("anchors by row key: the same mod's other row is a different anchor", () => {
    const { selectedIds, selection } = setupLine();
    selection.setAnchor("m:2:b");

    selection.selectRange("m:2:c");

    expect(selectedIds.value).toEqual(["b", "c"]);
  });

  it("runs a range from a header, which selects nothing itself", () => {
    const { selectedIds, selection } = setupLine();
    selection.setAnchor("f:2");

    selection.selectRange("m:2:c");

    expect(selectedIds.value).toEqual(["b", "c"]);
  });

  it("keeps what was selected before the range and adds nothing twice", () => {
    const { selectedIds, selection } = setupLine(["c"]);
    selection.setAnchor("m:1:a");

    selection.selectRange("m:1:b");

    expect(selectedIds.value).toEqual(["c", "a", "b"]);
  });

  it("shrinks the range when the target moves back toward the anchor", () => {
    const { selectedIds, selection } = setupLine();
    selection.setAnchor("m:1:a");
    selection.selectRange("m:2:c");

    selection.selectRange("m:1:b");

    expect(selectedIds.value).toEqual(["a", "b"]);
  });

  it("toggles by mod id but anchors on the row it was done on", () => {
    const { selectedIds, selection } = setupLine();

    selection.toggle("b", "m:2:b");
    selection.selectRange("m:2:c");

    expect(selectedIds.value).toEqual(["b", "c"]);
  });

  it("starts from the focused row when Shift+Arrow finds no anchor", () => {
    const { selectedIds, selection } = setupLine();

    selection.extendSelection("f:2", "m:2:b");

    expect(selectedIds.value).toEqual(["b"]);
  });

  it("selects every mod of the answer for Select all, whatever the line shows", () => {
    const { selectedIds, selection } = setupLine();

    selection.selectAllShown();

    expect(selectedIds.value).toEqual(["a", "b", "c"]);
  });

  it("ignores a row key that is not in the line", () => {
    const { selectedIds, selection } = setupLine();
    selection.setAnchor("m:1:a");

    selection.selectRange("m:9:zzz");

    expect(selectedIds.value).toEqual([]);
  });

  it("ends the range when the line changes, like a refetch of the rows", async () => {
    const line = ref(LINE);
    const rows = ref<ModListRowDto[]>(["a", "b", "c"].map((modId) => modRow({ modId })));
    const selectedIds = ref<string[]>([]);
    const selection = useModSelection({ selectedIds, rows, rangeLine: line });
    selection.setAnchor("m:1:a");
    selection.selectRange("m:1:b");

    line.value = LINE.filter((row) => row.key !== "m:1:b");
    await nextTick();
    selection.selectRange("m:2:c");

    // The new range starts again from what was selected, and `a` is still the anchor.
    expect(selectedIds.value).toEqual(["a", "b", "c"]);
  });
});
