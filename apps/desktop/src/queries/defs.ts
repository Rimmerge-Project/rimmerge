import { useQuery } from "@pinia/colada";
import { type MaybeRefOrGetter, toValue } from "vue";

import { queryKeys } from "@/queries/keys";
import { getDefConflictView, inspectDef, listModChanges, searchDefs } from "@/services/ipc";
import type { DefRef, FindingKey } from "@/types/brands";
import type { ChangeFilterDto } from "@/types/generated/ChangeFilterDto";
import type { EffectiveFieldFilterDto } from "@/types/generated/EffectiveFieldFilterDto";
import type { FieldRowFilterDto } from "@/types/generated/FieldRowFilterDto";

/** What one mod changes, filtered and paged. Disabled while `modId` is `null`. */
export function useModChangesQuery(
  modId: MaybeRefOrGetter<string | null>,
  filter: MaybeRefOrGetter<ChangeFilterDto>,
) {
  return useQuery({
    key: () => queryKeys.defs.changes(toValue(modId) ?? "", toValue(filter)),
    query: () => listModChanges(toValue(modId) ?? "", toValue(filter)),
    enabled: () => toValue(modId) !== null,
  });
}

/**
 * One def/template's full inspection under the selected order, with
 * `filter` applied to its effective-field list. Disabled while `defRef`
 * is `null`. The selected order itself isn't part of the query key —
 * switching it (`useOrderSource`) already invalidates every query on
 * success, which is enough to refetch this one too.
 */
export function useDefInspectionQuery(
  defRef: MaybeRefOrGetter<DefRef | null>,
  filter: MaybeRefOrGetter<EffectiveFieldFilterDto>,
) {
  return useQuery({
    key: () => queryKeys.defs.inspection(toValue(defRef) ?? "", toValue(filter)),
    query: () => inspectDef(toValue(defRef) ?? ("" as DefRef), toValue(filter)),
    enabled: () => toValue(defRef) !== null,
  });
}

/** Searches every def/template the scan indexed. Disabled for an empty query. */
export function useDefSearchQuery(query: MaybeRefOrGetter<string>, limit: number) {
  return useQuery({
    key: () => queryKeys.defs.search(toValue(query), limit),
    query: () => searchDefs(toValue(query), limit),
    enabled: () => toValue(query).length > 0,
  });
}

/**
 * One def-shaped finding's field-by-field conflict view, with `filter`
 * applied to its field-row list. Disabled while `key` is `null`. Against
 * the profile's own findings when `patchId` is `null`/omitted, or that
 * compat patch's own scoped preview and decisions when it names one
 * — mirrors `useMergePreviewQuery`'s own `patchId` exactly.
 */
export function useDefConflictViewQuery(
  key: MaybeRefOrGetter<FindingKey | null>,
  filter: MaybeRefOrGetter<FieldRowFilterDto>,
  patchId: MaybeRefOrGetter<string | null> = null,
) {
  return useQuery({
    key: () => queryKeys.defs.conflictView(toValue(key) ?? "", toValue(filter), toValue(patchId)),
    query: () =>
      getDefConflictView(toValue(key) ?? ("" as FindingKey), toValue(filter), toValue(patchId)),
    enabled: () => toValue(key) !== null,
  });
}
