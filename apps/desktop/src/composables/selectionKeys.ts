import { isInert } from "@/composables/shortcutTargets";

/** What the two selection keys ask a table to do. */
export interface SelectionKeyActions {
  /** Selects every shown mod (Ctrl/Cmd+A). */
  selectAll: () => void;
  /** Clears the selection (Escape, while something is selected). */
  clearSelection: () => void;
}

/**
 * Ctrl/Cmd+A selects every shown mod and Escape clears a non-empty selection (stopping the event
 * so the same press does not also close the info panel). Both defer to text entry and dialogs.
 * Returns whether the key was consumed.
 */
export function handleSelectionKey(
  event: KeyboardEvent,
  {
    isSelectable,
    hasSelection,
    actions,
  }: { isSelectable: () => boolean; hasSelection: () => boolean; actions: SelectionKeyActions },
): boolean {
  // A focused row checkbox is a native input but not text entry: the keys still mean the list.
  const isRowCheckbox =
    event.target instanceof HTMLInputElement && event.target.type === "checkbox";
  if (!isSelectable() || (!isRowCheckbox && isInert(event.target))) {
    return false;
  }
  const isSelectAll = (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a";
  if (isSelectAll) {
    event.preventDefault();
    actions.selectAll();
    return true;
  }
  if (event.key === "Escape" && hasSelection()) {
    event.stopPropagation();
    actions.clearSelection();
    return true;
  }
  return false;
}
