import { useMutation, useQuery, useQueryCache } from "@pinia/colada";
import { type MaybeRefOrGetter, toValue } from "vue";

import { queryKeys } from "@/queries/keys";
import {
  addAssignmentSection,
  clearAssignmentRow,
  copyAssignmentRowFrom,
  createAssignment,
  deleteAssignment,
  exportAssignment,
  getAssignment,
  getAssignmentCoverage,
  inferAssignmentCandidate,
  listAssignmentCandidates,
  listAssignmentItems,
  listAssignments,
  removeAssignmentSection,
  setAssignmentRow,
  updateAssignment,
} from "@/services/ipc";
import type { AddAssignmentSectionRequestDto } from "@/types/generated/AddAssignmentSectionRequestDto";
import type { ClearAssignmentRowRequestDto } from "@/types/generated/ClearAssignmentRowRequestDto";
import type { CopyAssignmentRowFromRequestDto } from "@/types/generated/CopyAssignmentRowFromRequestDto";
import type { CreateAssignmentRequestDto } from "@/types/generated/CreateAssignmentRequestDto";
import type { ExportAssignmentRequestDto } from "@/types/generated/ExportAssignmentRequestDto";
import type { InferAssignmentCandidateRequestDto } from "@/types/generated/InferAssignmentCandidateRequestDto";
import type { ListAssignmentCandidatesRequestDto } from "@/types/generated/ListAssignmentCandidatesRequestDto";
import type { ListItemsFilterDto } from "@/types/generated/ListItemsFilterDto";
import type { RemoveAssignmentSectionRequestDto } from "@/types/generated/RemoveAssignmentSectionRequestDto";
import type { SetAssignmentRowRequestDto } from "@/types/generated/SetAssignmentRowRequestDto";
import type { UpdateAssignmentRequestDto } from "@/types/generated/UpdateAssignmentRequestDto";

/** Every loaded assignment (patch maker) project, summarized. */
export function useAssignmentsQuery() {
  return useQuery({
    key: queryKeys.assignments.list,
    query: listAssignments,
  });
}

/** One assignment project's full detail. Disabled while `assignmentId` is `null`. */
export function useAssignmentQuery(assignmentId: MaybeRefOrGetter<string | null>) {
  return useQuery({
    key: () => queryKeys.assignments.detail(toValue(assignmentId) ?? ""),
    query: () => getAssignment(toValue(assignmentId) as string),
    enabled: () => toValue(assignmentId) !== null,
  });
}

/**
 * The coverage work queue for one assignment project's own section.
 * Disabled while `assignmentId`/`section` is `null` — the editor passes
 * `null` for a free-standing section (no coverage queue applies there),
 * never fetching a `Coverage` result that pane has no use for.
 */
export function useAssignmentCoverageQuery(
  assignmentId: MaybeRefOrGetter<string | null>,
  section: MaybeRefOrGetter<string | null>,
) {
  return useQuery({
    key: () => queryKeys.assignments.coverage(toValue(assignmentId) ?? "", toValue(section) ?? ""),
    query: () => getAssignmentCoverage(toValue(assignmentId) as string, toValue(section) as string),
    enabled: () => toValue(assignmentId) !== null && toValue(section) !== null,
    placeholderData: (previousData) => previousData,
  });
}

/**
 * Searches and pages every active def of one item type — the row editor's
 * own item pickers. Disabled while `defType` is `null`. `assignmentId`,
 * when given, prepends that project's own free-standing rows of this
 * exact item type.
 */
export function useAssignmentItemsQuery(
  defType: MaybeRefOrGetter<string | null>,
  filter: MaybeRefOrGetter<ListItemsFilterDto>,
  assignmentId?: MaybeRefOrGetter<string | null>,
) {
  return useQuery({
    key: () =>
      queryKeys.assignments.items(
        toValue(defType) ?? "",
        toValue(filter),
        toValue(assignmentId) ?? null,
      ),
    query: () =>
      listAssignmentItems({
        defType: toValue(defType) as string,
        filter: toValue(filter),
        assignmentId: toValue(assignmentId) ?? null,
      }),
    enabled: () => toValue(defType) !== null,
    placeholderData: (previousData) => previousData,
  });
}

/**
 * Every query, invalidated after an assignment mutation — the same
 * unconditionally-broad blast radius `queries/patches.ts`'s own mutations
 * use.
 */
function invalidateAssignments(queryCache: ReturnType<typeof useQueryCache>): void {
  void queryCache.invalidateQueries();
}

/**
 * Phase 1 (cheap, no instance reads): every assignment-def candidate's
 * summary for a reference/target selection — the wizard's own first step.
 * A plain mutation (not a query): its result is held in the wizard's own
 * local state, never cached against a stable key.
 */
export function useListAssignmentCandidatesMutation() {
  return useMutation({
    mutation: (request: ListAssignmentCandidatesRequestDto) => listAssignmentCandidates(request),
  });
}

/**
 * Phase 2: the real (expensive) per-type inference for exactly one
 * candidate the user picked from {@link useListAssignmentCandidatesMutation}'s
 * own result.
 */
export function useInferAssignmentCandidateMutation() {
  return useMutation({
    mutation: (request: InferAssignmentCandidateRequestDto) => inferAssignmentCandidate(request),
  });
}

/** Validates and creates a new assignment project from the wizard's confirmed schema. */
export function useCreateAssignmentMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: CreateAssignmentRequestDto) => createAssignment(request),
    onSuccess: () => invalidateAssignments(queryCache),
  });
}

/** Applies any of the given fields to one assignment project. */
export function useUpdateAssignmentMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: UpdateAssignmentRequestDto) => updateAssignment(request),
    onSuccess: () => invalidateAssignments(queryCache),
  });
}

/** Validates and records one target's row, persisting it before returning. */
export function useSetAssignmentRowMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: SetAssignmentRowRequestDto) => setAssignmentRow(request),
    onSuccess: () => invalidateAssignments(queryCache),
  });
}

/** Removes one target's row, if it has one, persisting the change before returning. */
export function useClearAssignmentRowMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: ClearAssignmentRowRequestDto) => clearAssignmentRow(request),
    onSuccess: () => invalidateAssignments(queryCache),
  });
}

/** Infers and adds a new section to an existing assignment project. */
export function useAddAssignmentSectionMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: AddAssignmentSectionRequestDto) => addAssignmentSection(request),
    onSuccess: () => invalidateAssignments(queryCache),
  });
}

/** Removes a section from an assignment project. */
export function useRemoveAssignmentSectionMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: RemoveAssignmentSectionRequestDto) => removeAssignmentSection(request),
    onSuccess: () => invalidateAssignments(queryCache),
  });
}

/**
 * Builds a fresh row from an existing instance's own fields. A plain
 * mutation: never mutates the session, so nothing needs invalidating —
 * the caller saves the result itself via {@link useSetAssignmentRowMutation}.
 */
export function useCopyAssignmentRowFromMutation() {
  return useMutation({
    mutation: (request: CopyAssignmentRowFromRequestDto) => copyAssignmentRowFrom(request),
  });
}

/** Deletes an assignment project. Never touches a previously exported folder. */
export function useDeleteAssignmentMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (assignmentId: string) => deleteAssignment(assignmentId),
    onSuccess: () => invalidateAssignments(queryCache),
  });
}

/**
 * Renders, writes, and (when asked) installs an assignment project.
 * Rejects with a `RimmergeError` whose `code` is `"rimworld_running"`
 * when `request.install` is set, the game looks like it's running, and
 * `request.force` is `false`.
 */
export function useExportAssignmentMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: ExportAssignmentRequestDto) => exportAssignment(request),
    onSuccess: () => invalidateAssignments(queryCache),
  });
}
