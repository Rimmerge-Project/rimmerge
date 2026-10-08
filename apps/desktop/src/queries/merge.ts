import { useMutation, useQuery, useQueryCache } from "@pinia/colada";
import { type MaybeRefOrGetter, toValue } from "vue";

import { queryKeys } from "@/queries/keys";
import {
  getMergeMod,
  getMergePreview,
  previewMergeModFile,
  readTexture,
  setMergeChoices,
} from "@/services/ipc";
import type { FieldPath, FindingKey } from "@/types/brands";
import type { MergeChoiceDto } from "@/types/generated/MergeChoiceDto";
import type { MergeFieldFilterDto } from "@/types/generated/MergeFieldFilterDto";

/**
 * Refreshes and returns one finding's merge preview — against the
 * profile when `patchId` is `null`/omitted, or against that compat
 * patch's own scoped preview when given. Disabled while `key` is `null`.
 */
export function useMergePreviewQuery(
  key: MaybeRefOrGetter<FindingKey | null>,
  filter: MaybeRefOrGetter<MergeFieldFilterDto>,
  patchId: MaybeRefOrGetter<string | null> = null,
) {
  return useQuery({
    key: () => queryKeys.merge.preview(toValue(key) ?? "", toValue(filter), toValue(patchId)),
    query: () =>
      getMergePreview(toValue(key) as FindingKey, toValue(filter), toValue(patchId) ?? undefined),
    enabled: () => toValue(key) !== null,
  });
}

/**
 * Validates and stores a finding's per-field merge choices, persisting
 * the decision before returning — on the profile's own decisions when
 * `patchId` is omitted, or on that compat patch's own decisions when
 * given.
 *
 * Invalidates only three query key prefixes on success — narrower,
 * deliberately, than `useApplyMutation`/`useDecideMutation`'s own blanket
 * `invalidateQueries`: a merge decision only ever moves *this* finding's
 * own status
 * (`findings`), preview (`merge`), and def-conflict view (`defs`,
 * `"conflictView"` only — never `"changes"`/`"inspection"`/`"search"`,
 * which read `InspectDef`'s own cache, deliberately *not* invalidated by a
 * decision that leaves the selected order and active mods untouched, per
 * `rim-session/CLAUDE.md`'s own `Session.inspections` doc comment) —
 * never a mod's status, the dashboard's ledger stats, or anything
 * rules/tags/patches/assignments own, so invalidating everything else too
 * was pure waste on a page whose two preview queries (`MergeEditorPage`'s
 * `conflictsPage`/`allPage`) are each genuinely expensive to rebuild
 * (`rim_session::use_cases::PlanMerge` replays every contributing patch
 * from scratch). **The `defs`/`"conflictView"` prefix is required**:
 * without it, a decided collision's `DefConflictView` chip never
 * refreshes after `Escape` back to the inbox
 * (`e2e/specs/def-conflict.spec.ts`'s own "after-merge value" spec
 * covers it).
 * `useSessionEvents`'s own handler skips `mergeChanged`/`patchDecided`
 * entirely rather than invalidating the same prefixes again — this
 * mutation is the only caller of `set_merge_choices` in this app, so a
 * second, wider invalidation there would only refetch these same queries
 * a second time for no gain.
 */
export function useSetMergeChoicesMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: ({
      key,
      choices,
      patchId,
    }: {
      key: FindingKey;
      choices: Record<FieldPath, MergeChoiceDto>;
      patchId?: string;
    }) => setMergeChoices(key, choices, patchId),
    onSuccess: () =>
      Promise.all([
        queryCache.invalidateQueries({ key: ["merge"] }),
        queryCache.invalidateQueries({ key: ["findings"] }),
        queryCache.invalidateQueries({ key: ["defs", "conflictView"] }),
      ]),
  });
}

/**
 * Describes the generated merge mod. Renders in memory only.
 *
 * The render replays every ledger entry on a freshly swapped session (seconds on a large
 * install) while holding the session lock, so call this only from components that are mounted
 * while the answer is wanted (the Merge mod page, the Apply dialog's body). Do not add an
 * `enabled` option: observers share one cache entry and `invalidateQueries` honours only the
 * last observer's options, so one caller's gate would switch off another's refresh. An entry
 * with no mounted observer is simply skipped by an invalidation.
 *
 * `staleTime: Infinity`, as `useDefGraphicQuery`: the answer changes only with the session, and
 * every change to it invalidates this entry (a swap, a decision, `session://changed`). Reopening
 * the dialog or refocusing the window therefore does not re-render under the session lock.
 */
export function useMergeModQuery() {
  return useQuery({
    key: queryKeys.merge.mod,
    query: getMergeMod,
    staleTime: Number.POSITIVE_INFINITY,
  });
}

/**
 * Renders the merge mod in memory and returns one rendered file's text
 * content. Disabled while `path` is `null`.
 */
export function useMergeModFileQuery(path: MaybeRefOrGetter<string | null>) {
  return useQuery({
    key: () => queryKeys.merge.file(toValue(path) ?? ""),
    query: () => previewMergeModFile(toValue(path) as string),
    enabled: () => toValue(path) !== null,
  });
}

/**
 * Reads one mod's texture file as a displayable data URL, for the
 * texture-override change summary's side-by-side images. Disabled while
 * either `modId` or `texturePath` is `null`.
 *
 * `staleTime: Infinity` — a given `(modId, texturePath)` pair's bytes
 * never change within a loaded project (the file on disk is whatever the
 * scan already found), so there's nothing to gain by treating it as due
 * for a background refetch the way every other query's default 5s
 * `staleTime` would (a remount, a window refocus, ...) — deciding a
 * finding elsewhere on the page shouldn't reload an already-displayed
 * image. This doesn't override an explicit `queryCache.invalidateQueries()`
 * call (every mutation's own blast radius, and `session://changed`'s) —
 * invalidation always wins regardless of `staleTime` — only the
 * time-based staleness checks `staleTime` itself controls.
 */
export function useTextureQuery(
  modId: MaybeRefOrGetter<string | null>,
  texturePath: MaybeRefOrGetter<string | null>,
) {
  return useQuery({
    key: () => queryKeys.textures.one(toValue(modId) ?? "", toValue(texturePath) ?? ""),
    query: () => readTexture(toValue(modId) as string, toValue(texturePath) as string),
    enabled: () => toValue(modId) !== null && toValue(texturePath) !== null,
    staleTime: Number.POSITIVE_INFINITY,
  });
}
