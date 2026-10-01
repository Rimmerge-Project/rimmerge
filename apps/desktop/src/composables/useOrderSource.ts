import { useMutation, useQueryCache } from "@pinia/colada";
import { computed } from "vue";

import { selectOrder } from "@/services/ipc";
import { useSessionStore } from "@/stores/session";
import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";

/**
 * The order-source switch shared by the dashboard and load-order page.
 * Switching sources is a real backend round-trip (`select_order` moves
 * which order commands like `list_findings`/`get_mod` read from), not
 * just a local toggle, so this wraps it in a mutation and only updates
 * {@link useSessionStore}'s `selected` once the backend confirms it.
 */
export function useOrderSource() {
  const session = useSessionStore();
  const queryCache = useQueryCache();

  const { mutate, isLoading } = useMutation({
    mutation: (source: OrderSourceDto) => selectOrder(source),
    onSuccess: (_stats, source) => {
      session.setSelected(source);
      // Every order-dependent query (dashboard's `selected`/`movedMods`,
      // findings, mod detail) is stale the moment the backend's own
      // selection moves.
      void queryCache.invalidateQueries();
    },
  });

  /** Switches the selected order, a no-op when `source` is already selected. */
  function select(source: OrderSourceDto): void {
    if (source === session.selected) {
      return;
    }
    mutate(source);
  }

  return {
    selected: computed(() => session.selected),
    select,
    isSwitching: isLoading,
  };
}
