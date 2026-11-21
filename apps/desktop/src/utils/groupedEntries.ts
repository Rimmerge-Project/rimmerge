import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import type { GroupedEntryDto } from "@/types/generated/GroupedEntryDto";

/** The short token a folder carries in keys and test ids: its group id, or `u` for "Not in a group". */
export function folderToken(folder: FolderKeyDto): string {
  return folder.kind === "group" ? String(folder.groupId) : "u";
}

/** The token as a test id segment: the group id, or `ungrouped`. */
export function folderTestToken(folder: FolderKeyDto): string {
  return folder.kind === "group" ? String(folder.groupId) : "ungrouped";
}

/** The key of a folder header row. */
export function folderRowKey(folder: FolderKeyDto): string {
  return `f:${folderToken(folder)}`;
}

/** The key of one copy of a mod in a folder. A mod in two folders has two keys. */
export function modRowKey(folder: FolderKeyDto, modId: string): string {
  return `m:${folderToken(folder)}:${modId}`;
}

/** The folder a header entry stands for. */
export function folderKeyOfEntry(
  entry: Extract<GroupedEntryDto, { kind: "folder" }>,
): FolderKeyDto {
  return entry.folder.kind === "group"
    ? { kind: "group", groupId: entry.folder.id }
    : { kind: "ungrouped" };
}

/** One line of the Groups view with the keys and the parent link the keyboard model needs. */
export type TreeRow =
  | {
      readonly kind: "folder";
      readonly key: string;
      readonly folder: FolderKeyDto;
      /** Level in the tree, 1 for a top-level folder. */
      readonly level: number;
      /** The enclosing folder's key; `null` for a top-level folder. */
      readonly parentKey: string | null;
      readonly entry: Extract<GroupedEntryDto, { kind: "folder" }>;
    }
  | {
      readonly kind: "mod";
      readonly key: string;
      readonly modId: string;
      readonly folder: FolderKeyDto;
      /** Level in the tree: the folder's level plus one. */
      readonly level: number;
      /** The key of the folder this copy sits in. */
      readonly parentKey: string;
      readonly alsoIn: number;
    };

/**
 * Gives every entry of the backend's flat preorder list its stable key, its level and its parent.
 * Entries arrive folder first, then what is inside it, so a folder's parent is the nearest earlier
 * folder one level up. Nothing here decides which entries show: the backend already applied the
 * folds, the filters and the cut.
 */
export function treeRows(entries: readonly GroupedEntryDto[]): TreeRow[] {
  const lastFolderAtLevel = new Map<number, string>();
  return entries.map((entry): TreeRow => {
    if (entry.kind === "folder") {
      const folder = folderKeyOfEntry(entry);
      const key = folderRowKey(folder);
      lastFolderAtLevel.set(entry.depth, key);
      return {
        kind: "folder",
        key,
        folder,
        level: entry.depth,
        parentKey: lastFolderAtLevel.get(entry.depth - 1) ?? null,
        entry,
      };
    }
    return {
      kind: "mod",
      key: modRowKey(entry.folder, entry.modId),
      modId: entry.modId,
      folder: entry.folder,
      level: entry.depth,
      parentKey: folderRowKey(entry.folder),
      alsoIn: entry.alsoIn,
    };
  });
}

/** One row of a Shift range: its key, and the mod it names (`null` for a folder header). */
export interface RangeLineRow {
  readonly key: string;
  readonly modId: string | null;
}

/** The rows a Shift range runs over, in screen order. A header is in the line but selects nothing. */
export function rangeLineOf(rows: readonly TreeRow[]): RangeLineRow[] {
  return rows.map((row) => ({ key: row.key, modId: row.kind === "mod" ? row.modId : null }));
}
