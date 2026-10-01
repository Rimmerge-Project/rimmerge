// Merge previews, choices, and the merge mod.

import type { FieldPath, FindingKey } from "@/types/brands";
import type { MergeChoiceDto } from "@/types/generated/MergeChoiceDto";
import type { MergeFieldFilterDto } from "@/types/generated/MergeFieldFilterDto";
import type { MergeModDto } from "@/types/generated/MergeModDto";
import type { MergeModFileDto } from "@/types/generated/MergeModFileDto";
import type { MergePreviewDto } from "@/types/generated/MergePreviewDto";
import type { MergeStateDto } from "@/types/generated/MergeStateDto";
import type { TextureDto } from "@/types/generated/TextureDto";
import { call } from "./core";

/**
 * Refreshes and returns one finding's merge preview, filtered and paged —
 * against the profile's own findings when `patchId` is omitted, or
 * against that compat patch's own scoped preview when given.
 */
export function getMergePreview(
  key: FindingKey,
  filter: MergeFieldFilterDto,
  patchId?: string,
): Promise<MergePreviewDto> {
  return call("get_merge_preview", { request: { key, filter, patchId: patchId ?? null } });
}

/**
 * Validates and stores `choices` as `key`'s `Merge` decision, persisting
 * it and returning the refreshed merge state. `choices` is the full map —
 * this backend call owns applying it wholesale, not merging field by
 * field — keyed by each field's canonical path text. Stored on the
 * profile's own decisions when `patchId` is omitted, or on that compat
 * patch's own decisions when given.
 */
export function setMergeChoices(
  key: FindingKey,
  choices: Record<FieldPath, MergeChoiceDto>,
  patchId?: string,
): Promise<MergeStateDto> {
  return call("set_merge_choices", { request: { key, choices, patchId: patchId ?? null } });
}

/**
 * Describes the generated merge mod: identity, whether it currently
 * exists on disk, one entry per `Merge`/`ShipAsset` decision, the files a
 * render would produce, and every mod its content depends on. Renders in
 * memory only — never writes or removes anything.
 */
export function getMergeMod(): Promise<MergeModDto> {
  return call("get_merge_mod");
}

/**
 * Renders the merge mod in memory and returns the text content of the
 * rendered file at `relativePath` — never writes anything to disk.
 */
export function previewMergeModFile(relativePath: string): Promise<MergeModFileDto> {
  return call("preview_merge_mod_file", { request: { relativePath } });
}

/**
 * Reads `modId`'s texture file at `texturePath` (a normalized key) back
 * as a base64 `data:` URL, for the change summary's texture-override
 * panel.
 */
export function readTexture(modId: string, texturePath: string): Promise<TextureDto> {
  return call("read_texture", { modId, texturePath });
}
