import { computed, type Ref, ref, shallowRef, type WatchSource, watch } from "vue";

/** One page of rows plus the total count matching the current filter, before paging. */
export interface PagedResult<TRow> {
  items: readonly TRow[];
  total: number;
}

/**
 * Accumulates a paged list beyond one request's own page cap: as `offset`
 * grows (via {@link loadMore}), each landed page's rows are kept in a map
 * keyed by the `offset` it was requested at, then flattened back out in
 * offset order — never simply appended to a running array. That
 * distinction matters the moment the query `data` is read from gets
 * invalidated and refetches its *current* page (`offset` unchanged — e.g.
 * `session://changed` invalidating every query while the user has "show
 * more"d partway through a list): appending would land that refetched
 * page a second time right next to itself, since `offset` never moved.
 * Keying by offset makes a re-landed page for an already-seen offset
 * *replace* its own slot instead, so `rows` never grows a duplicate.
 *
 * Resets to the first page — clears every accumulated row, zeroes
 * `total`, sets `offset` back to `0` — whenever `resetSource` changes (a
 * different def/mod, or a filter change that should start the list
 * over). Shared by `useMergeFieldPage`, `ModChanges`, and `DefPage`'s own
 * effective-field list so this accumulate-by-offset logic lives in
 * exactly one place.
 *
 * `offset` is owned by the caller (it already needs to fold it into its
 * own filter object to build the query in the first place); this only
 * reads and, from {@link loadMore}, writes it.
 *
 * **`resetSource` must read raw filter refs directly, never a `computed`
 * that also depends on `offset`**: if a caller composes more
 * than one filter dimension into one request-building `computed` (say
 * `filter = computed(() => ({ onlyChanged, offset, limit }))`) and then
 * reads `filter.value.someField` inside its own `resetSource` getter,
 * `offset` becomes a *transitive* tracked dependency of that getter —
 * `filter.value`'s object reference changes on every `offset` bump too
 * (a computed returning a fresh object literal is never referentially
 * equal to its own last value), so Vue's function-source `watch` (which
 * compares by reference) fires on *every* {@link loadMore} call, not only
 * when `someField` itself actually changes. Symptom: `pages` gets wiped
 * by the reset watcher right after `loadMore` set it, and nothing
 * refetches to refill it, since the underlying request never actually
 * changed — `rows` silently goes to empty. Fix: derive a standalone
 * `computed` reading only the raw ref(s) that should trigger a reset
 * (e.g. `const onlyChanged = computed(() => kindFilter.value !==
 * "unchanged")`) and pass *that* as (part of) `resetSource`, never a
 * value read back out through the same computed that also folds in
 * `offset`.
 *
 * **The reset watcher re-lands `data.value` after clearing, rather than
 * leaving `pages` empty for the next `data` change to refill**. When a
 * component is reused rather than remounted (e.g.
 * `SuggestionPanel` re-pointed at a different finding), a caller
 * consuming a Pinia Colada query already has its own `watch` on `key`
 * that re-subscribes the query — Vue queues watcher callbacks in the
 * order they were *created*, and that Colada-internal watcher was set up
 * before this composable's own `resetSource` watcher on the very same
 * synchronous key swap. If the new key's data is already cached (a prior
 * visit within `staleTime`), `data.value` updates *synchronously* in that
 * same flush, so the `data` watcher below lands the new page *before*
 * `resetSource`'s watcher runs and wipes `pages` — erasing the page that
 * was just landed, with no further `data` change ever coming to refill
 * it (the query is already settled, so nothing refetches). The fix does
 * not depend on winning that ordering race: after clearing, immediately
 * re-land whatever `data.value` currently holds, so the reset is
 * self-consistent regardless of which watcher Vue happened to run first.
 */
export function usePagedRows<TRow>(
  offset: Ref<number>,
  data: Ref<PagedResult<TRow> | null | undefined>,
  resetSource: WatchSource<unknown>,
) {
  const pages = shallowRef(new Map<number, readonly TRow[]>());
  const total = ref(0);

  function landPage(page: PagedResult<TRow> | null | undefined): void {
    if (!page) {
      return;
    }
    total.value = page.total;
    const next = new Map(pages.value);
    next.set(offset.value, page.items);
    pages.value = next;
  }

  watch(resetSource, () => {
    offset.value = 0;
    pages.value = new Map();
    total.value = 0;
    landPage(data.value);
  });

  watch(data, landPage, { immediate: true });

  const rows = computed<TRow[]>(() =>
    [...pages.value.entries()].sort(([a], [b]) => a - b).flatMap(([, items]) => items),
  );

  const hasMore = computed(() => rows.value.length < total.value);

  /** Fetches the next page, appending its rows to {@link rows}. A no-op once every matching row is already loaded. */
  function loadMore(): void {
    if (hasMore.value) {
      offset.value = rows.value.length;
    }
  }

  return { rows, total, hasMore, loadMore };
}
