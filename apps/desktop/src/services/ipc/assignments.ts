// Assignment projects.

import type { AddAssignmentSectionRequestDto } from "@/types/generated/AddAssignmentSectionRequestDto";
import type { AssignmentCandidateDto } from "@/types/generated/AssignmentCandidateDto";
import type { AssignmentDetailDto } from "@/types/generated/AssignmentDetailDto";
import type { AssignmentRowResultDto } from "@/types/generated/AssignmentRowResultDto";
import type { AssignmentSummaryDto } from "@/types/generated/AssignmentSummaryDto";
import type { AssignmentUpdateResultDto } from "@/types/generated/AssignmentUpdateResultDto";
import type { ClearAssignmentRowRequestDto } from "@/types/generated/ClearAssignmentRowRequestDto";
import type { CopyAssignmentRowFromRequestDto } from "@/types/generated/CopyAssignmentRowFromRequestDto";
import type { CopyAssignmentRowResultDto } from "@/types/generated/CopyAssignmentRowResultDto";
import type { CoverageDto } from "@/types/generated/CoverageDto";
import type { CreateAssignmentRequestDto } from "@/types/generated/CreateAssignmentRequestDto";
import type { ExportAssignmentReportDto } from "@/types/generated/ExportAssignmentReportDto";
import type { ExportAssignmentRequestDto } from "@/types/generated/ExportAssignmentRequestDto";
import type { InferAssignmentCandidateRequestDto } from "@/types/generated/InferAssignmentCandidateRequestDto";
import type { ListAssignmentCandidatesRequestDto } from "@/types/generated/ListAssignmentCandidatesRequestDto";
import type { ListAssignmentCandidatesResponseDto } from "@/types/generated/ListAssignmentCandidatesResponseDto";
import type { ListAssignmentItemsRequestDto } from "@/types/generated/ListAssignmentItemsRequestDto";
import type { ListItemsPageDto } from "@/types/generated/ListItemsPageDto";
import type { RemoveAssignmentSectionRequestDto } from "@/types/generated/RemoveAssignmentSectionRequestDto";
import type { RemoveAssignmentSectionResultDto } from "@/types/generated/RemoveAssignmentSectionResultDto";
import type { SetAssignmentRowRequestDto } from "@/types/generated/SetAssignmentRowRequestDto";
import type { UpdateAssignmentRequestDto } from "@/types/generated/UpdateAssignmentRequestDto";
import { call } from "./core";

/** Every loaded assignment (patch maker) project, summarized. */
export function listAssignments(): Promise<AssignmentSummaryDto[]> {
  return call("list_assignments");
}

/** One assignment project's full detail. */
export function getAssignment(assignmentId: string): Promise<AssignmentDetailDto> {
  return call("get_assignment", { assignmentId });
}

/**
 * Phase 1 (cheap, no instance reads): every assignment-def candidate's
 * summary (type, instance count, owners) for a reference/target selection.
 * The wizard's own first step; pick one and call {@link inferAssignmentCandidate}.
 */
export function listAssignmentCandidates(
  request: ListAssignmentCandidatesRequestDto,
): Promise<ListAssignmentCandidatesResponseDto> {
  return call("list_assignment_candidates", { request });
}

/** Phase 2: the real (expensive) per-type inference for exactly one candidate. */
export function inferAssignmentCandidate(
  request: InferAssignmentCandidateRequestDto,
): Promise<AssignmentCandidateDto> {
  return call("infer_assignment_candidate", { request });
}

/** Validates and creates a new assignment project from the wizard's confirmed schema. */
export function createAssignment(
  request: CreateAssignmentRequestDto,
): Promise<AssignmentDetailDto> {
  return call("create_assignment", { request });
}

/** Applies any of `request`'s given fields to one assignment project. */
export function updateAssignment(
  request: UpdateAssignmentRequestDto,
): Promise<AssignmentUpdateResultDto> {
  return call("update_assignment", { request });
}

/**
 * Validates and records one section's row — target-keyed (`request.target`
 * given) or free-standing (omitted, addressed by `request.row.defName`
 * alone) — persisting it before returning.
 */
export function setAssignmentRow(
  request: SetAssignmentRowRequestDto,
): Promise<AssignmentRowResultDto> {
  return call("set_assignment_row", { request });
}

/**
 * Removes one section's row — target-keyed (`request.target` given) or
 * free-standing (`request.defName` given) — if it has one, persisting the
 * change before returning.
 */
export function clearAssignmentRow(
  request: ClearAssignmentRowRequestDto,
): Promise<AssignmentRowResultDto> {
  return call("clear_assignment_row", { request });
}

/** Infers and adds a new section to an existing assignment project. */
export function addAssignmentSection(
  request: AddAssignmentSectionRequestDto,
): Promise<AssignmentDetailDto> {
  return call("add_assignment_section", { request });
}

/** Removes a section from an assignment project. */
export function removeAssignmentSection(
  request: RemoveAssignmentSectionRequestDto,
): Promise<RemoveAssignmentSectionResultDto> {
  return call("remove_assignment_section", { request });
}

/** Deletes an assignment project. Never touches a previously exported folder. */
export function deleteAssignment(assignmentId: string): Promise<void> {
  return call("delete_assignment", { assignmentId });
}

/** Builds one assignment project's coverage work queue. */
export function getAssignmentCoverage(
  assignmentId: string,
  section?: string,
): Promise<CoverageDto> {
  return call("get_assignment_coverage", { assignmentId, section: section ?? null });
}

/** Searches and pages every active def of one item type — the row editor's own item pickers. */
export function listAssignmentItems(
  request: ListAssignmentItemsRequestDto,
): Promise<ListItemsPageDto> {
  return call("list_assignment_items", { request });
}

/**
 * Builds a fresh row from an existing instance's own fields — a pure
 * preview, never saved — plus every borrowed `ItemSlot` value dropped
 * because the def it named isn't active. The caller calls
 * {@link setAssignmentRow} itself to persist the row.
 */
export function copyAssignmentRowFrom(
  request: CopyAssignmentRowFromRequestDto,
): Promise<CopyAssignmentRowResultDto> {
  return call("copy_assignment_row_from", { request });
}

/**
 * Renders, writes, and (when asked) installs an assignment project.
 * Rejects with a `RimmergeError` whose `code` is `"rimworld_running"`
 * when `request.install` is set, the game looks like it's running, and
 * `request.force` is `false`.
 */
export function exportAssignment(
  request: ExportAssignmentRequestDto,
): Promise<ExportAssignmentReportDto> {
  return call("export_assignment", { request });
}
