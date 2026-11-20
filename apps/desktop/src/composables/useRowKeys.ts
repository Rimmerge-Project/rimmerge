import { computed, nextTick, type Ref } from "vue";

import type { RowMenu } from "@/components/mods/ModTable.vue";
import { isInert } from "@/composables/shortcutTargets";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";

/** What a row key asks the table to do; the table owns the rows' events and menus. */
export interface RowKeyActions {
  /** Opens the mod (Enter). */
  open: (modId: string) => void;
  /** Toggles the row's checkbox (Space, while the table is selectable). */
  toggleSelected: (modId: string) => void;
  /** One shown row up or down (Alt+ArrowUp/Down, while the rows carry a Move menu). */
  moveStep: (modId: string, direction: "up" | "down") => void;
  /** Opens the row's menu (Shift+F10 or the ContextMenu key). */
  openMenu: (event: Event, modId: string) => void;
  /** Roving focus moved to a row from the keyboard. */
  focusMod: (modId: string) => void;
}

/** What {@link useRowKeys} reads from the table. */
export interface RowKeysOptions {
  mods: () => readonly ModListRowDto[];
  /** The roving-tabindex row's id, owned by the table's caller; `null` before anything has been focused. */
  focusedId: Ref<string | null>;
  isSelectable: () => boolean;
  rowMenu: () => RowMenu;
  /** Scrolls the virtualized list so the row at `index` is rendered. */
  scrollToIndex: (index: number) => void;
  getScroller: () => HTMLElement | null;
  actions: RowKeyActions;
}

/**
 * A mod table's keyboard model: one roving-tabindex row at a time, arrows/Home/End moving that
 * focus, Enter opening, Space toggling the checkbox, Alt+Arrow moving the row, and Shift+F10 or
 * the ContextMenu key opening the row menu.
 */
export function useRowKeys({
  mods,
  focusedId,
  isSelectable,
  rowMenu,
  scrollToIndex,
  getScroller,
  actions,
}: RowKeysOptions) {
  /** The roving-tabindex row — only this one is in the Tab order at a time. `null` before anything has been focused (every row then reads `tabindex="0"`, so the first Tab press lands on the first one). */
  const rovingIndex = computed<number | null>(() => {
    const index = mods().findIndex((mod) => mod.modId === focusedId.value);
    return index >= 0 ? index : null;
  });

  /**
   * Tracks the roving-tabindex target only — never emits `focusMod`. A
   * plain click already fires `openRow`'s own immediate `selectMod`
   * synchronously, and a click also natively focuses the row (it has a
   * tabindex), so this same DOM `focus` event fires right alongside that
   * click every time. Emitting `focusMod` here too would arm the caller's
   * debounced auto-follow (`ModsPage.vue`'s `followFocus`) on every
   * click, racing it against whatever explicit navigation the click (or
   * something the user does moments later, e.g. inside the now-open
   * panel) already triggered — a stale timer could fire mid-flight and
   * cancel that explicit navigation. Only {@link moveFocus} (pure
   * keyboard arrow/Home/End movement, with no immediate navigation of its
   * own) emits `focusMod`.
   */
  function onRowFocus(modId: string): void {
    focusedId.value = modId;
  }

  /** Moves roving focus by `delta` rows (±1), or to the very first/last row when `toEnd` is given. */
  async function moveFocus(delta: number, toEnd?: "start" | "end"): Promise<void> {
    const rows = mods();
    if (rows.length === 0) {
      return;
    }
    const current = rovingIndex.value ?? 0;
    const target =
      toEnd === "start"
        ? 0
        : toEnd === "end"
          ? rows.length - 1
          : Math.min(Math.max(current + delta, 0), rows.length - 1);
    const targetMod = rows[target];
    if (!targetMod) {
      return;
    }
    await focusRow(targetMod.modId);
    actions.focusMod(targetMod.modId);
  }

  /**
   * Scrolls the row for `modId` into view and focuses it, whether or not it is rendered yet.
   * A row that is not in the list is ignored.
   */
  async function focusRow(modId: string): Promise<void> {
    const index = mods().findIndex((mod) => mod.modId === modId);
    if (index === -1) {
      return;
    }
    scrollToIndex(index);
    focusedId.value = modId;
    // The target row may not exist in the DOM yet (virtualization) until
    // the scroll above re-renders `visibleRows` — wait for Vue to flush
    // that render before focusing it.
    await nextTick();
    const element = getScroller()?.querySelector<HTMLElement>(`[data-testid="mod-row-${modId}"]`);
    element?.focus();
  }

  /**
   * Alt+ArrowUp/Down moves the row (the Active list only). Text entry and dialogs keep their own
   * keys (`isInert`); plain arrows stay the roving focus. Returns whether the key was consumed.
   */
  function onMoveKey(event: KeyboardEvent, modId: string): boolean {
    if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) {
      return false;
    }
    if (isInert(event.target)) {
      return true;
    }
    event.preventDefault();
    if (rowMenu() === "move") {
      actions.moveStep(modId, event.key === "ArrowUp" ? "up" : "down");
    }
    return true;
  }

  /**
   * Shift+F10 or the ContextMenu key on a focused row opens that row's menu, the keyboard route to
   * the ⋮ button (which is out of the Tab order, so rows stay one tab stop each). The same inert
   * rules as the other row shortcuts apply. Returns whether the key was consumed.
   */
  function onMenuKey(event: KeyboardEvent, modId: string): boolean {
    const isMenuKey = event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey);
    if (!isMenuKey || rowMenu() === "none" || isInert(event.target)) {
      return false;
    }
    // Without this the browser's own context menu opens over ours.
    event.preventDefault();
    actions.openMenu(event, modId);
    return true;
  }

  function onRowKeydown(event: KeyboardEvent, modId: string): void {
    if (onMoveKey(event, modId) || onMenuKey(event, modId)) {
      return;
    }
    switch (event.key) {
      case "Enter":
        actions.open(modId);
        break;
      case " ":
        if (isSelectable()) {
          event.preventDefault();
          actions.toggleSelected(modId);
        }
        break;
      case "ArrowDown":
        event.preventDefault();
        void moveFocus(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        void moveFocus(-1);
        break;
      case "Home":
        event.preventDefault();
        void moveFocus(0, "start");
        break;
      case "End":
        event.preventDefault();
        void moveFocus(0, "end");
        break;
      default:
        break;
    }
  }

  return { rovingIndex, onRowFocus, onRowKeydown, focusRow };
}
