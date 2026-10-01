import { useMutation, useQuery, useQueryCache } from "@pinia/colada";
import { type MaybeRefOrGetter, toValue } from "vue";

import { queryKeys } from "@/queries/keys";
import { apply, getApplyPreflight } from "@/services/ipc";
import type { ApplyRequestDto } from "@/types/generated/ApplyRequestDto";
import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";

/**
 * The hard problems in `source`'s order — what `apply` would write.
 * `enabled` keeps the query off while the Apply dialog is closed;
 * `session://changed` invalidates it like every other query, so a decision
 * made elsewhere updates the list.
 */
export function useApplyPreflightQuery(
  source: MaybeRefOrGetter<OrderSourceDto>,
  enabled: MaybeRefOrGetter<boolean> = true,
) {
  return useQuery({
    key: () => queryKeys.applyPreflight(toValue(source)),
    query: () => getApplyPreflight(toValue(source)),
    enabled: () => toValue(enabled),
  });
}

/**
 * Persists decisions/rules and, when asked, writes `ModsConfig.xml`.
 * Invalidates every query on success — the same blast radius
 * `useSessionEvents` gives `session://changed` for every other mutating
 * command, applied directly here too so the dialog's own caller doesn't
 * have to wait on the event round trip to see fresh data.
 */
export function useApplyMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: ApplyRequestDto) => apply(request),
    onSuccess: () => queryCache.invalidateQueries(),
  });
}
