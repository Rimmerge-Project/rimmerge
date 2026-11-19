import { computed, ref, watch } from "vue";

import { useSessionStore } from "@/stores/session";
import type { ListSortDto } from "@/types/generated/ListSortDto";
import { ACTIVE_DEFAULT_SORT, INACTIVE_DEFAULT_SORT, isLoadOrderSort } from "@/utils/modListSort";

/**
 * The Mods page's display sort, one per list. Page-local: it lives while the
 * page stays mounted, resets when another project is loaded, and is never
 * written anywhere. Sorting only changes the query sent to the backend; it
 * never edits the load order or any pending change.
 *
 * `isActiveInLoadOrder` is the fact the reordering controls must gate on:
 * reordering the Active list is meaningful only while it shows the load order.
 */
export function useModListSort() {
  const activeSort = ref<ListSortDto>(ACTIVE_DEFAULT_SORT);
  const inactiveSort = ref<ListSortDto>(INACTIVE_DEFAULT_SORT);
  const isActiveInLoadOrder = computed(() => isLoadOrderSort(activeSort.value));

  function reset(): void {
    activeSort.value = ACTIVE_DEFAULT_SORT;
    inactiveSort.value = INACTIVE_DEFAULT_SORT;
  }

  // A project switch (a different profile) starts from the defaults. A rescan
  // of the same project keeps the user's sort.
  const session = useSessionStore();
  watch(() => session.paths?.profileDir ?? null, reset);

  return { activeSort, inactiveSort, isActiveInLoadOrder, reset };
}
