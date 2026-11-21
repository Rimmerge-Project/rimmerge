/** What a pointer gesture on a row asks its table's owner to do. `TTarget` names the row. */
export interface RowPointerActions<TTarget> {
  /** A plain click: open the row (the info panel). */
  open: (target: TTarget) => void;
  /** A plain click or plain keyboard movement: the range anchor follows to this row. */
  anchor: (target: TTarget) => void;
  /** A Ctrl/Cmd+click: toggle this row's selection. */
  toggle: (target: TTarget) => void;
  /** A Shift+click: add the range from the selection anchor to this row. */
  range: (target: TTarget) => void;
  /** A plain double click: activate or deactivate this mod. */
  toggleActive: (target: TTarget) => void;
}

/** Descendants of a row that handle their own clicks, so a double click on them is not the row's. */
const ROW_CONTROL_SELECTOR = 'button, input, a, [role="checkbox"], .p-checkbox';

/**
 * The pointer model both mod tables share: Ctrl/Cmd+click toggles and Shift+click extends a range
 * (neither opening the info panel), a plain click only opens it and never touches the selection,
 * and a plain double click on the row itself asks to toggle the mod's activation.
 */
export function useRowPointer<TTarget>({
  isSelectable,
  actions,
}: {
  isSelectable: () => boolean;
  actions: RowPointerActions<TTarget>;
}) {
  function onRowClick(event: MouseEvent, target: TTarget): void {
    if (isSelectable() && (event.ctrlKey || event.metaKey)) {
      actions.toggle(target);
      return;
    }
    if (isSelectable() && event.shiftKey) {
      // Shift+click would otherwise also select the text between the two rows.
      window.getSelection()?.removeAllRanges();
      actions.range(target);
      return;
    }
    actions.anchor(target);
    actions.open(target);
  }

  /** A double click would also select the word under the pointer; the row toggles instead. */
  function onRowMouseDown(event: MouseEvent): void {
    if (event.detail > 1) {
      event.preventDefault();
    }
  }

  /**
   * A plain double click (no Ctrl/Cmd/Shift, which belong to selection) asks to toggle the row's
   * activation. Its two clicks already ran `onRowClick`, which only opens the info panel and moves
   * the anchor, so it never fights the single click.
   */
  function onRowDblClick(event: MouseEvent, target: TTarget): void {
    if (event.ctrlKey || event.metaKey || event.shiftKey) {
      return;
    }
    // A double click on a control inside the row (checkbox, menu button, group chip) belongs to it.
    if (event.target instanceof Element && event.target.closest(ROW_CONTROL_SELECTOR) !== null) {
      return;
    }
    actions.toggleActive(target);
  }

  /**
   * A Shift+click on the row checkbox extends a range like one on the row; any other click is left
   * to the checkbox, which toggles through its own change event.
   */
  function onCheckboxClick(event: MouseEvent, target: TTarget): void {
    if (event.shiftKey) {
      event.preventDefault();
      window.getSelection()?.removeAllRanges();
      actions.range(target);
    }
  }

  return { onRowClick, onRowMouseDown, onRowDblClick, onCheckboxClick };
}
