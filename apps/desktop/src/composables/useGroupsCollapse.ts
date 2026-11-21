import { computed, type MaybeRefOrGetter, ref, toValue, watch } from "vue";

import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import {
  foldsStorageKey,
  foldToKey,
  keyToFold,
  NO_FOLDS,
  type PaneKind,
  parseFolds,
  type StoredFold,
  type StoredFolds,
  serializeFolds,
  sortedFolds,
} from "@/utils/groupsCollapse";

function readFolds(profileDir: string | null): StoredFolds {
  if (profileDir === null) {
    return NO_FOLDS;
  }
  try {
    return parseFolds(localStorage.getItem(foldsStorageKey(profileDir)));
  } catch {
    // Storage blocked or unavailable: nothing is remembered, everything opens.
    return NO_FOLDS;
  }
}

function writeFolds(profileDir: string | null, folds: StoredFolds): void {
  if (profileDir === null) {
    return;
  }
  try {
    localStorage.setItem(foldsStorageKey(profileDir), serializeFolds(folds));
  } catch {
    // Not remembered after a restart; the in-memory folds still apply.
  }
}

/**
 * Which Groups view folders are folded shut, per pane and per profile, kept in `localStorage`
 * (a per-machine viewing preference, not a fact about the library). The page only holds the set
 * and sends it with the list request; the backend decides which rows a fold hides. A stored value
 * that does not parse is the same as none, and an id the organization no longer has is dropped by
 * {@link pruneTo}.
 */
export function useGroupsCollapse(profileDir: MaybeRefOrGetter<string | null>) {
  const folds = ref<StoredFolds>(readFolds(toValue(profileDir)));

  // Another project has its own folds.
  watch(
    () => toValue(profileDir),
    (next) => {
      folds.value = readFolds(next);
    },
  );

  function change(kind: PaneKind, next: Iterable<StoredFold>): void {
    const updated: StoredFolds = { ...folds.value, [kind]: sortedFolds(new Set(next)) };
    folds.value = updated;
    writeFolds(toValue(profileDir), updated);
  }

  /** The pane's folded folders, as the keys a list request carries. */
  function keysFor(kind: PaneKind): FolderKeyDto[] {
    return folds.value[kind].map(foldToKey);
  }

  /** Folds or opens one folder. */
  function setCollapsed(kind: PaneKind, key: FolderKeyDto, isCollapsedNow: boolean): void {
    const fold = keyToFold(key);
    const without = folds.value[kind].filter((existing) => existing !== fold);
    change(kind, isCollapsedNow ? [...without, fold] : without);
  }

  /** Folds every known group and "Not in a group". */
  function collapseAll(kind: PaneKind, groupIds: Iterable<number>): void {
    change(kind, [...groupIds, "ungrouped"]);
  }

  function expandAll(kind: PaneKind): void {
    change(kind, []);
  }

  /** Drops every stored group id that is not a known group, so deleted groups leave the store. */
  function pruneTo(groupIds: Iterable<number>): void {
    const known = new Set(groupIds);
    for (const kind of ["active", "inactive"] as const) {
      const kept = folds.value[kind].filter((fold) => fold === "ungrouped" || known.has(fold));
      if (kept.length !== folds.value[kind].length) {
        change(kind, kept);
      }
    }
  }

  return {
    activeKeys: computed(() => keysFor("active")),
    inactiveKeys: computed(() => keysFor("inactive")),
    setCollapsed,
    collapseAll,
    expandAll,
    pruneTo,
  };
}
