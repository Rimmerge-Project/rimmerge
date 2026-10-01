import { useMutation, useQuery, useQueryCache } from "@pinia/colada";
import { type MaybeRefOrGetter, toValue } from "vue";

import { queryKeys } from "@/queries/keys";
import { decide, getFinding, listFindings, revertDecision } from "@/services/ipc";
import type { FindingKey } from "@/types/brands";
import type { DecideRequestDto } from "@/types/generated/DecideRequestDto";
import type { FindingFilterDto } from "@/types/generated/FindingFilterDto";

/**
 * Pages the selected order's ledger. `placeholderData` keeps the
 * previous page's rows on screen while a newly-picked filter (a kind
 * chip, a status chip, a search edit) fetches its own not-yet-cached
 * key — without it, `useQuery` reports `isPending` (there is genuinely no
 * data for a key it has never fetched before) for that first moment on
 * every new filter combination, and `InboxPage`'s `isPending` branch
 * would unmount `FindingList` and remount it once the page lands, losing
 * any uncontrolled DOM state (the "Filter by kind" disclosure, before it
 * moved into `inbox.kindsOpen`, closed exactly this way).
 */
export function useFindingsQuery(filter: MaybeRefOrGetter<FindingFilterDto>) {
  return useQuery({
    key: () => queryKeys.findings(toValue(filter)),
    query: () => listFindings(toValue(filter)),
    placeholderData: (previousData) => previousData,
  });
}

/** One finding's full detail. Disabled while `key` is `null`. */
export function useFindingQuery(key: MaybeRefOrGetter<FindingKey | null>) {
  return useQuery({
    key: () => queryKeys.finding(toValue(key) ?? ""),
    query: () => getFinding(toValue(key) as FindingKey),
    enabled: () => toValue(key) !== null,
  });
}

/**
 * Every query, invalidated after a decision. In production the backend's
 * own `session://changed` event (see `useSessionEvents`) would refetch
 * everything anyway, making this a duplicate refetch there — kept
 * unfiltered regardless so the Vitest/Playwright mock tier, which never
 * emits that event, still ends up with the same fully-fresh cache a real
 * run would: deciding a finding can move the dashboard's ledger stats and
 * a mod's own status, not just the ledger itself. Returned (not
 * fire-and-forget) so callers — notably the inbox's own cursor-advance
 * logic, which needs to know whether the decided finding is still at the
 * cursor once the refiltered list actually lands — can await the refetch
 * settling, not just the invalidation being requested.
 */
function invalidateAfterDecision(queryCache: ReturnType<typeof useQueryCache>): Promise<unknown> {
  return queryCache.invalidateQueries();
}

/** Records a decision on a finding, persisting it before returning. */
export function useDecideMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: DecideRequestDto) => decide(request),
    onSuccess: () => invalidateAfterDecision(queryCache),
  });
}

/** Removes a decision on a finding, persisting the change before returning. */
export function useRevertDecisionMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (key: FindingKey) => revertDecision(key),
    onSuccess: () => invalidateAfterDecision(queryCache),
  });
}
