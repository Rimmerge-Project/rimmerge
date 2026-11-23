import { storeToRefs } from "pinia";
import { computed } from "vue";

import { usePreferencesStore } from "@/stores/preferences";
import type { ListSortDto } from "@/types/generated/ListSortDto";

/**
 * The Mods page's display sort, one per list. It is a remembered display preference (the
 * preferences store keeps it in `localStorage`, global like the id/name toggle), so it survives a
 * restart and a project switch alike. Sorting only changes the query sent to the backend; it never
 * edits the load order or any pending change.
 *
 * Whether the Active list may be reordered is not decided here: the backend answers it on every
 * list (`ModListDto.reorderable`) from the sort the query carries, so a restored sort is judged
 * exactly like a chosen one.
 */
export function useModListSort() {
  const preferences = usePreferencesStore();
  const { panes } = storeToRefs(preferences);

  const activeSort = computed<ListSortDto>({
    get: () => panes.value.active.sort,
    set: (sort) => preferences.setPaneSort("active", sort),
  });
  const inactiveSort = computed<ListSortDto>({
    get: () => panes.value.inactive.sort,
    set: (sort) => preferences.setPaneSort("inactive", sort),
  });

  return { activeSort, inactiveSort };
}
