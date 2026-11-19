import { refDebounced } from "@vueuse/core";
import { computed, type Ref, ref } from "vue";

import type { ListSortDto } from "@/types/generated/ListSortDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";
import type { SourceDto } from "@/types/generated/SourceDto";

const SEARCH_DEBOUNCE_MS = 300;

/**
 * One Mods list's search/tag/source filter state and the list query derived from it. Each list
 * has its own, so searching one never changes the other. The Inactive list has no tag filter
 * (`hasTag: false`): the backend ignores a tag there, so the query never carries one.
 * `isFiltered` says whether the list is showing less than everything, which is what lets a
 * selection tell "hidden by a filter" from "gone from the list". The query also carries the
 * list's display sort; a sort is not a filter, so it never counts toward `isFiltered`.
 */
export function useModListFilters({
  sort,
  hasTag,
}: {
  sort: Ref<ListSortDto>;
  /** Whether this list offers a tag filter (only the Active list's mods carry tags). */
  hasTag: boolean;
}) {
  // The input stays responsive on every keystroke; only the value that
  // actually drives the query is debounced, so typing a search term
  // doesn't fire a list query per character.
  const search = ref("");
  const debouncedSearch = refDebounced(search, SEARCH_DEBOUNCE_MS);
  const source = ref<SourceDto | null>(null);
  const tag = ref("");

  const query = computed<ModListQueryDto>(() => ({
    search: debouncedSearch.value.length > 0 ? debouncedSearch.value : null,
    tag: hasTag && tag.value.length > 0 ? tag.value : null,
    source: source.value,
    sort: sort.value,
  }));

  const isFiltered = computed(
    () => query.value.search !== null || query.value.tag !== null || query.value.source !== null,
  );

  return { search, source, tag, query, isFiltered };
}
