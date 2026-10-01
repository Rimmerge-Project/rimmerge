import { defineStore } from "pinia";
import { ref } from "vue";

/**
 * The merge editor's own UI-only state: the keyboard cursor's row and
 * column, and whether the auto-resolved and unchanged sections are
 * shown. Server data (the preview itself) is Pinia Colada's job — see
 * `queries/merge.ts`; this store never holds a DTO. A fresh instance per
 * editor visit would work just as well, but a store (reset on `Esc`/`q`,
 * see {@link reset}) matches every other keyboard-driven page in this
 * app (`useInboxStore`). The editor never touches `useInboxStore`'s own
 * cursor, so the inbox's selection is already "preserved" on return by
 * simply never being written to.
 */
export const useMergeStore = defineStore("merge", () => {
  /** Index into the currently visible field rows (conflicts, then auto, then unchanged). */
  const rowCursor = ref(0);
  /** Index into `[...owners, "result"]` — see `useMergeKeys`'s column model. */
  const columnCursor = ref(0);
  /** Whether the auto-resolved section (`OneSided`/`Agreeing` rows) is collapsed. Collapsed by default. */
  const collapsedAuto = ref(true);
  /** Whether `Unchanged` rows are shown at all. Hidden by default. */
  const showUnchanged = ref(false);
  /**
   * Explicit per-container collapse overrides, keyed by the container's
   * own path text (e.g. `"wildAnimals"`) — absent means "use the
   * data-driven default" (see `utils/mergeFieldContainers.ts`'s
   * `effectiveContainerCollapsed`). Only ever set by a user click on a
   * container header in the auto-resolved/unchanged sections; the
   * conflicts section's own container header never collapses, matching
   * `collapsedAuto`/`showUnchanged`'s own "conflicts always visible" rule.
   */
  const containerOverrides = ref<Record<string, boolean>>({});

  function setRowCursor(index: number): void {
    rowCursor.value = Math.max(0, index);
  }

  function setColumnCursor(index: number): void {
    columnCursor.value = Math.max(0, index);
  }

  function toggleAuto(): void {
    collapsedAuto.value = !collapsedAuto.value;
  }

  function toggleUnchanged(): void {
    showUnchanged.value = !showUnchanged.value;
  }

  /** Sets an explicit collapse override for `container` — the effective value the caller (`MergeFieldTable`, via `MergeEditorPage`) already computed by flipping whatever it was showing. */
  function setContainerCollapsed(container: string, collapsed: boolean): void {
    containerOverrides.value = { ...containerOverrides.value, [container]: collapsed };
  }

  /** Resets every field back to its default — called on leaving the editor. */
  function reset(): void {
    rowCursor.value = 0;
    columnCursor.value = 0;
    collapsedAuto.value = true;
    showUnchanged.value = false;
    containerOverrides.value = {};
  }

  return {
    rowCursor,
    columnCursor,
    collapsedAuto,
    showUnchanged,
    containerOverrides,
    setRowCursor,
    setColumnCursor,
    toggleAuto,
    toggleUnchanged,
    setContainerCollapsed,
    reset,
  };
});
