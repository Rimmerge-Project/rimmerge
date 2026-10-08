import type { useQueryCache } from "@pinia/colada";

type QueryCache = ReturnType<typeof useQueryCache>;
type InvalidationFilters = Parameters<QueryCache["invalidateQueries"]>[0];

/**
 * Marks the matching queries stale and refetches the active ones, without waiting for the
 * refetches. Use it where the caller must not block on them (a mutation's `onSuccess`, an event
 * handler); use {@link awaitInvalidation} where the refetch has to land first.
 *
 * Pinia Colada's `invalidateQueries` returns a `Promise.all` over the refetches, and each refetch
 * rethrows its query's error, so a bare `void queryCache.invalidateQueries()` turns any failing
 * query into an unhandled rejection. The rejection is swallowed here on purpose: a failed query
 * already carries the error in its own `status`/`error` state, which is where the UI reads it, so
 * there is nothing further to report.
 */
export function startInvalidation(queryCache: QueryCache, filters?: InvalidationFilters): void {
  queryCache.invalidateQueries(filters).catch(() => undefined);
}

/**
 * Marks the matching queries stale, refetches the active ones, and resolves once every refetch
 * has settled. It never rejects: no filters means every query, several filters run side by side.
 * Use it where the refetch must land before the caller goes on (a mutation's `onSuccess`).
 *
 * @remarks An empty `filters` list means every query, so a possibly empty array must not be
 * spread into it unless that is intended.
 *
 * Pinia Colada awaits `onSuccess` inside `mutate`, so a raw awaited or returned
 * `invalidateQueries` would fail a mutation whose backend write already committed whenever any
 * unrelated query's refetch fails (status `error`, the global `onError` toast for the wrong
 * operation, a rejected `mutateAsync`). A failed query already carries its own error state.
 */
export async function awaitInvalidation(
  queryCache: QueryCache,
  ...filters: InvalidationFilters[]
): Promise<void> {
  const requests = filters.length === 0 ? [undefined] : filters;
  await Promise.allSettled(requests.map((request) => queryCache.invalidateQueries(request)));
}
