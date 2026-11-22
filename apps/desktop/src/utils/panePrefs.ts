import type { ListSortDto } from "@/types/generated/ListSortDto";
import type { ListSortKeyDto } from "@/types/generated/ListSortKeyDto";
import type { SortDirectionDto } from "@/types/generated/SortDirectionDto";
import type { PaneKind } from "@/utils/groupsCollapse";
import {
  ACTIVE_DEFAULT_SORT,
  ACTIVE_SORT_KEYS,
  INACTIVE_DEFAULT_SORT,
  INACTIVE_SORT_KEYS,
} from "@/utils/modListSort";

/** How a Mods pane lays its rows out: one flat list, or folders by group. */
export type PaneView = "list" | "groups";

/** What one Mods pane remembers across restarts: its sort and its view. */
export interface PanePref {
  readonly sort: ListSortDto;
  readonly view: PaneView;
}

/** Both panes' remembered display preferences. */
export type PanePrefs = Readonly<Record<PaneKind, PanePref>>;

/** What a pane shows when nothing usable is stored: Active in load order, Inactive by name, both List. */
export const DEFAULT_PANE_PREFS: PanePrefs = {
  active: { sort: ACTIVE_DEFAULT_SORT, view: "list" },
  inactive: { sort: INACTIVE_DEFAULT_SORT, view: "list" },
};

/** A stored value larger than this can only be damaged or foreign, so it is not parsed. */
const MAX_STORED_CHARS = 4 * 1024;

const SORT_KEYS: Readonly<Record<PaneKind, readonly ListSortKeyDto[]>> = {
  active: ACTIVE_SORT_KEYS,
  inactive: INACTIVE_SORT_KEYS,
};
const DIRECTIONS: readonly SortDirectionDto[] = ["ascending", "descending"];
const VIEWS: readonly PaneView[] = ["list", "groups"];

/** The `localStorage` key of the panes' preferences (one value for both panes, global like the label mode). */
export const PANE_PREFS_STORAGE_KEY = "rimmerge.modsPanes.v1";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** One pane's stored entry, or `null` unless every field is in its closed set for that pane. */
function parsePane(kind: PaneKind, raw: unknown): PanePref | null {
  if (!isRecord(raw) || !isRecord(raw["sort"])) {
    return null;
  }
  const { key, direction } = raw["sort"];
  const view = raw["view"];
  const sortKey = SORT_KEYS[kind].find((known) => known === key);
  const sortDirection = DIRECTIONS.find((known) => known === direction);
  const paneView = VIEWS.find((known) => known === view);
  if (sortKey === undefined || sortDirection === undefined || paneView === undefined) {
    return null;
  }
  return { sort: { key: sortKey, direction: sortDirection }, view: paneView };
}

/**
 * Parses the stored panes' preferences once, at read. Each pane stands alone: a pane whose entry is
 * missing, has a key this pane does not offer (a removed one, or load order on Inactive), a
 * direction or view outside its set, or is not an object gets its own default, never half of a
 * stored value. Anything that is not JSON, or is oversized, is both defaults.
 */
export function parsePanePrefs(stored: string | null): PanePrefs {
  if (stored === null || stored.length > MAX_STORED_CHARS) {
    return DEFAULT_PANE_PREFS;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(stored);
  } catch {
    return DEFAULT_PANE_PREFS;
  }
  if (!isRecord(parsed)) {
    return DEFAULT_PANE_PREFS;
  }
  return {
    active: parsePane("active", parsed["active"]) ?? DEFAULT_PANE_PREFS.active,
    inactive: parsePane("inactive", parsed["inactive"]) ?? DEFAULT_PANE_PREFS.inactive,
  };
}

/** The value {@link parsePanePrefs} reads back. */
export function serializePanePrefs(prefs: PanePrefs): string {
  return JSON.stringify(prefs);
}
