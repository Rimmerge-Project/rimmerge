import { computed } from "vue";

import { useOrderQuery } from "@/queries/order";
import { useSessionStore } from "@/stores/session";

/**
 * Whether the last imported log's own logged load order can be compared
 * against the currently selected order, and if so whether it differs
 * — shared by `StartupPage.vue`'s
 * own warning and `ApplyDialog.vue`'s predicted-vs-observed panel, both
 * of which need the identical three-state check: no log imported at all
 * (nothing to say — `session.gameLogSummary` is `null`), a log imported
 * but carrying no load block at all (neither `Initializing new game with
 * mods:` nor `Loading game from file ... with mods:`; can't tell —
 * `noOrderBlock`, a real log shape), or a log whose own order — its last
 * load event's — can genuinely be compared (`differs`).
 *
 * **`pending`**: the selected
 * order's own `list_order` query starts `undefined` before it resolves.
 * Comparing against `[]` in that window would read as a false "differs"
 * for any log with a non-empty logged order — `differs`/`noOrderBlock`
 * both stay `false` while `pending` is `true`, rather than guessing from
 * an order that hasn't loaded yet.
 */
export function useLoggedOrderCheck() {
  const session = useSessionStore();
  const { data: orderRows } = useOrderQuery(() => session.selected);

  const pending = computed(() => orderRows.value === undefined);

  /** `null` while pending or with nothing to compare (see the store's own `loggedOrderDiffersFrom`). */
  const comparison = computed<boolean | null>(() => {
    if (pending.value) {
      return null;
    }
    const activeOrderIds = (orderRows.value ?? []).map((row) => row.modId);
    return session.loggedOrderDiffersFrom(activeOrderIds);
  });

  const differs = computed(() => comparison.value === true);

  /** A log has been imported, but it carries no order block to compare at all. */
  const noOrderBlock = computed(
    () => !pending.value && session.gameLogSummary !== null && comparison.value === null,
  );

  return { pending, differs, noOrderBlock };
}
