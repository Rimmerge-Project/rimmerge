// Patch projects.

import type { FindingKey } from "@/types/brands";
import type { CreatePatchRequestDto } from "@/types/generated/CreatePatchRequestDto";
import type { DecidePatchRequestDto } from "@/types/generated/DecidePatchRequestDto";
import type { DecideResultDto } from "@/types/generated/DecideResultDto";
import type { ExportPatchReportDto } from "@/types/generated/ExportPatchReportDto";
import type { ExportPatchRequestDto } from "@/types/generated/ExportPatchRequestDto";
import type { FindingFilterDto } from "@/types/generated/FindingFilterDto";
import type { FindingPageDto } from "@/types/generated/FindingPageDto";
import type { ImportDecisionsReportDto } from "@/types/generated/ImportDecisionsReportDto";
import type { ImportProfileDecisionsRequestDto } from "@/types/generated/ImportProfileDecisionsRequestDto";
import type { MergeModFileDto } from "@/types/generated/MergeModFileDto";
import type { PatchDetailDto } from "@/types/generated/PatchDetailDto";
import type { PatchFindingsRequestDto } from "@/types/generated/PatchFindingsRequestDto";
import type { PatchRenderDto } from "@/types/generated/PatchRenderDto";
import type { PatchSummaryDto } from "@/types/generated/PatchSummaryDto";
import type { PatchUpdateResultDto } from "@/types/generated/PatchUpdateResultDto";
import type { ResolutionDetailDto } from "@/types/generated/ResolutionDetailDto";
import type { UpdatePatchRequestDto } from "@/types/generated/UpdatePatchRequestDto";
import { call } from "./core";

/** Every loaded compat patch project, summarized. */
export function listPatches(): Promise<PatchSummaryDto[]> {
  return call("list_patches");
}

/** One patch project's full detail. */
export function getPatch(patchId: string): Promise<PatchDetailDto> {
  return call("get_patch", { patchId });
}

/** Validates and creates a new compat patch project, persisting it. */
export function createPatch(request: CreatePatchRequestDto): Promise<PatchDetailDto> {
  return call("create_patch", { request });
}

/** Applies any of `request`'s given fields to one patch project. */
export function updatePatch(request: UpdatePatchRequestDto): Promise<PatchUpdateResultDto> {
  return call("update_patch", { request });
}

/** Deletes a patch project. Never touches a previously exported folder. */
export function deletePatch(patchId: string): Promise<void> {
  return call("delete_patch", { patchId });
}

/** Pages one patch's own scoped ledger for the selected order. */
export function listPatchFindings(
  patchId: string,
  filter: FindingFilterDto,
): Promise<FindingPageDto> {
  const request: PatchFindingsRequestDto = { patchId, filter };
  return call("list_patch_findings", { request });
}

/** One finding's full detail from a patch's own scoped ledger. */
export function getPatchFinding(patchId: string, key: FindingKey): Promise<ResolutionDetailDto> {
  return call("get_patch_finding", { patchId, key });
}

/**
 * Records a decision on one of a patch's own findings, persisting it
 * before returning. Never resorts and never touches the profile ledger.
 */
export function decidePatch(request: DecidePatchRequestDto): Promise<DecideResultDto> {
  return call("decide_patch", { request });
}

/** Removes a decision from one of a patch's own findings, persisting the change. */
export function revertPatchDecision(patchId: string, key: FindingKey): Promise<DecideResultDto> {
  return call("revert_patch_decision", { patchId, key });
}

/**
 * Copies the profile's `Merge`/`ShipAsset` decisions on the given keys
 * (or, when `keys` is `null`, every key the patch's own scoped ledger
 * admits) into the patch.
 */
export function importProfileDecisions(
  request: ImportProfileDecisionsRequestDto,
): Promise<ImportDecisionsReportDto> {
  return call("import_profile_decisions", { request });
}

/** Removes every decision a patch's own scoped ledger currently considers orphaned. */
export function prunePatchDecisions(patchId: string): Promise<PatchDetailDto> {
  return call("prune_patch_decisions", { patchId });
}

/**
 * Renders a patch's own merge mod candidate in memory — what
 * `exportPatch` would currently produce. Never writes.
 */
export function getPatchRender(patchId: string): Promise<PatchRenderDto> {
  return call("get_patch_render", { patchId });
}

/**
 * Renders a patch's own merge mod candidate in memory and returns the
 * text content of the rendered file at `relativePath` — never writes
 * anything to disk.
 */
export function previewPatchFile(patchId: string, relativePath: string): Promise<MergeModFileDto> {
  return call("preview_patch_file", { patchId, relativePath });
}

/**
 * Renders, writes, and (when asked) installs a patch. Rejects with a
 * `RimmergeError` whose `code` is `"rimworld_running"` when
 * `request.install` is set, the game looks like it's running, and
 * `request.force` is `false`.
 */
export function exportPatch(request: ExportPatchRequestDto): Promise<ExportPatchReportDto> {
  return call("export_patch", { request });
}
