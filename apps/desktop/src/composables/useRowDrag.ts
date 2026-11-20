import { ref, watch } from "vue";

import type { DropMode } from "@/components/mods/ModTable.vue";
import { useDragAutoScroll } from "@/composables/useDragAutoScroll";

/** What a drag tells the table's caller. */
export interface RowDragActions {
  dragStart: (modId: string) => void;
  dragEnd: () => void;
  /** A drop; `slot` is the gap before that row index (the row count is after the last row). */
  drop: (slot: number) => void;
}

/** What {@link useRowDrag} reads from the table. */
export interface RowDragOptions {
  getScroller: () => HTMLElement | null;
  rowCount: () => number;
  /** Every row's fixed height, which maps a pointer position to a gap. */
  rowHeightPx: number;
  isDraggable: () => boolean;
  /** What the table does with the drag in progress, set by the page. */
  dropMode: () => DropMode;
  actions: RowDragActions;
}

/**
 * A virtualized mod table's native HTML5 drag and drop: picking a row up, the drop line in the
 * gap nearest the pointer (`position`), the whole-pane outline (`pane`), the refused hover
 * (`refused`), and edge auto-scroll while a drag hovers.
 */
export function useRowDrag({
  getScroller,
  rowCount,
  rowHeightPx,
  isDraggable,
  dropMode,
  actions,
}: RowDragOptions) {
  const autoScroll = useDragAutoScroll(getScroller);
  /** The gap a position drop would land in (a line shows it); `null` when nothing hovers. */
  const dragSlot = ref<number | null>(null);
  const isHoveringPane = ref(false);
  const isHoveringRefused = ref(false);

  function clearHover(): void {
    dragSlot.value = null;
    isHoveringPane.value = false;
    isHoveringRefused.value = false;
    autoScroll.stop();
  }

  watch(dropMode, (mode) => {
    if (mode === "off") {
      clearHover();
    }
  });

  function onRowDragStart(event: DragEvent, modId: string): void {
    if (!isDraggable()) {
      event.preventDefault();
      return;
    }
    if (event.dataTransfer) {
      // Some engines refuse to start a drag with no data.
      event.dataTransfer.setData("text/plain", modId);
      event.dataTransfer.effectAllowed = "move";
    }
    actions.dragStart(modId);
  }

  function onRowDragEnd(): void {
    clearHover();
    actions.dragEnd();
  }

  /** The gap nearest the pointer: the row's top half is the gap before it, the bottom half after. */
  function slotAt(event: DragEvent): number {
    const scroller = getScroller();
    if (!scroller) {
      return rowCount();
    }
    const offset = event.clientY - scroller.getBoundingClientRect().top + scroller.scrollTop;
    return Math.min(Math.max(Math.round(offset / rowHeightPx), 0), rowCount());
  }

  function onDragOver(event: DragEvent): void {
    const mode = dropMode();
    if (mode === "off") {
      return;
    }
    if (mode === "refused") {
      // Not cancelling the event leaves the drop refused and shows the not-allowed cursor.
      if (event.dataTransfer) {
        event.dataTransfer.dropEffect = "none";
      }
      isHoveringRefused.value = true;
      return;
    }
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = "move";
    }
    autoScroll.update(event);
    if (mode === "position") {
      dragSlot.value = slotAt(event);
    } else {
      isHoveringPane.value = true;
    }
  }

  function onDragLeave(event: DragEvent): void {
    const next = event.relatedTarget;
    if (next instanceof Node && getScroller()?.contains(next)) {
      return;
    }
    clearHover();
  }

  function onDrop(event: DragEvent): void {
    const mode = dropMode();
    if (mode !== "position" && mode !== "pane") {
      return;
    }
    event.preventDefault();
    const slot = mode === "position" ? slotAt(event) : rowCount();
    clearHover();
    actions.drop(slot);
  }

  return {
    dragSlot,
    isHoveringPane,
    isHoveringRefused,
    onRowDragStart,
    onRowDragEnd,
    onDragOver,
    onDragLeave,
    onDrop,
  };
}
