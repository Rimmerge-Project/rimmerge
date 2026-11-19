import { refDebounced } from "@vueuse/core";
import { computed, type Ref, ref } from "vue";

import type { ListSortDto } from "@/types/generated/ListSortDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";
import type { SourceDto } from "@/types/generated/SourceDto";

const SEARCH_DEBOUNCE_MS = 300;

/**
 * The Mods page's search/tag/source filter state and the two list queries
 * derived from it. The Inactive query never carries a tag: the backend
 * ignores it there, and the tag input is hidden on that tab. `isFiltered`
 * says whether the lists are showing less than everything, which is what
 * lets a selection tell "hidden by a filter" from "gone from the list".
 * Each query also carries its list's display sort; a sort is not a filter, so
 * it never counts toward `isFiltered`.
 */
export function useModListFilters(sorts: { active: Ref<ListSortDto>; inactive: Ref<ListSortDto> }) {
  // The input stays responsive on every keystroke; only the value that
  // actually drives the query is debounced, so typing a search term
  // doesn't fire a list query per character.
  const search = ref("");
  const debouncedSearch = refDebounced(search, SEARCH_DEBOUNCE_MS);
  const source = ref<SourceDto | null>(null);
  const tag = ref("");

  const searchTerm = computed(() =>
    debouncedSearch.value.length > 0 ? debouncedSearch.value : null,
  );

  const activeQuery = computed<ModListQueryDto>(() => ({
    search: searchTerm.value,
    tag: tag.value.length > 0 ? tag.value : null,
    source: source.value,
    sort: sorts.active.value,
  }));

  const inactiveQuery = computed<ModListQueryDto>(() => ({
    search: searchTerm.value,
    tag: null,
    source: source.value,
    sort: sorts.inactive.value,
  }));

  const isActiveFiltered = computed(
    () =>
      activeQuery.value.search !== null ||
      activeQuery.value.tag !== null ||
      activeQuery.value.source !== null,
  );
  const isInactiveFiltered = computed(
    () => inactiveQuery.value.search !== null || inactiveQuery.value.source !== null,
  );

  return { search, source, tag, activeQuery, inactiveQuery, isActiveFiltered, isInactiveFiltered };
}
