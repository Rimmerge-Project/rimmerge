import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import { findNode, subtreeIds } from "@/utils/organization";

/** The drag type a mod row sets, so a drop target can tell a row drag from a file or text. */
export const ROW_DRAG_TYPE = "application/x-rimmerge-mods";

/** The drag type a group tree entry sets. */
export const GROUP_DRAG_TYPE = "application/x-rimmerge-group";

/** Why a group cannot be dropped on a target, or `null` when it can. */
export type ReparentRefusal = "inside-itself" | "already-there";

/**
 * What stops group `id` going under `parent` (`null` for the top level): itself or one of its
 * own subgroups, or the place it already is. Depth is not checked here: the backend owns that
 * limit, and its refusal shows as an error toast.
 */
export function reparentRefusal(
  nodes: readonly GroupNodeDto[],
  id: number,
  parent: number | null,
): ReparentRefusal | null {
  const group = findNode(nodes, id);
  if (group === undefined) {
    return null;
  }
  if (parent !== null && subtreeIds(nodes, id).has(parent)) {
    return "inside-itself";
  }
  return group.parent === parent ? "already-there" : null;
}
