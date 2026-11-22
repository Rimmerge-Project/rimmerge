import { useQueryCache } from "@pinia/colada";
import { storeToRefs } from "pinia";
import { computed, type MaybeRefOrGetter, toValue } from "vue";

import {
  useInactiveModsGroupedQuery,
  useInactiveModsQuery,
  useWorkingModsGroupedQuery,
  useWorkingModsQuery,
} from "@/queries/activeSet";
import { queryKeys } from "@/queries/keys";
import { usePreferencesStore } from "@/stores/preferences";
import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import type { GroupedListQueryDto } from "@/types/generated/GroupedListQueryDto";
import type { GroupedModListDto } from "@/types/generated/GroupedModListDto";
import type { ModListDto } from "@/types/generated/ModListDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";
import type { PaneKind } from "@/utils/groupsCollapse";
import type { PaneView } from "@/utils/panePrefs";

/** Bound on how many newer fetches {@link usePaneList}'s `refreshList` waits behind. */
const MAX_SUPERSEDING_FETCHES = 10;

/**
 * Each pane's remembered view (a display preference kept by the preferences store, like the sort,
 * so it survives a restart and a project switch). Whether Groups can be shown right now is the
 * caller's question (`useGroupsView`), not part of the preference.
 */
export function usePaneViews() {
  const preferences = usePreferencesStore();
  const { panes } = storeToRefs(preferences);

  const activeView = computed<PaneView>({
    get: () => panes.value.active.view,
    set: (view) => preferences.setPaneView("active", view),
  });
  const inactiveView = computed<PaneView>({
    get: () => panes.value.inactive.view,
    set: (view) => preferences.setPaneView("inactive", view),
  });

  return { activeView, inactiveView };
}

/**
 * One pane's rows from whichever query its view runs, so the page holds one pair of values per
 * pane and not a pair of queries. Only the current view's query is enabled: a pane never fetches
 * both shapes. The List query is the pane's own query; the Groups one adds the folded folders.
 * `list` is the same `ModListDto` shape in both views (the grouped answer is a superset), so every
 * reader of the rows (selection, the info panel, drag sets) is the same code in both.
 *
 * `isPlaceholder` is set while the rows are the previous key's answer (the Groups query keeps it
 * on screen while a new key loads): they are not yet the answer to the current query, so a reader
 * that takes them for the full list (the selection prune) must wait for it to turn false.
 */
export function usePaneList({
  kind,
  view,
  query,
  collapsed,
}: {
  kind: PaneKind;
  view: MaybeRefOrGetter<PaneView>;
  /** The pane's own query (filter, scope, sort). */
  query: MaybeRefOrGetter<ModListQueryDto>;
  /** The folders folded shut in this pane. */
  collapsed: MaybeRefOrGetter<readonly FolderKeyDto[]>;
}) {
  const isGrouped = computed(() => toValue(view) === "groups");
  const groupedQuery = computed<GroupedListQueryDto>(() => ({
    list: toValue(query),
    collapsed: [...toValue(collapsed)],
  }));
  const isListEnabled = computed(() => !isGrouped.value);

  const listResult =
    kind === "active"
      ? useWorkingModsQuery(query, isListEnabled)
      : useInactiveModsQuery(query, isListEnabled);
  const groupedResult =
    kind === "active"
      ? useWorkingModsGroupedQuery(groupedQuery, isGrouped)
      : useInactiveModsGroupedQuery(groupedQuery, isGrouped);

  const grouped = computed<GroupedModListDto | undefined>(() =>
    isGrouped.value ? groupedResult.data.value : undefined,
  );
  const data = computed<ModListDto | undefined>(() =>
    isGrouped.value ? groupedResult.data.value : listResult.data.value,
  );
  const isPending = computed(() =>
    isGrouped.value ? groupedResult.isPending.value : listResult.isPending.value,
  );
  const isPlaceholder = computed(() => isGrouped.value && groupedResult.isPlaceholderData.value);
  const error = computed<Error | null>(() =>
    isGrouped.value ? groupedResult.error.value : listResult.error.value,
  );

  const queryCache = useQueryCache();

  /** The call now fetching this pane's List entry, if any. */
  function pendingListCall() {
    const key =
      kind === "active"
        ? queryKeys.workingMods(toValue(query))
        : queryKeys.inactiveMods(toValue(query));
    return queryCache.get(key)?.pending ?? null;
  }

  /**
   * The List query's answer once it is current: Colada's `refresh` returns the cached rows when
   * they are fresh and otherwise the fetch (an entry nobody observes is only marked stale by an
   * invalidation, and its old rows show at once on a key switch). Colada (1.4.5) aborts a fetch
   * when a newer one starts on the same entry (an invalidation) and resolves the aborted call with
   * the entry's OLD state, still `success`; so after our own refresh this waits for every call
   * still pending on the entry (bounded) and reads the entry, as `useGameLaunch`'s
   * `readFreshStatus` does. `null` unless the entry ends in `success`: an error state keeps the
   * previous rows, which are not the answer.
   */
  async function refreshList(): Promise<ModListDto | null> {
    await listResult.refresh();
    for (let attempt = 0; attempt < MAX_SUPERSEDING_FETCHES; attempt++) {
      const newer = pendingListCall();
      if (newer === null) {
        break;
      }
      await newer.refreshCall.catch(() => undefined);
    }
    // Still superseded after every retry: the held rows may be the old answer, so act on none.
    if (pendingListCall() !== null) {
      return null;
    }
    const state = listResult.state.value;
    return state.status === "success" ? state.data : null;
  }

  return { data, grouped, isPending, isPlaceholder, error, refreshList };
}
