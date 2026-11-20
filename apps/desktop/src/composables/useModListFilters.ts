import { refDebounced } from "@vueuse/core";
import { computed, type MaybeRefOrGetter, type Ref, ref, toValue } from "vue";

import type { ListSortDto } from "@/types/generated/ListSortDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";
import type { SourceDto } from "@/types/generated/SourceDto";
import { type GroupScopeChoice, groupFilterOf } from "@/utils/organization";

const SEARCH_DEBOUNCE_MS = 300;

/**
 * One Mods list's search/tag/source filter state and the list query derived from it. Each list
 * has its own, so searching one never changes the other; the group scope is the page's, shared by
 * both lists, and the search runs inside it. `isFiltered` says whether the list is showing less
 * than everything, which is what lets a selection tell "hidden by a filter" from "gone from the
 * list". The query also carries the list's display sort; a sort is not a filter, so it never
 * counts toward `isFiltered`.
 */
export function useModListFilters({
  sort,
  group,
}: {
  sort: Ref<ListSortDto>;
  /** The page's group scope: a filter on this list, never a reorder. */
  group: MaybeRefOrGetter<GroupScopeChoice>;
}) {
  // The input stays responsive on every keystroke; only the value that
  // actually drives the query is debounced, so typing a search term
  // doesn't fire a list query per character.
  const search = ref("");
  const debouncedSearch = refDebounced(search, SEARCH_DEBOUNCE_MS);
  const source = ref<SourceDto | null>(null);
  const tag = ref("");

  // The group is left out of the query while there is no scope, so an unscoped query is the same
  // object (and cache entry) it was before groups existed.
  const query = computed<ModListQueryDto>(() => {
    const groupFilter = groupFilterOf(toValue(group));
    return {
      search: debouncedSearch.value.length > 0 ? debouncedSearch.value : null,
      tag: tag.value.length > 0 ? tag.value : null,
      source: source.value,
      sort: sort.value,
      ...(groupFilter === null ? {} : { group: groupFilter }),
    };
  });

  const isFiltered = computed(
    () =>
      query.value.search !== null ||
      query.value.tag !== null ||
      query.value.source !== null ||
      query.value.group !== undefined,
  );

  return { search, source, tag, query, isFiltered };
}
