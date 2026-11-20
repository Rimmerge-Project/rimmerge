/** The part of vue-i18n's `t` this module needs: a message lookup, plural-aware when given a count. */
type Translate = (key: string, named: Record<string, unknown>, plural?: number) => string;

/**
 * "12 selected", or "12 selected (3 not shown)" when the filter hides some of them. One whole
 * sentence, so the note about hidden rows (and any space or bracket around it) belongs to the
 * translation. Each number is worded by its own plural rule: `selected` and `hidden` are two
 * parts, each agreeing with its own count (ru, pl and uk change the noun with the number).
 */
export function selectionSummary(t: Translate, selectedCount: number, hiddenCount: number): string {
  const selected = t("mods.selection.selectedCount", { count: selectedCount }, selectedCount);
  if (hiddenCount <= 0) {
    return selected;
  }
  return t("mods.selection.selectedCountHidden", {
    selected,
    hidden: t("mods.selection.hiddenPart", { count: hiddenCount }, hiddenCount),
  });
}
