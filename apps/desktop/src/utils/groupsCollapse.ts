import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";

/** The two Mods panes; each remembers its own folded folders. */
export type PaneKind = "active" | "inactive";

/** A folded folder as stored: a group id, or `ungrouped` for "Ungrouped". */
export type StoredFold = number | "ungrouped";

/** The folded folders of both panes of one profile. */
export interface StoredFolds {
  readonly active: readonly StoredFold[];
  readonly inactive: readonly StoredFold[];
}

/** Every group (at most 1,000) plus "Ungrouped": the backend's bound on one request. */
export const MAX_FOLDS_PER_PANE = 1_001;

/** A stored value larger than this is not parsed at all: it can only be damaged or foreign. */
const MAX_STORED_CHARS = 64 * 1024;

const STORAGE_PREFIX = "rimmerge.groupsView.collapsed.v1:";

/** The `localStorage` key for one profile's folds. */
export function foldsStorageKey(profileDir: string): string {
  return `${STORAGE_PREFIX}${profileDir}`;
}

/** Nothing folded: the default, and what any unreadable stored value means. */
export const NO_FOLDS: StoredFolds = { active: [], inactive: [] };

function parsePaneFolds(raw: unknown): StoredFold[] | null {
  if (!Array.isArray(raw) || raw.length > MAX_FOLDS_PER_PANE) {
    return null;
  }
  const folds = new Set<StoredFold>();
  for (const item of raw) {
    if (
      item === "ungrouped" ||
      (typeof item === "number" && Number.isSafeInteger(item) && item > 0)
    ) {
      folds.add(item);
      continue;
    }
    return null;
  }
  return [...folds];
}

/**
 * Parses a stored value into folds, or {@link NO_FOLDS} when it is not exactly the shape
 * {@link serializeFolds} writes: too large, not JSON, a pane that is not a bounded array of positive
 * integers and `"ungrouped"`. A damaged value never half-applies; the user just sees everything open.
 */
export function parseFolds(stored: string | null): StoredFolds {
  if (stored === null || stored.length > MAX_STORED_CHARS) {
    return NO_FOLDS;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(stored);
  } catch {
    return NO_FOLDS;
  }
  if (typeof parsed !== "object" || parsed === null) {
    return NO_FOLDS;
  }
  const active = parsePaneFolds((parsed as Record<string, unknown>)["active"]);
  const inactive = parsePaneFolds((parsed as Record<string, unknown>)["inactive"]);
  return active === null || inactive === null ? NO_FOLDS : { active, inactive };
}

/** The stored form of both panes' folds. */
export function serializeFolds(folds: StoredFolds): string {
  return JSON.stringify({ active: folds.active, inactive: folds.inactive });
}

/** A fold as the request key the backend takes. */
export function foldToKey(fold: StoredFold): FolderKeyDto {
  return fold === "ungrouped" ? { kind: "ungrouped" } : { kind: "group", groupId: fold };
}

/** A request key as a stored fold. */
export function keyToFold(key: FolderKeyDto): StoredFold {
  return key.kind === "ungrouped" ? "ungrouped" : key.groupId;
}

/**
 * The folds in a stable order (groups by id, "Ungrouped" last), so the same set always makes
 * the same query and the same cache entry.
 */
export function sortedFolds(folds: Iterable<StoredFold>): StoredFold[] {
  return [...folds].sort((left, right) => {
    if (left === right) {
      return 0;
    }
    if (left === "ungrouped") {
      return 1;
    }
    if (right === "ungrouped") {
      return -1;
    }
    return left - right;
  });
}
