import { useQueryCache } from "@pinia/colada";
import { useToast } from "primevue/usetoast";
import { computed, type MaybeRefOrGetter, nextTick, type Ref, ref, toValue } from "vue";
import { useI18n } from "vue-i18n";

import type { DropMode, MenuAction } from "@/components/mods/ModTable.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useMoveModsMutation } from "@/queries/activeSet";
import { startInvalidation } from "@/queries/invalidation";
import { RimmergeError } from "@/services/ipc";
import type { AnchorDto } from "@/types/generated/AnchorDto";
import type { ModListDto } from "@/types/generated/ModListDto";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import { assertNever } from "@/utils/assertNever";
import { describeCommandError } from "@/utils/errors";

/** The row menu entries that place a mod; Groups and Color are the page's own dialogs. */
type PositionMenuAction = Exclude<MenuAction, "groups" | "color">;

import { anchorForSlot, anchorForStep, isNoopSlot, moveSetFor } from "@/utils/modMove";

/** Which of the two lists a drag started in or a drop landed on. */
export type ListKind = "active" | "inactive";

/** The drag in progress: where it started and the mods it carries. */
export interface DragState {
  readonly from: ListKind;
  readonly ids: readonly string[];
  /** Whether Core is among the shown dragged rows, which cannot leave the Active list. */
  readonly includesCore: boolean;
  /** How many carried mods are selected rows the list's filter hides. */
  readonly hiddenCount: number;
}

/** A pending "Move after…" or "Activate at…" choice: the mods and what the choice does. */
export interface PositionRequest {
  readonly mode: "move" | "activate";
  readonly ids: readonly string[];
}

/** The two list-side actions a drop or a position choice can trigger. */
interface Activation {
  activateAt: (ids: string[], anchor: AnchorDto | null) => Promise<void>;
}
interface Deactivation {
  deactivateDropped: (ids: string[]) => Promise<void>;
}

/**
 * Everything that changes the Active order or the lists' membership by position on the Mods
 * page: the drag in progress and what each list does with it, the one-step and Move menu moves,
 * and the status text that announces each move. Every route ends in an anchor (a neighbour named
 * by id), so a drop is exact under a filtered view, and in one of the same three calls: `move_mods`,
 * a planned anchored activation, a planned deactivation.
 *
 * Reordering is gated on the backend's `reorderable` answer for the Active list, never on the sort.
 */
export function useModReorder({
  activeList,
  inactiveList,
  isActiveFiltered,
  selectedActiveIds,
  selectedInactiveIds,
  activation,
  deactivation,
  focusActiveRow,
  onSwitchedToCurrent,
}: {
  /** The Active list's latest answer. */
  activeList: MaybeRefOrGetter<ModListDto | undefined>;
  /** The Inactive list's latest answer. */
  inactiveList: MaybeRefOrGetter<ModListDto | undefined>;
  /** Whether the Active list is showing a filtered view (its `total` is then not the order's size). */
  isActiveFiltered: MaybeRefOrGetter<boolean>;
  selectedActiveIds: Ref<string[]>;
  selectedInactiveIds: Ref<string[]>;
  activation: Activation;
  deactivation: Deactivation;
  /** Puts keyboard focus back on an Active row after a keyboard move. */
  focusActiveRow: (modId: string) => Promise<void>;
  /** Called after a move that switched the selected order from Suggested to Current. */
  onSwitchedToCurrent: () => void;
}) {
  const { t } = useI18n();
  const tm = useTranslateMessage();
  const toast = useToast();
  const queryCache = useQueryCache();
  const modLabel = useModLabel();
  const { mutateAsync: move } = useMoveModsMutation();

  const drag = ref<DragState | null>(null);
  const positionRequest = ref<PositionRequest | null>(null);
  /** The text of the one always-mounted live region. */
  const announcement = ref("");

  const activeRows = computed<readonly ModListRowDto[]>(() => toValue(activeList)?.items ?? []);
  const inactiveRows = computed<readonly ModListRowDto[]>(() => toValue(inactiveList)?.items ?? []);
  const isReorderable = computed(() => toValue(activeList)?.reorderable === true);
  /** Why moving is off, or `null` while the Active list is in load order. */
  const reorderBlockedReason = computed(() =>
    toValue(activeList) !== undefined && !isReorderable.value ? t("mods.move.blockedReason") : null,
  );

  function startDrag(from: ListKind, modId: string): void {
    const rows = from === "active" ? activeRows.value : inactiveRows.value;
    const selected = from === "active" ? selectedActiveIds.value : selectedInactiveIds.value;
    const ids = moveSetFor(modId, selected, rows);
    const dragged = new Set(ids);
    const shownIds = new Set(rows.map((row) => row.modId));
    drag.value = {
      from,
      ids,
      hiddenCount: ids.filter((id) => !shownIds.has(id)).length,
      includesCore: rows.some((row) => dragged.has(row.modId) && row.source === "core"),
    };
  }

  function endDrag(): void {
    drag.value = null;
  }

  /** What `target` does with the drag in progress. */
  function dropModeFor(target: ListKind): DropMode {
    const current = drag.value;
    if (current === null) {
      return "off";
    }
    if (target === "active") {
      if (isReorderable.value) {
        return "position";
      }
      return current.from === "inactive" ? "pane" : "refused";
    }
    if (current.from === "inactive" || current.includesCore) {
      return "refused";
    }
    return "pane";
  }

  /** The reason a refused drag shows over `target`; empty when it is not refused. */
  function refusalFor(target: ListKind): string {
    if (dropModeFor(target) !== "refused") {
      return "";
    }
    if (target === "active") {
      return t("mods.move.blockedReason");
    }
    return drag.value?.from === "inactive"
      ? t("mods.move.inactiveNotOrdered")
      : t("mods.move.coreStaysActive");
  }

  function positionOf(modId: string): number | null {
    return activeRows.value.find((row) => row.modId === modId)?.position ?? null;
  }

  /**
   * Sets the live-region text. The text is cleared for a tick first: a screen reader only reads a
   * change, so the same sentence twice in a row (two identical steps) would otherwise be silent.
   */
  async function announce(text: string): Promise<void> {
    announcement.value = "";
    await nextTick();
    announcement.value = text;
  }

  /**
   * "Moved X to position 14 of 312", from the refreshed Active list. A move carries the selected
   * rows a filter hides, so a block move says how many of them were not shown.
   */
  async function announceMoved(ids: readonly string[]): Promise<void> {
    const positions = ids.map(positionOf).filter((position) => position !== null);
    if (positions.length === 0) {
      return;
    }
    const position = Math.min(...positions) + 1;
    const list = toValue(activeList);
    // Under a filter `total` counts only the matches, so it is not the size of the order.
    const total = list === undefined || toValue(isActiveFiltered) ? null : list.total;
    const [firstId] = ids;
    if (ids.length === 1 && firstId !== undefined) {
      const name = modLabel.label(firstId);
      await announce(
        total === null
          ? t("mods.move.announced", { name, position })
          : t("mods.move.announcedOf", { name, position, total }),
      );
      return;
    }
    const count = ids.length;
    const hidden = count - positions.length;
    await announce(manyMovedText({ count, hidden, position, total }));
  }

  function manyMovedText({
    count,
    hidden,
    position,
    total,
  }: {
    count: number;
    hidden: number;
    position: number;
    total: number | null;
  }): string {
    if (hidden > 0) {
      return total === null
        ? t("mods.move.announcedManyHidden", { count, hidden, position }, count)
        : t("mods.move.announcedManyHiddenOf", { count, hidden, position, total }, count);
    }
    return total === null
      ? t("mods.move.announcedMany", { count, position }, count)
      : t("mods.move.announcedManyOf", { count, position, total }, count);
  }

  async function performMove(
    ids: readonly string[],
    anchor: AnchorDto,
    focusModId: string | null,
  ): Promise<void> {
    try {
      const result = await move({ ids: [...ids], anchor });
      await announceMoved(ids);
      if (result.didSwitchToCurrent) {
        onSwitchedToCurrent();
      }
      if (focusModId !== null) {
        await focusActiveRow(focusModId);
      }
    } catch (caught: unknown) {
      if (caught instanceof RimmergeError && caught.code === "move_anchor_not_active") {
        startInvalidation(queryCache);
      }
      const described = describeCommandError(caught);
      toast.add({
        severity: "error",
        summary: tm(described.title),
        detail: tm(described.detail),
        life: 8000,
      });
    }
  }

  /** A drop on `target` before the row at `slot`. */
  async function drop(target: ListKind, slot: number): Promise<void> {
    const current = drag.value;
    endDrag();
    if (current === null) {
      return;
    }
    const ids = [...current.ids];
    if (target === "inactive") {
      if (current.from === "active" && !current.includesCore) {
        await deactivation.deactivateDropped(ids);
      }
      return;
    }
    const moving = new Set(ids);
    if (current.from === "active") {
      // Defence in depth: `ModTable` already refuses a drop on a list that is not reorderable, so
      // this only guards a caller that skips the table.
      if (!isReorderable.value || isNoopSlot(activeRows.value, slot, moving)) {
        return;
      }
      await performMove(ids, anchorForSlot(activeRows.value, slot, moving), null);
      return;
    }
    // Under another sort a gap in the view means nothing in load order: default placement.
    const anchor = isReorderable.value ? anchorForSlot(activeRows.value, slot, moving) : null;
    await activation.activateAt(ids, anchor);
  }

  /** One shown row up or down (the Move menu and Alt+Arrow). */
  async function step(modId: string, direction: "up" | "down"): Promise<void> {
    if (reorderBlockedReason.value !== null) {
      await announce(reorderBlockedReason.value);
      return;
    }
    const ids = moveSetFor(modId, selectedActiveIds.value, activeRows.value);
    const anchor = anchorForStep(activeRows.value, new Set(ids), direction);
    if (anchor === null) {
      await announce(t(direction === "up" ? "mods.move.edgeTop" : "mods.move.edgeBottom"));
      return;
    }
    await performMove(ids, anchor, modId);
  }

  /** The Move menu's other entries, and an inactive row's "Activate at…". */
  async function menuAction(modId: string, action: PositionMenuAction): Promise<void> {
    switch (action) {
      case "top":
      case "bottom": {
        const ids = moveSetFor(modId, selectedActiveIds.value, activeRows.value);
        await performMove(ids, { kind: action === "top" ? "start" : "end" }, modId);
        return;
      }
      case "after":
        positionRequest.value = {
          mode: "move",
          ids: moveSetFor(modId, selectedActiveIds.value, activeRows.value),
        };
        return;
      case "activateAt":
        positionRequest.value = {
          mode: "activate",
          ids: moveSetFor(modId, selectedInactiveIds.value, inactiveRows.value),
        };
        return;
      default:
        assertNever(action);
    }
  }

  /** The position dialog's choice, applied to the mods it was opened for. */
  async function positionChosen(anchor: AnchorDto): Promise<void> {
    const request = positionRequest.value;
    positionRequest.value = null;
    if (request === null) {
      return;
    }
    const ids = [...request.ids];
    if (request.mode === "move") {
      await performMove(ids, anchor, ids[0] ?? null);
      return;
    }
    await activation.activateAt(ids, anchor);
  }

  return {
    drag,
    positionRequest,
    announcement,
    isReorderable,
    reorderBlockedReason,
    announce,
    startDrag,
    endDrag,
    dropModeFor,
    refusalFor,
    drop,
    step,
    menuAction,
    positionChosen,
  };
}
