// Sharing a load order: exporting the order in ModsConfig.xml, previewing an imported list,
// importing it, and opening a missing mod's Workshop page.

import type { ExportOrderFileRequestDto } from "@/types/generated/ExportOrderFileRequestDto";
import type { ExportResultDto } from "@/types/generated/ExportResultDto";
import type { ExportTextDto } from "@/types/generated/ExportTextDto";
import type { ImportOrderRequestDto } from "@/types/generated/ImportOrderRequestDto";
import type { OpenWorkshopPageRequestDto } from "@/types/generated/OpenWorkshopPageRequestDto";
import type { OrderImportOutcomeDto } from "@/types/generated/OrderImportOutcomeDto";
import type { PreviewOrderImportFileRequestDto } from "@/types/generated/PreviewOrderImportFileRequestDto";
import type { PreviewOrderImportTextRequestDto } from "@/types/generated/PreviewOrderImportTextRequestDto";
import type { ProjectSummaryDto } from "@/types/generated/ProjectSummaryDto";
import { call } from "./core";

/**
 * Writes the order in `ModsConfig.xml` (re-read now, not the selected order) to a RimWorld
 * mod list. The backend accepts a `.rml` path only.
 */
export function exportOrderFile(request: ExportOrderFileRequestDto): Promise<ExportResultDto> {
  return call("export_order_file", { request });
}

/** Renders the order in `ModsConfig.xml` in the shareable text format. Writes nothing. */
export function exportOrderText(): Promise<ExportTextDto> {
  return call("export_order_text");
}

/**
 * Where the save dialog should start: a file in RimWorld's own `ModLists` folder, or `null`
 * when that folder does not exist.
 */
export function suggestedModListPath(): Promise<string | null> {
  return call("suggested_mod_list_path");
}

/** Previews importing a mod list file. Read-only; an unreadable list is a `rejected` outcome. */
export function previewOrderImportFile(
  request: PreviewOrderImportFileRequestDto,
): Promise<OrderImportOutcomeDto> {
  return call("preview_order_import_file", { request });
}

/** Previews importing pasted text. Read-only; an unreadable list is a `rejected` outcome. */
export function previewOrderImportText(
  request: PreviewOrderImportTextRequestDto,
): Promise<OrderImportOutcomeDto> {
  return call("preview_order_import_text", { request });
}

/**
 * Imports a previewed order: rescans with it and selects Current. Emits `project://progress`
 * while it runs. Writes nothing to `ModsConfig.xml`.
 */
export function importOrder(request: ImportOrderRequestDto): Promise<ProjectSummaryDto> {
  return call("import_order", { request });
}

/** Opens a mod's Steam Workshop page. Sends an id; the backend builds the fixed URL. */
export function openWorkshopPage(request: OpenWorkshopPageRequestDto): Promise<void> {
  return call("open_workshop_page", { request });
}
