import { refDebounced } from "@vueuse/core";
import { computed, ref } from "vue";

import type { ModFilterDto } from "@/types/generated/ModFilterDto";
import type { SourceDto } from "@/types/generated/SourceDto";

/** The most rows `list_mods`/`list_inactive_mods` return per page (the backend's own `MAX_PAGE_SIZE`). */
const MOD_PAGE_LIMIT = 200;
const SEARCH_DEBOUNCE_MS = 300;

/**
 * The Mods page's search/tag/source filter state and the two query
 * filters derived from it. The Inactive filter never carries a tag: an
 * inactive mod has none, and the tag input is hidden on that tab.
 */
export function useModListFilters() {
  // The input stays responsive on every keystroke; only the value that
  // actually drives the query is debounced, so typing a search term
  // doesn't fire a `list_mods`/`list_inactive_mods` query per character.
  const search = ref("");
  const debouncedSearch = refDebounced(search, SEARCH_DEBOUNCE_MS);
  const source = ref<SourceDto | null>(null);
  const tag = ref("");

  const searchTerm = computed(() =>
    debouncedSearch.value.length > 0 ? debouncedSearch.value : null,
  );

  const activeFilter = computed<ModFilterDto>(() => ({
    search: searchTerm.value,
    tag: tag.value.length > 0 ? tag.value : null,
    source: source.value,
    offset: 0,
    limit: MOD_PAGE_LIMIT,
  }));

  const inactiveFilter = computed<ModFilterDto>(() => ({
    search: searchTerm.value,
    tag: null,
    source: source.value,
    offset: 0,
    limit: MOD_PAGE_LIMIT,
  }));

  return { search, source, tag, activeFilter, inactiveFilter };
}
