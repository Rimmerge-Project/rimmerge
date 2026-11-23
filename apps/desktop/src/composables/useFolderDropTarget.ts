import { onScopeDispose, ref } from "vue";

import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import { ROW_DRAG_TYPE } from "@/utils/groupDrop";
import type { TreeRow } from "@/utils/groupedEntries";

/** How long a drag must hover a folded folder before it opens, as in the group rail. */
export const HOVER_OPEN_MS = 800;

type FolderRow = Extract<TreeRow, { kind: "folder" }>;

/**
 * A Groups view folder header as a drop target for mods. A group's header takes the drag whatever
 * the pane does with a drop elsewhere (even a pane that refuses every drop), and a drop on it is
 * only ever an add to that group: it never yields a position and the mods stay where they are. "Not in
 * a group" refuses a drop (there is nothing to add to). A folded header opens after a short
 * hover so a subfolder can be reached.
 */
export function useFolderDropTarget({
  isDragActive,
  openFolder,
  dropOnGroup,
  clearPaneHover,
}: {
  /** Whether a mod drag is in progress (the page's drag, not a file dragged in from outside). */
  isDragActive: () => boolean;
  /** Opens a folded folder after the hover delay. */
  openFolder: (folder: FolderKeyDto) => void;
  /** The mods were dropped on this group's header. */
  dropOnGroup: (groupId: number) => void;
  /** Clears the pane-wide hover the table drew before the pointer reached a header. */
  clearPaneHover: () => void;
}) {
  /** The header the drag hovers, to draw it as the drop target. */
  const hoverKey = ref<string | null>(null);
  /** Whether the hovered header refuses the drop. */
  const isRefusedHover = ref(false);
  let openTimer: ReturnType<typeof setTimeout> | undefined;
  let timerKey: string | null = null;

  function cancelOpen(): void {
    clearTimeout(openTimer);
    openTimer = undefined;
    timerKey = null;
  }

  function clear(): void {
    hoverKey.value = null;
    isRefusedHover.value = false;
    cancelOpen();
  }

  onScopeDispose(cancelOpen);

  function scheduleOpen(row: FolderRow): void {
    if (!row.entry.isCollapsed || timerKey === row.key) {
      return;
    }
    cancelOpen();
    timerKey = row.key;
    openTimer = setTimeout(() => {
      timerKey = null;
      openFolder(row.folder);
    }, HOVER_OPEN_MS);
  }

  /** A mod drag by the app's own type: a file or text dragged in is not one, whatever the page holds. */
  function isModDrag(event: DragEvent): boolean {
    return isDragActive() && (event.dataTransfer?.types.includes(ROW_DRAG_TYPE) ?? false);
  }

  function onHeaderDragOver(event: DragEvent, row: FolderRow): void {
    if (!isModDrag(event)) {
      return;
    }
    // The pane's own hover and drop handling are for the gaps between folders.
    event.stopPropagation();
    clearPaneHover();
    hoverKey.value = row.key;
    if (row.folder.kind === "ungrouped") {
      // Not cancelling the event leaves the drop refused and shows the not-allowed cursor.
      isRefusedHover.value = true;
      if (event.dataTransfer) {
        event.dataTransfer.dropEffect = "none";
      }
      return;
    }
    isRefusedHover.value = false;
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = "copy";
    }
    scheduleOpen(row);
  }

  function onHeaderDragLeave(event: DragEvent, row: FolderRow): void {
    const next = event.relatedTarget;
    const header = event.currentTarget;
    if (next instanceof Node && header instanceof HTMLElement && header.contains(next)) {
      return;
    }
    if (hoverKey.value === row.key) {
      clear();
    }
  }

  function onHeaderDrop(event: DragEvent, row: FolderRow): void {
    if (!isModDrag(event)) {
      return;
    }
    event.stopPropagation();
    clear();
    if (row.folder.kind === "group") {
      event.preventDefault();
      dropOnGroup(row.folder.groupId);
    }
  }

  return { hoverKey, isRefusedHover, onHeaderDragOver, onHeaderDragLeave, onHeaderDrop, clear };
}
