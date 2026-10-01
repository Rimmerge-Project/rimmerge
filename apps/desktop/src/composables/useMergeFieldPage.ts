import { computed, type MaybeRefOrGetter, ref, toValue } from "vue";

import { usePagedRows } from "@/composables/usePagedRows";
import { useMergePreviewQuery } from "@/queries/merge";
import type { FindingKey } from "@/types/brands";
import type { MergeFieldFilterDto } from "@/types/generated/MergeFieldFilterDto";

/**
 * Matches `rim_session::MAX_PAGE_SIZE` — the server clamps `filter.limit`
 * to this regardless of what's requested, so a def with more matching
 * rows than one page needs more than one request to see in full.
 */
const PAGE_SIZE = 200;

/**
 * Accumulates a `get_merge_preview` field list beyond the server's
 * per-request page cap: fetches `onlyConflicts`-filtered pages of
 * {@link PAGE_SIZE}, accumulating each successive page (by growing
 * `offset`) via {@link loadMore} — see {@link usePagedRows} for how pages
 * are kept (by offset, not a plain append) so a query invalidation that
 * refetches the current page never duplicates it. `total`/`preview`
 * reflect the most recently landed page's response; every page carries
 * the same preview metadata (owners, totals, state, caveats — all
 * computed over the whole diff, not just the page), only `fields`/`total`
 * vary with the filter's own paging. Resets to the first page whenever
 * `key` (or `patchId`) changes (a different def, or the same def viewed
 * from a different patch's own scoped decisions).
 */
export function useMergeFieldPage(
  key: MaybeRefOrGetter<FindingKey | null>,
  onlyConflicts: boolean,
  /**
   * Against the profile when `null`/omitted, or against that compat
   * patch's own scoped preview when given — folded straight into `useMergePreviewQuery`.
   */
  patchId: MaybeRefOrGetter<string | null> = null,
) {
  const offset = ref(0);

  const filter = computed<MergeFieldFilterDto>(() => ({
    onlyConflicts,
    search: null,
    offset: offset.value,
    limit: PAGE_SIZE,
  }));
  const query = useMergePreviewQuery(key, filter, patchId);

  const page = usePagedRows(
    offset,
    computed(() =>
      query.data.value ? { items: query.data.value.fields, total: query.data.value.total } : null,
    ),
    () => [toValue(key), toValue(patchId)],
  );

  return {
    fields: page.rows,
    total: page.total,
    hasMore: page.hasMore,
    loadMore: page.loadMore,
    /** The most recently landed page's full preview metadata (owners, totals, state, caveats — see the doc comment above). */
    preview: query.data,
    isPending: query.isPending,
    error: query.error,
  };
}
