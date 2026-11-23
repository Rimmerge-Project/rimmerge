import {
  computed,
  getCurrentScope,
  type MaybeRefOrGetter,
  onScopeDispose,
  type Ref,
  ref,
  toValue,
  watch,
} from "vue";

import type { ModListRowDto } from "@/types/generated/ModListRowDto";

/**
 * One row of the line a Shift range runs over: the key a table names it by, and the mod it holds
 * (`null` for a row that selects nothing, such as a Groups view folder header). In the List view the
 * key is the mod id; in the Groups view it is the entry key, since one mod can appear in many folders.
 */
export interface RangeRow {
  readonly key: string;
  readonly modId: string | null;
}

/** How long the count region stays empty to make an unchanged count audible again. */
const REPEAT_PAUSE_MS = 100;

function haveSameIds(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((id, index) => id === right[index]);
}

/**
 * One Mods list's selection over the rows it is showing. "Shown" means the
 * rows of the current list response, which the backend already filtered by
 * the search/source/tag query, so "select all" never re-implements a filter
 * here. A selected id a later filter hides stays selected and is counted in
 * `hiddenSelectedCount` so the list can say so; {@link clear} removes all of
 * them. Nothing here is saved anywhere.
 *
 * The selection also keeps an anchor, the row a Shift range starts from. It
 * follows focus: a toggle, a plain click and plain keyboard movement all move
 * it ({@link setAnchor}) without changing what is selected, and a Shift range
 * leaves it where it is. A range is always anchor..target over the shown rows
 * (so moving back toward the anchor shrinks it) on top of whatever was
 * selected before the range began; hidden selected ids stay and a hidden row
 * is never added. An anchor whose row is no longer shown counts as no anchor. Any refetch of the
 * rows yields a new range line and ends an active range: the selection is kept and the next
 * Shift step starts a new range.
 *
 * Selection is per mod id, and so are the counts ({@link rows} holds each mod once). A Shift range
 * runs over {@link rangeLine} instead, the rows as they stand on screen: a mod that appears in
 * several folders is several rows there, and a range is what lies between two of them, on top of
 * the selection with duplicates removed. Without a line it is the rows themselves, keyed by mod id.
 */
export function useModSelection({
  selectedIds,
  rows,
  rangeLine,
}: {
  /** The selected ids, owned by the caller (the activate/deactivate flows read them). */
  selectedIds: Ref<string[]>;
  /** The rows the list currently shows, each mod once. */
  rows: MaybeRefOrGetter<readonly ModListRowDto[]>;
  /** The rows a Shift range runs over, in screen order; absent or `undefined` is {@link rows}. */
  rangeLine?: MaybeRefOrGetter<readonly RangeRow[] | undefined>;
}) {
  const shownIds = computed(() => toValue(rows).map((row) => row.modId));
  const line = computed<readonly RangeRow[]>(
    () =>
      (rangeLine === undefined ? undefined : toValue(rangeLine)) ??
      shownIds.value.map((modId) => ({ key: modId, modId })),
  );
  const lineKeys = computed(() => line.value.map((row) => row.key));
  const anchorId = ref<string | null>(null);
  /** The selection as it was when the current Shift range began; `null` while no range is active. */
  let rangeBase: string[] | null = null;
  /** The last selection this composable wrote, to tell its own writes from the caller's. */
  let committed: readonly string[] = selectedIds.value;
  /** The line the current range was computed over; a different array means the rows changed. */
  let rangeShown: readonly RangeRow[] | null = null;
  /**
   * Whether the count's live region should read as empty for a moment. A Shift range that leaves
   * the selected count unchanged would otherwise change nothing the region could announce.
   */
  const isCountPaused = ref(false);
  let pauseTimer: ReturnType<typeof setTimeout> | null = null;
  const selectedCount = computed(() => selectedIds.value.length);
  const selectedSet = computed(() => new Set(selectedIds.value));
  const shownSelectedCount = computed(
    () => shownIds.value.filter((id) => selectedSet.value.has(id)).length,
  );
  const hiddenSelectedCount = computed(() => selectedCount.value - shownSelectedCount.value);
  const areAllShownSelected = computed(
    () => shownIds.value.length > 0 && shownSelectedCount.value === shownIds.value.length,
  );
  const areSomeShownSelected = computed(
    () => shownSelectedCount.value > 0 && !areAllShownSelected.value,
  );

  function commit(next: string[]): void {
    committed = next;
    selectedIds.value = next;
  }

  function endRange(): void {
    rangeBase = null;
    rangeShown = null;
    if (pauseTimer !== null) {
      clearTimeout(pauseTimer);
      pauseTimer = null;
    }
    isCountPaused.value = false;
  }

  if (getCurrentScope()) {
    onScopeDispose(endRange);
  }

  // A range is anchor..target over one line; once the rows change it is over.
  watch(line, endRange);

  // The activate/deactivate flows and pruning replace the selection from outside; a range
  // announcement or base from before that would be stale.
  watch(selectedIds, (current) => {
    if (!haveSameIds(current, committed)) {
      committed = current;
      endRange();
    }
  });

  /** Empties the count for {@link REPEAT_PAUSE_MS} so an unchanged count is announced again. */
  function repeatCountAnnouncement(): void {
    if (pauseTimer !== null) {
      clearTimeout(pauseTimer);
    }
    isCountPaused.value = true;
    pauseTimer = setTimeout(() => {
      pauseTimer = null;
      isCountPaused.value = false;
    }, REPEAT_PAUSE_MS);
  }

  /** Moves the range anchor to a row without changing the selection (plain click, plain movement). */
  function setAnchor(key: string): void {
    anchorId.value = key;
    endRange();
  }

  /**
   * Toggles one mod (a Ctrl/Cmd+click, the checkbox or Space) and makes the row it was done on the
   * range anchor. `anchorKey` is that row's key when it is not the mod id (the Groups view).
   */
  function toggle(modId: string, anchorKey: string = modId): void {
    commit(
      selectedSet.value.has(modId)
        ? selectedIds.value.filter((id) => id !== modId)
        : [...selectedIds.value, modId],
    );
    setAnchor(anchorKey);
  }

  /**
   * Selects the rows from the anchor to the one named `key` (either direction), on top of what was
   * selected before the range began; a mod met twice is added once, and a row that holds no mod (a
   * folder header) adds nothing. With no usable anchor (none, or its row is not shown) the row is
   * added to the selection and becomes the anchor. A row that is not shown is ignored. A range
   * begun over different rows is over, so it starts again from the current selection.
   */
  function selectRange(key: string): void {
    const rowsInLine = line.value;
    const target = lineKeys.value.indexOf(key);
    if (target === -1) {
      return;
    }
    if (rangeShown !== null && rangeShown !== rowsInLine) {
      endRange();
    }
    const previousCount = selectedIds.value.length;
    const anchorIndex = anchorId.value === null ? -1 : lineKeys.value.indexOf(anchorId.value);
    if (anchorIndex === -1) {
      anchorId.value = key;
      endRange();
      const modId = rowsInLine[target]?.modId ?? null;
      if (modId !== null && !selectedSet.value.has(modId)) {
        commit([...selectedIds.value, modId]);
      }
      return;
    }
    const base = rangeBase ?? [...selectedIds.value];
    rangeBase = base;
    rangeShown = rowsInLine;
    const known = new Set(base);
    const additions: string[] = [];
    for (const row of rowsInLine.slice(
      Math.min(anchorIndex, target),
      Math.max(anchorIndex, target) + 1,
    )) {
      if (row.modId !== null && !known.has(row.modId)) {
        known.add(row.modId);
        additions.push(row.modId);
      }
    }
    const next = [...base, ...additions];
    commit(next);
    if (next.length === previousCount) {
      repeatCountAnnouncement();
    }
  }

  /**
   * Shift+Arrow: extends from the anchor to the row focus moved to. With no usable anchor (focus
   * arrived by Tab) the row focus left becomes it first.
   */
  function extendSelection(fromKey: string, toKey: string): void {
    const hasAnchor = anchorId.value !== null && lineKeys.value.includes(anchorId.value);
    if (!hasAnchor && lineKeys.value.includes(fromKey)) {
      anchorId.value = fromKey;
    }
    selectRange(toKey);
  }

  /** Adds every shown row to the selection, keeping hidden selected ids. */
  function selectAllShown(): void {
    endRange();
    const additions = shownIds.value.filter((id) => !selectedSet.value.has(id));
    commit([...selectedIds.value, ...additions]);
  }

  /** Removes the shown rows from the selection, keeping hidden selected ids. */
  function deselectShown(): void {
    endRange();
    const shown = new Set(shownIds.value);
    commit(selectedIds.value.filter((id) => !shown.has(id)));
  }

  /** Removes every selected id, hidden ones included. */
  function clear(): void {
    commit([]);
    anchorId.value = null;
    endRange();
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
    endRange();
    commit(selectedIds.value.filter((id) => shown.has(id)));
  }

  return {
    shownCount: computed(() => shownIds.value.length),
    selectedCount,
    isCountPaused: computed(() => isCountPaused.value),
    setAnchor,
    toggle,
    selectRange,
    extendSelection,
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
