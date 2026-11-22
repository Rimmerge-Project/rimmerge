import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import type { TextRange } from "@/utils/textMarks";

/**
 * The matched ranges to draw in `label`, the text a list row shows for the mod. The backend
 * matched the row's name and its id; the label is one of them (the id in id mode, the name in
 * name mode), so the matching ranges are picked by comparing what is *shown*, never by the mode
 * flag: a name-mode label that fell back to the id marks the id's ranges, and a label that is
 * neither (a name the list's own row does not carry) marks nothing rather than guess.
 */
export function marksForLabel(
  mod: Pick<ModListRowDto, "name" | "modId" | "search">,
  label: string,
): readonly TextRange[] {
  const search = mod.search;
  if (search === undefined) return [];
  if (label === mod.name) return search.name;
  if (label === mod.modId) return search.id;
  return [];
}
