import { useMutation, useQuery, useQueryCache } from "@pinia/colada";
import { type MaybeRefOrGetter, toValue } from "vue";

import { queryKeys } from "@/queries/keys";
import {
  createPatch,
  decidePatch,
  deletePatch,
  exportPatch,
  getPatch,
  getPatchFinding,
  getPatchRender,
  importProfileDecisions,
  listPatches,
  listPatchFindings,
  previewPatchFile,
  prunePatchDecisions,
  RimmergeError,
  revertPatchDecision,
  updatePatch,
} from "@/services/ipc";
import type { FindingKey } from "@/types/brands";
import type { CreatePatchRequestDto } from "@/types/generated/CreatePatchRequestDto";
import type { DecidePatchRequestDto } from "@/types/generated/DecidePatchRequestDto";
import type { ExportPatchRequestDto } from "@/types/generated/ExportPatchRequestDto";
import type { FindingFilterDto } from "@/types/generated/FindingFilterDto";
import type { ImportProfileDecisionsRequestDto } from "@/types/generated/ImportProfileDecisionsRequestDto";
import type { UpdatePatchRequestDto } from "@/types/generated/UpdatePatchRequestDto";

/** Every loaded compat patch project, summarized. */
export function usePatchesQuery() {
  return useQuery({
    key: queryKeys.patches.list,
    query: listPatches,
  });
}

/** One patch project's full detail. Disabled while `patchId` is `null`. */
export function usePatchQuery(patchId: MaybeRefOrGetter<string | null>) {
  return useQuery({
    key: () => queryKeys.patches.detail(toValue(patchId) ?? ""),
    query: () => getPatch(toValue(patchId) as string),
    enabled: () => toValue(patchId) !== null,
  });
}

/** Pages one patch's own scoped ledger. Disabled while `patchId` is `null`. */
export function usePatchFindingsQuery(
  patchId: MaybeRefOrGetter<string | null>,
  filter: MaybeRefOrGetter<FindingFilterDto>,
) {
  return useQuery({
    key: () => queryKeys.patches.findings(toValue(patchId) ?? "", toValue(filter)),
    query: () => listPatchFindings(toValue(patchId) as string, toValue(filter)),
    enabled: () => toValue(patchId) !== null,
    placeholderData: (previousData) => previousData,
  });
}

/**
 * One finding's full detail from a patch's own scoped ledger. Disabled
 * while either `patchId` or `key` is `null`.
 *
 * `finding_not_found` resolves to `null` rather than rejecting: shrinking
 * a patch's scope (`useUpdatePatchMutation`) invalidates every query,
 * including this one, at the moment it fires — and since that happens
 * before `PatchInbox`'s own `items`/`currentKey` have had a chance to
 * react to the *new*, smaller scoped-findings page, this query can be
 * asked to refetch a key the new scope already dropped, one render
 * ahead of `currentKey` itself following suit. That's an expected,
 * momentary consequence of the "invalidate everything on any mutation"
 * pattern this app uses everywhere (`queries/rules.ts`'s own mutations
 * included), not a real failure worth surfacing as a query error — the
 * finding-list side of the same race already recovers correctly on its
 * own next tick, and `PatchInbox.vue`'s `v-else` (no selection) branch
 * already renders correctly for a `null` detail.
 */
export function usePatchFindingQuery(
  patchId: MaybeRefOrGetter<string | null>,
  key: MaybeRefOrGetter<FindingKey | null>,
) {
  return useQuery({
    key: () => queryKeys.patches.finding(toValue(patchId) ?? "", toValue(key) ?? ""),
    query: async () => {
      try {
        return await getPatchFinding(toValue(patchId) as string, toValue(key) as FindingKey);
      } catch (error) {
        if (error instanceof RimmergeError && error.code === "finding_not_found") {
          return null;
        }
        throw error;
      }
    },
    enabled: () => toValue(patchId) !== null && toValue(key) !== null,
  });
}

/**
 * Renders a patch's own merge mod candidate — what `useExportPatchMutation`
 * would currently produce. Disabled while `patchId` is `null`.
 */
export function usePatchRenderQuery(patchId: MaybeRefOrGetter<string | null>) {
  return useQuery({
    key: () => queryKeys.patches.render(toValue(patchId) ?? ""),
    query: () => getPatchRender(toValue(patchId) as string),
    enabled: () => toValue(patchId) !== null,
  });
}

/**
 * One rendered file's text content from a patch's own merge mod
 * candidate. Disabled while either `patchId` or `path` is `null`.
 */
export function usePatchFileQuery(
  patchId: MaybeRefOrGetter<string | null>,
  path: MaybeRefOrGetter<string | null>,
) {
  return useQuery({
    key: () => queryKeys.patches.file(toValue(patchId) ?? "", toValue(path) ?? ""),
    query: () => previewPatchFile(toValue(patchId) as string, toValue(path) as string),
    enabled: () => toValue(patchId) !== null && toValue(path) !== null,
  });
}

/**
 * Every query, invalidated after a patch mutation — the same
 * unconditionally-broad blast radius `queries/rules.ts`'s own mutations
 * use, for the same reason: in production `session://changed` would
 * refetch everything anyway, and the Vitest/Playwright mock tier never
 * emits that event.
 */
function invalidatePatches(queryCache: ReturnType<typeof useQueryCache>): void {
  void queryCache.invalidateQueries();
}

/** Validates and creates a new compat patch project, persisting it. */
export function useCreatePatchMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: CreatePatchRequestDto) => createPatch(request),
    onSuccess: () => invalidatePatches(queryCache),
  });
}

/** Applies any of the given fields to one patch project. */
export function useUpdatePatchMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: UpdatePatchRequestDto) => updatePatch(request),
    onSuccess: () => invalidatePatches(queryCache),
  });
}

/** Deletes a patch project. Never touches a previously exported folder. */
export function useDeletePatchMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (patchId: string) => deletePatch(patchId),
    onSuccess: () => invalidatePatches(queryCache),
  });
}

/** Records a decision on one of a patch's own findings, persisting it. */
export function useDecidePatchMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: DecidePatchRequestDto) => decidePatch(request),
    onSuccess: () => invalidatePatches(queryCache),
  });
}

/** Removes a decision from one of a patch's own findings, persisting the change. */
export function useRevertPatchDecisionMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: ({ patchId, key }: { patchId: string; key: FindingKey }) =>
      revertPatchDecision(patchId, key),
    onSuccess: () => invalidatePatches(queryCache),
  });
}

/**
 * Copies the profile's `Merge`/`ShipAsset` decisions on the given keys
 * (or every key the patch's own scoped ledger admits) into the patch.
 */
export function useImportProfileDecisionsMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: ImportProfileDecisionsRequestDto) => importProfileDecisions(request),
    onSuccess: () => invalidatePatches(queryCache),
  });
}

/** Removes every decision a patch's own scoped ledger currently considers orphaned. */
export function usePrunePatchDecisionsMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (patchId: string) => prunePatchDecisions(patchId),
    onSuccess: () => invalidatePatches(queryCache),
  });
}

/**
 * Renders, writes, and (when asked) installs a patch. Rejects with a
 * `RimmergeError` whose `code` is `"rimworld_running"` when
 * `request.install` is set, the game looks like it's running, and
 * `request.force` is `false`.
 */
export function useExportPatchMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: ExportPatchRequestDto) => exportPatch(request),
    onSuccess: () => invalidatePatches(queryCache),
  });
}
