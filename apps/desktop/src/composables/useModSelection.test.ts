import { describe, expect, it } from "vitest";
import { ref } from "vue";

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
});
