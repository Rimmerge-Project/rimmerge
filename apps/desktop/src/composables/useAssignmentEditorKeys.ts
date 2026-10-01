import { type MaybeRefOrGetter, onMounted, onScopeDispose, toValue } from "vue";

import { isInert, ownsArrowKeys, ownsEnterNatively } from "@/composables/shortcutTargets";
import type { CoverageRowDto } from "@/types/generated/CoverageRowDto";

/** The row editor's own item-picker search boxes (`ItemPicker.vue`'s own `data-testid`) — `Enter`'s preferred focus target over a plain first input. */
const FIRST_PICKER_SELECTOR = '[data-testid^="item-picker-search-"]';
/** Every other focusable control, the fallback when the section has no item-slot picker at all (a scalar-only free-standing row, say). */
const FIRST_FOCUSABLE_SELECTOR = "input, select, textarea, button:not([disabled])";

/** What `useAssignmentEditorKeys` needs from its host page. */
export interface UseAssignmentEditorKeysOptions {
  /** The coverage work queue, in display order — what `j`/`k` step through. */
  rows: MaybeRefOrGetter<CoverageRowDto[]>;
  /** The currently selected row's own target, or `null` before any selection. */
  selectedTarget: MaybeRefOrGetter<CoverageRowDto["target"] | null>;
  /** Selects a row — called by `j`/`k` with the row now under the cursor. */
  onSelect: (row: CoverageRowDto) => void;
  /** The row editor's own pane, whose first picker/focusable control `Enter` focuses. `null`/`undefined` while nothing is selected. */
  editorContainer: MaybeRefOrGetter<HTMLElement | null | undefined>;
}

/** The index of `rows`' entry matching `target`, or `-1` when `target` is `null` or matches nothing (a stale selection). */
function indexOfSelected(rows: CoverageRowDto[], target: CoverageRowDto["target"] | null): number {
  if (!target) {
    return -1;
  }
  return rows.findIndex(
    (row) =>
      row.target.def.defType === target.def.defType &&
      row.target.def.defName === target.def.defName,
  );
}

/**
 * Registers the editor's keyboard shortcuts: `j`/`k` (and the arrow equivalents) move through the
 * coverage work queue, `Enter` focuses the row editor's own first
 * picker — reusing {@link isInert}/{@link ownsEnterNatively}, the same
 * inert rules `useInboxKeys`/`useMergeKeys` share, so a keystroke typed
 * into a search box, a select, or a dialog is never hijacked. Unlike
 * those two composables, the cursor here is derived (the selected
 * row's own index into `rows`), not stored — `AssignmentEditorPage`
 * already owns `selectedTarget` as the single source of truth for which
 * row is current, and duplicating that into a second cursor ref would
 * just be two numbers that could disagree.
 */
export function useAssignmentEditorKeys(options: UseAssignmentEditorKeysOptions) {
  function moveCursor(delta: number): void {
    const rows = toValue(options.rows);
    if (rows.length === 0) {
      return;
    }
    const current = indexOfSelected(rows, toValue(options.selectedTarget));
    const nextIndex = Math.min(Math.max(current + delta, 0), rows.length - 1);
    const nextRow = rows[nextIndex];
    if (nextRow) {
      options.onSelect(nextRow);
    }
  }

  /** Focuses the row editor's own first item-picker search box, falling back to its first focusable control when it has none (a scalar-only section). */
  function focusFirstPicker(): void {
    const container = toValue(options.editorContainer);
    if (!container) {
      return;
    }
    const target =
      container.querySelector<HTMLElement>(FIRST_PICKER_SELECTOR) ??
      container.querySelector<HTMLElement>(FIRST_FOCUSABLE_SELECTOR);
    target?.focus();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (isInert(event.target)) {
      return;
    }
    // A focused radio group moves its own selection on the arrows; every other key is ours.
    if (event.key.startsWith("Arrow") && ownsArrowKeys(event.target)) {
      return;
    }

    switch (event.key) {
      case "j":
      case "ArrowDown":
        event.preventDefault();
        moveCursor(1);
        break;
      case "k":
      case "ArrowUp":
        event.preventDefault();
        moveCursor(-1);
        break;
      case "Enter":
        if (ownsEnterNatively(event.target)) {
          break;
        }
        event.preventDefault();
        focusFirstPicker();
        break;
      default:
        break;
    }
  }

  onMounted(() => {
    window.addEventListener("keydown", handleKeydown);
  });
  onScopeDispose(() => {
    window.removeEventListener("keydown", handleKeydown);
  });

  return { handleKeydown, moveCursor, focusFirstPicker };
}
