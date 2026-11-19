import { ref, watch } from "vue";

import { useSessionStore } from "@/stores/session";
import type { ListSortDto } from "@/types/generated/ListSortDto";
import { ACTIVE_DEFAULT_SORT, INACTIVE_DEFAULT_SORT } from "@/utils/modListSort";

/**
 * The Mods page's display sort, one per list. Page-local: it lives while the
 * page stays mounted, resets when another project is loaded, and is never
 * written anywhere. Sorting only changes the query sent to the backend; it
 * never edits the load order or any pending change.
 *
 * Whether the Active list may be reordered is not decided here: the backend
 * answers it on every list (`ModListDto.reorderable`), so the rule has one home.
 */
export function useModListSort() {
  const activeSort = ref<ListSortDto>(ACTIVE_DEFAULT_SORT);
  const inactiveSort = ref<ListSortDto>(INACTIVE_DEFAULT_SORT);

  function reset(): void {
    activeSort.value = ACTIVE_DEFAULT_SORT;
    inactiveSort.value = INACTIVE_DEFAULT_SORT;
  }

  // A project switch (a different profile) starts from the defaults. A rescan
  // of the same project keeps the user's sort.
  const session = useSessionStore();
  watch(() => session.paths?.profileDir ?? null, reset);

  return { activeSort, inactiveSort, reset };
}
