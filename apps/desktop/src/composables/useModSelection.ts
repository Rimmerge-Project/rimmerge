import { computed, type MaybeRefOrGetter, type Ref, toValue } from "vue";

import type { ModListRowDto } from "@/types/generated/ModListRowDto";

/**
 * One Mods list's selection over the rows it is showing. "Shown" means the
 * rows of the current list response, which the backend already filtered by
 * the search/source/tag query, so "select all" never re-implements a filter
 * here. A selected id a later filter hides stays selected and is counted in
 * `hiddenSelectedCount` so the list can say so; {@link clear} removes all of
 * them. Nothing here is saved anywhere.
 */
export function useModSelection({
  selectedIds,
  rows,
}: {
  /** The selected ids, owned by the caller (the activate/deactivate flows read them). */
  selectedIds: Ref<string[]>;
  /** The rows the list currently shows. */
  rows: MaybeRefOrGetter<readonly ModListRowDto[]>;
}) {
  const shownIds = computed(() => toValue(rows).map((row) => row.modId));
  const selectedSet = computed(() => new Set(selectedIds.value));
  const shownSelectedCount = computed(
    () => shownIds.value.filter((id) => selectedSet.value.has(id)).length,
  );
  const hiddenSelectedCount = computed(() => selectedIds.value.length - shownSelectedCount.value);
  const areAllShownSelected = computed(
    () => shownIds.value.length > 0 && shownSelectedCount.value === shownIds.value.length,
  );
  const areSomeShownSelected = computed(
    () => shownSelectedCount.value > 0 && !areAllShownSelected.value,
  );

  /** Adds every shown row to the selection, keeping hidden selected ids. */
  function selectAllShown(): void {
    const additions = shownIds.value.filter((id) => !selectedSet.value.has(id));
    selectedIds.value = [...selectedIds.value, ...additions];
  }

  /** Removes the shown rows from the selection, keeping hidden selected ids. */
  function deselectShown(): void {
    const shown = new Set(shownIds.value);
    selectedIds.value = selectedIds.value.filter((id) => !shown.has(id));
  }

  /** Removes every selected id, hidden ones included. */
  function clear(): void {
    selectedIds.value = [];
  }

  /**
   * Drops selected ids that are no longer in the list. Only meaningful for a
   * response to an unfiltered query, where "not in the list" means gone
   * rather than hidden.
   */
  function pruneToShown(): void {
    const shown = new Set(shownIds.value);
    if (selectedIds.value.every((id) => shown.has(id))) {
      return;
    }
    selectedIds.value = selectedIds.value.filter((id) => shown.has(id));
  }

  return {
    shownCount: computed(() => shownIds.value.length),
    shownSelectedCount,
    hiddenSelectedCount,
    areAllShownSelected,
    areSomeShownSelected,
    selectAllShown,
    deselectShown,
    clear,
    pruneToShown,
  };
}

/** One list's selection, as {@link useModSelection} returns it. */
export type ModSelection = ReturnType<typeof useModSelection>;
