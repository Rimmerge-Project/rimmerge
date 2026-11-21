import { computed, nextTick, type Ref, watch } from "vue";

import { handleSelectionKey } from "@/composables/selectionKeys";
import { isInert } from "@/composables/shortcutTargets";
import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import type { TreeRow } from "@/utils/groupedEntries";

/** What a key on a Groups view row asks the table to do; the table owns events and menus. */
export interface TreeKeyActions {
  /** Opens the mod (Enter on a mod row). */
  open: (modId: string) => void;
  /** Toggles the mod's checkbox (Space on a mod row). */
  toggleSelected: (modId: string, rowKey: string) => void;
  /** Moves the range anchor to the row focus moved to (plain arrow, Home, End). */
  setAnchor: (rowKey: string) => void;
  /** Adds the range from the selection anchor to the row (Shift+Space on a mod row). */
  selectRange: (rowKey: string) => void;
  /** Extends the selection from the anchor to the row focus just moved to (Shift+Arrow). */
  extendSelection: (fromKey: string, toKey: string) => void;
  selectAll: () => void;
  clearSelection: () => void;
  /** Folds or opens a folder (Enter and Space, Left and Right on its header). */
  setFolded: (folder: FolderKeyDto, isCollapsed: boolean) => void;
  /** Opens the row's menu (Shift+F10 or the ContextMenu key). */
  openMenu: (event: Event, row: TreeRow) => void;
  /** Roving focus moved to a mod row from the keyboard. */
  focusMod: (modId: string) => void;
}

/** What {@link useTreeRowKeys} reads from the table. */
export interface TreeRowKeysOptions {
  rows: () => readonly TreeRow[];
  /** The roving-tabindex row's key; `null` before anything has been focused. */
  focusedKey: Ref<string | null>;
  hasSelection: () => boolean;
  /** Whether mod rows have a menu at all (it needs a position entry or organization). */
  hasModMenu: () => boolean;
  scrollToIndex: (index: number) => void;
  getScroller: () => HTMLElement | null;
  actions: TreeKeyActions;
}

/**
 * The Groups view's keyboard model, one roving tab stop over every visible row (headers included)
 * and keyed by entry key, since one mod can sit under several folders. Up/Down/Home/End move focus;
 * on a header Right opens a folded folder or enters an open one, Left folds an open folder or goes
 * to the parent, Enter and Space toggle it; on a mod row Enter opens the mod, Space toggles its
 * checkbox and Left goes to the folder header. Shift+Arrow and Shift+Space extend a range
 * over the visible rows, Ctrl/Cmd+A and Escape select all and clear. Alt+Arrow does nothing:
 * reordering is off in this view.
 */
export function useTreeRowKeys({
  rows,
  focusedKey,
  hasSelection,
  hasModMenu,
  scrollToIndex,
  getScroller,
  actions,
}: TreeRowKeysOptions) {
  const rovingIndex = computed<number | null>(() => {
    const index = rows().findIndex((row) => row.key === focusedKey.value);
    return index >= 0 ? index : null;
  });

  function onRowFocus(key: string): void {
    focusedKey.value = key;
  }

  /** Whether DOM focus is on the row `key` or something inside it. */
  function isFocusWithin(key: string): boolean {
    const active = document.activeElement;
    const row = active instanceof Element ? active.closest<HTMLElement>("[data-entry-key]") : null;
    return row !== null && row.dataset["entryKey"] === key && getScroller()?.contains(row) === true;
  }

  // A copy hidden by a fold hands roving focus to the folder it sat in; a row that is gone for any
  // other reason falls back to the first row (no row is the roving one). When DOM focus was on the
  // removed row it would fall to the body, so it follows; otherwise only the tab stop moves.
  watch(rows, (current, previous) => {
    const key = focusedKey.value;
    if (key === null || current.some((row) => row.key === key)) {
      return;
    }
    const parentKey = previous?.find((row) => row.key === key)?.parentKey ?? null;
    const target =
      parentKey !== null && current.some((row) => row.key === parentKey) ? parentKey : null;
    const hadDomFocus = isFocusWithin(key);
    focusedKey.value = target;
    if (target !== null && hadDomFocus) {
      void focusKey(target);
    }
  });

  /** Scrolls the row with `key` into view and focuses it. A key that is not shown is ignored. */
  async function focusKey(key: string): Promise<void> {
    const index = rows().findIndex((row) => row.key === key);
    if (index === -1) {
      return;
    }
    scrollToIndex(index);
    focusedKey.value = key;
    // The row may not be in the DOM yet (virtualization) until the scroll re-renders.
    await nextTick();
    const scroller = getScroller();
    const element = scroller
      ? [...scroller.querySelectorAll<HTMLElement>("[data-entry-key]")].find(
          (candidate) => candidate.dataset["entryKey"] === key,
        )
      : undefined;
    element?.focus();
  }

  /**
   * Focuses a copy of `modId`: the row focus last rested on when it is a copy of this mod, else
   * the first copy. A mod that is not shown (its folders are folded) is ignored.
   */
  async function focusMod(modId: string): Promise<void> {
    const current = rows().find((row) => row.key === focusedKey.value);
    const target =
      current?.kind === "mod" && current.modId === modId
        ? current
        : rows().find((row) => row.kind === "mod" && row.modId === modId);
    if (target !== undefined) {
      await focusKey(target.key);
    }
  }

  async function moveFocus(delta: number, toEnd?: "start" | "end"): Promise<TreeRow | null> {
    const all = rows();
    if (all.length === 0) {
      return null;
    }
    const current = rovingIndex.value ?? 0;
    const target =
      toEnd === "start"
        ? 0
        : toEnd === "end"
          ? all.length - 1
          : Math.min(Math.max(current + delta, 0), all.length - 1);
    const row = all[target];
    if (row === undefined) {
      return null;
    }
    await focusKey(row.key);
    if (row.kind === "mod") {
      actions.focusMod(row.modId);
    }
    return row;
  }

  async function moveFocusAndAnchor(delta: number, toEnd?: "start" | "end"): Promise<void> {
    const row = await moveFocus(delta, toEnd);
    if (row !== null) {
      actions.setAnchor(row.key);
    }
  }

  async function extendFocus(delta: number, fromKey: string): Promise<void> {
    const row = await moveFocus(delta);
    if (row !== null && row.key !== fromKey) {
      actions.extendSelection(fromKey, row.key);
    }
  }

  /** A folder's header: fold, open, enter or leave it. */
  function onFolderKey(event: KeyboardEvent, row: Extract<TreeRow, { kind: "folder" }>): boolean {
    const isCollapsed = row.entry.isCollapsed;
    switch (event.key) {
      case "Enter":
      case " ":
        event.preventDefault();
        actions.setFolded(row.folder, !isCollapsed);
        return true;
      case "ArrowRight": {
        event.preventDefault();
        if (isCollapsed) {
          actions.setFolded(row.folder, false);
          return true;
        }
        const next = rows()[(rovingIndex.value ?? 0) + 1];
        if (next !== undefined && next.parentKey === row.key) {
          void focusKey(next.key);
          actions.setAnchor(next.key);
        }
        return true;
      }
      case "ArrowLeft":
        event.preventDefault();
        if (!isCollapsed) {
          actions.setFolded(row.folder, true);
        } else if (row.parentKey !== null) {
          void focusKey(row.parentKey);
          actions.setAnchor(row.parentKey);
        }
        return true;
      default:
        return false;
    }
  }

  /** A mod row: open it, toggle its checkbox, or go back to its folder. */
  function onModKey(event: KeyboardEvent, row: Extract<TreeRow, { kind: "mod" }>): boolean {
    switch (event.key) {
      case "Enter":
        actions.open(row.modId);
        return true;
      case " ":
        event.preventDefault();
        if (event.shiftKey) {
          actions.selectRange(row.key);
        } else {
          actions.toggleSelected(row.modId, row.key);
        }
        return true;
      case "ArrowLeft":
        event.preventDefault();
        void focusKey(row.parentKey);
        actions.setAnchor(row.parentKey);
        return true;
      default:
        return false;
    }
  }

  function onMenuKey(event: KeyboardEvent, row: TreeRow): boolean {
    const isMenuKey = event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey);
    if (!isMenuKey || (row.kind === "mod" && !hasModMenu()) || isInert(event.target)) {
      return false;
    }
    // Without this the browser's own context menu opens over ours.
    event.preventDefault();
    actions.openMenu(event, row);
    return true;
  }

  function onRowKeydown(event: KeyboardEvent, row: TreeRow): void {
    const isAltArrow = event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown");
    if (isAltArrow) {
      // Reordering is off here, and a plain focus move must not happen on an Alt press either.
      if (!isInert(event.target)) {
        event.preventDefault();
      }
      return;
    }
    const isHandled =
      onMenuKey(event, row) ||
      handleSelectionKey(event, {
        isSelectable: () => true,
        hasSelection,
        actions,
      });
    if (isHandled) {
      return;
    }
    if (row.kind === "folder" ? onFolderKey(event, row) : onModKey(event, row)) {
      return;
    }
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        void (event.shiftKey ? extendFocus(1, row.key) : moveFocusAndAnchor(1));
        break;
      case "ArrowUp":
        event.preventDefault();
        void (event.shiftKey ? extendFocus(-1, row.key) : moveFocusAndAnchor(-1));
        break;
      case "Home":
        event.preventDefault();
        void moveFocusAndAnchor(0, "start");
        break;
      case "End":
        event.preventDefault();
        void moveFocusAndAnchor(0, "end");
        break;
      default:
        break;
    }
  }

  return { rovingIndex, onRowFocus, onRowKeydown, focusKey, focusMod };
}
