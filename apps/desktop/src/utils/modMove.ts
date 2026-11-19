import type { ActivatePlanDto } from "@/types/generated/ActivatePlanDto";
import type { AnchorDto } from "@/types/generated/AnchorDto";
import type { DeactivatePlanDto } from "@/types/generated/DeactivatePlanDto";

/** The part of a list row the move arithmetic reads. */
export interface MoveRow {
  readonly modId: string;
}

/**
 * The anchor for dropping `moving` into the gap before the row at `slot` (`rows.length` is the
 * gap after the last row). Anchors name a neighbour by id, never an index, so the result is exact
 * for a filtered view: the neighbour is the nearest row that is shown and is not itself moving.
 * Nothing shown but moving rows (or an empty view) puts the mods at the end.
 */
export function anchorForSlot(
  rows: readonly MoveRow[],
  slot: number,
  moving: ReadonlySet<string>,
): AnchorDto {
  const gap = Math.min(Math.max(slot, 0), rows.length);
  const next = rows.slice(gap).find((row) => !moving.has(row.modId));
  if (next !== undefined) {
    return { kind: "before", modId: next.modId };
  }
  const previous = rows.slice(0, gap).findLast((row) => !moving.has(row.modId));
  if (previous !== undefined) {
    return { kind: "after", modId: previous.modId };
  }
  return { kind: "end" };
}

/**
 * The anchor for moving `moving` one shown row up or down: just before the nearest shown,
 * non-moving row above the topmost moving row, or just after the nearest below the lowest one.
 * `null` when there is no such row (already at that edge of the view) or none of the moving rows
 * is shown.
 */
export function anchorForStep(
  rows: readonly MoveRow[],
  moving: ReadonlySet<string>,
  direction: "up" | "down",
): AnchorDto | null {
  const first = rows.findIndex((row) => moving.has(row.modId));
  if (first === -1) {
    return null;
  }
  if (direction === "up") {
    const above = rows.slice(0, first).findLast((row) => !moving.has(row.modId));
    return above === undefined ? null : { kind: "before", modId: above.modId };
  }
  const last = rows.findLastIndex((row) => moving.has(row.modId));
  const below = rows.slice(last + 1).find((row) => !moving.has(row.modId));
  return below === undefined ? null : { kind: "after", modId: below.modId };
}

/**
 * Whether dropping `moving` into the gap before `slot` would leave the list as it is: every
 * moving mod is shown, they sit together, and the gap is inside or right beside that block.
 */
export function isNoopSlot(
  rows: readonly MoveRow[],
  slot: number,
  moving: ReadonlySet<string>,
): boolean {
  const indices = rows.flatMap((row, index) => (moving.has(row.modId) ? [index] : []));
  const first = indices[0];
  const last = indices.at(-1);
  if (first === undefined || last === undefined || indices.length !== moving.size) {
    return false;
  }
  return last - first + 1 === indices.length && slot >= first && slot <= last + 1;
}

/**
 * The mods a drag or a Move menu acts on for the row `modId`: the whole selection when the row is
 * part of it (shown rows in list order, then selected rows a filter hides, in selection order),
 * otherwise just that row.
 */
export function moveSetFor(
  modId: string,
  selectedIds: readonly string[],
  rows: readonly MoveRow[],
): string[] {
  if (!selectedIds.includes(modId)) {
    return [modId];
  }
  const selected = new Set(selectedIds);
  const shown = rows.filter((row) => selected.has(row.modId)).map((row) => row.modId);
  const shownSet = new Set(shown);
  return [...shown, ...selectedIds.filter((id) => !shownSet.has(id))];
}

/**
 * What a plan does once it has come back: `ask` always opens the confirm dialog (the buttons),
 * `commitIfPlain` commits straight away when the plan is exactly what was asked for with no
 * warning (a drop or a position choice) and opens the dialog otherwise.
 */
export type PlanMode = "ask" | "commitIfPlain";

function sameIds(left: readonly string[], right: readonly string[]): boolean {
  const rightSet = new Set(right);
  return left.length === rightSet.size && left.every((id) => rightSet.has(id));
}

/**
 * Whether a dropped activation may commit without the confirm dialog: the plan adds exactly the
 * dragged mods (no dependency pulled in, none skipped) and carries no warning.
 */
export function isPlainActivation(plan: ActivatePlanDto, droppedIds: readonly string[]): boolean {
  return (
    sameIds(plan.toAdd, droppedIds) &&
    plan.alreadyActive.length === 0 &&
    Object.keys(plan.unresolvableDependencies).length === 0
  );
}

/**
 * Whether a dropped deactivation may commit without the confirm dialog: it removes exactly the
 * dragged mods, refuses none, and no mod still active depends on a removed one.
 */
export function isPlainDeactivation(
  plan: DeactivatePlanDto,
  droppedIds: readonly string[],
): boolean {
  const hasDependents = Object.values(plan.dependentsStillActive).some(
    (dependents) => dependents.length > 0,
  );
  return sameIds(plan.toRemove, droppedIds) && plan.refused.length === 0 && !hasDependents;
}
