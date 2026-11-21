/** What a Mods table's rows may be dragged to, set by the page from the drag in progress. */
export type DropMode =
  /** No drag is in progress. */
  | "off"
  /** Accepts the drag at a position between rows (a line shows where). */
  | "position"
  /** Accepts the drag anywhere on the list (it has no order of its own to drop into). */
  | "pane"
  /** Refuses the drag (the not-allowed cursor shows, with the reason). */
  | "refused";

/** Which row action button the rows carry. */
export type RowMenu = "none" | "move" | "activate";

/** A row menu entry that is not a one-step move: a position, or one of the mod's own organization dialogs. */
export type MenuAction = "top" | "bottom" | "after" | "activateAt" | "groups" | "color";

/** Whether rows have a menu at all: a position entry, or the mod's own organization entries. */
export function hasRowMenuFor(rowMenu: RowMenu, hasOrganizationActions: boolean): boolean {
  return rowMenu !== "none" || hasOrganizationActions;
}

/**
 * Whether the menu would hold one entry only: the Inactive list's "Activate at..." while
 * organization is not usable. The row button then runs it directly instead of opening a menu.
 */
export function isDirectActivateFor(rowMenu: RowMenu, hasOrganizationActions: boolean): boolean {
  return rowMenu === "activate" && !hasOrganizationActions;
}

/** What a Groups view folder header's menu can do for its folder. */
export type FolderAction = "select" | "activate" | "deactivate";
