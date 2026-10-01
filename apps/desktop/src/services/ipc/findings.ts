// Findings, decisions, and def inspection.

import type { DefRef, FindingKey } from "@/types/brands";
import type { ChangeFilterDto } from "@/types/generated/ChangeFilterDto";
import type { ChangePageDto } from "@/types/generated/ChangePageDto";
import type { DecideRequestDto } from "@/types/generated/DecideRequestDto";
import type { DecideResultDto } from "@/types/generated/DecideResultDto";
import type { DefConflictViewDto } from "@/types/generated/DefConflictViewDto";
import type { DefGraphicDto } from "@/types/generated/DefGraphicDto";
import type { DefInspectionDto } from "@/types/generated/DefInspectionDto";
import type { DefSearchHitDto } from "@/types/generated/DefSearchHitDto";
import type { DefTextureDto } from "@/types/generated/DefTextureDto";
import type { EffectiveFieldFilterDto } from "@/types/generated/EffectiveFieldFilterDto";
import type { FieldRowFilterDto } from "@/types/generated/FieldRowFilterDto";
import type { FindingFilterDto } from "@/types/generated/FindingFilterDto";
import type { FindingPageDto } from "@/types/generated/FindingPageDto";
import type { ResolutionDetailDto } from "@/types/generated/ResolutionDetailDto";
import { call } from "./core";

/** Pages the selected order's ledger. */
export function listFindings(filter: FindingFilterDto): Promise<FindingPageDto> {
  return call("list_findings", { filter });
}

/** One finding's full detail from the selected order's ledger. */
export function getFinding(key: FindingKey): Promise<ResolutionDetailDto> {
  return call("get_finding", { key });
}

/** Records a decision on a finding, persisting it before returning. */
export function decide(request: DecideRequestDto): Promise<DecideResultDto> {
  return call("decide", { request });
}

/** Removes a decision on a finding, persisting the change before returning. */
export function revertDecision(key: FindingKey): Promise<DecideResultDto> {
  return call("revert_decision", { key });
}

/** Lists what `modId` changes: defs it owns, templates it registers, foreign defs it patches, assets it overrides. */
export function listModChanges(modId: string, filter: ChangeFilterDto): Promise<ChangePageDto> {
  return call("list_mod_changes", { modId, filter });
}

/**
 * Inspects one def or `Name`-attributed template under the selected
 * order: every owner and patcher, the template chain, and the effective
 * def the game actually runs, with `filter` applied to the effective-field
 * list.
 */
export function inspectDef(
  defRef: DefRef,
  filter: EffectiveFieldFilterDto,
): Promise<DefInspectionDto> {
  return call("inspect_def", { request: { defRef, filter } });
}

/**
 * What `defRef` shows under the selected order: its slots (each texture
 * with its availability), or that it shows nothing of its own. Read-only.
 */
export function resolveDefGraphic(defRef: DefRef): Promise<DefGraphicDto> {
  return call("resolve_def_graphic", { request: { defRef } });
}

/**
 * One texture of `defRef`'s resolved graphic, as a `data:` URL or the
 * reason there is none. `textureKey` must be a key taken from that def's
 * own {@link resolveDefGraphic} answer; any other key is refused with
 * `invalid_input`.
 */
export function readDefTexture(defRef: DefRef, textureKey: string): Promise<DefTextureDto> {
  return call("read_def_texture", { request: { defRef, textureKey } });
}

/** Searches every def and `Name`-attributed template the scan indexed, name matches before type-only matches. */
export function searchDefs(query: string, limit: number): Promise<DefSearchHitDto[]> {
  return call("search_defs", { query, limit });
}

/**
 * One def-shaped finding's (`defOverride`/`patchCollision`/
 * `duplicateTemplateName`) field-by-field conflict view: who changed
 * what, who wins and why, and what couldn't be evaluated. Runs the
 * necessary inspect/plan steps against the selected order itself, so it
 * never needs those run first. Against the profile's own findings when
 * `patchId` is `null`, or that compat patch's own scoped preview and
 * decisions when it names one — mirrors {@link getMergePreview}'s
 * own `patchId` exactly.
 */
export function getDefConflictView(
  key: FindingKey,
  filter: FieldRowFilterDto,
  patchId: string | null = null,
): Promise<DefConflictViewDto> {
  return call("get_def_conflict_view", { request: { key, filter, patchId } });
}
