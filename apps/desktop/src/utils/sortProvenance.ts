import type { SortProvenanceDto } from "@/types/generated/SortProvenanceDto";

/** The `t()` shape both functions below take — a caller's own `useI18n().t`. */
type Translate = (key: string, params?: Record<string, unknown>) => string;

/** A human-readable label for one {@link SortProvenanceDto}'s `tieBreak`. */
export function describeTieBreak(tieBreak: SortProvenanceDto["tieBreak"], t: Translate): string {
  return tieBreak === "rebuild"
    ? t("order.sortProvenance.rebuild")
    : t("order.sortProvenance.preserveCurrent");
}

/**
 * A one-line summary of which settings produced a suggested order — shown
 * next to the dashboard's and the apply dialog's "mods that move" figure,
 * and in the why-panel, so a large disturbance (the first `rebuild` of a
 * list RimSort produced moves most mods, by design) is explained rather
 * than alarming. `t` is the caller's own `useI18n().t` — this function
 * can't call `useI18n()` itself (a plain `utils/` helper, not a
 * component).
 */
export function describeSortProvenance(provenance: SortProvenanceDto, t: Translate): string {
  const pairs = provenance.useImportedPairs
    ? t("order.sortProvenance.on")
    : t("order.sortProvenance.off");
  const placements = provenance.useImportedPlacements
    ? t("order.sortProvenance.on")
    : t("order.sortProvenance.off");
  return t("order.sortProvenance.summary", {
    tieBreak: describeTieBreak(provenance.tieBreak, t),
    pairs,
    placements,
  });
}
