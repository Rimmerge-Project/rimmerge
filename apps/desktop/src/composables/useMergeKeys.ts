import { type MaybeRefOrGetter, onMounted, onScopeDispose, ref, toValue } from "vue";

import { isInert, ownsArrowKeys, ownsEnterNatively } from "@/composables/shortcutTargets";
import { useMergeStore } from "@/stores/merge";
import { asFieldPath, type FieldPath } from "@/types/brands";
import type { MergeChoiceDto } from "@/types/generated/MergeChoiceDto";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import type { MergeOwnerDto } from "@/types/generated/MergeOwnerDto";
import type { MergeStateDto } from "@/types/generated/MergeStateDto";

/** The choices half of `useMergeChoices` — the only part `useMergeKeys` needs. */
export interface MergeChoiceActions {
  setChoice: (path: FieldPath, choice: MergeChoiceDto) => Promise<MergeStateDto>;
  clearChoice: (path: FieldPath) => Promise<MergeStateDto>;
}

/** What `useMergeKeys` needs from its host page. */
export interface UseMergeKeysOptions {
  /** The currently visible field rows (conflicts, then auto if expanded, then unchanged if shown), in display order. */
  rows: MaybeRefOrGetter<MergeFieldDto[]>;
  /** Every owner, in the selected order — column 0 is always the base. */
  owners: MaybeRefOrGetter<MergeOwnerDto[]>;
  /** The selected-order winner's mod id — what `w` chooses. */
  winner: MaybeRefOrGetter<string>;
  choices: MergeChoiceActions;
  /**
   * Whether the current preview is for a `PatchCollision` finding, where
   * a `Drop` choice can never actually apply — disables the `d` shortcut
   * entirely, the
   * same rule `MergeFieldRow`'s own Drop button follows.
   */
  isPatchCollision?: MaybeRefOrGetter<boolean>;
  /** `q`/`Esc` (outside an edit): back to the inbox. */
  onExit: () => void;
  /** `?`: toggle the shortcut help overlay. */
  onToggleHelp: () => void;
}

/**
 * Registers the merge editor's keyboard shortcuts for as long as the calling component is mounted. Row
 * and column cursor state live in `useMergeStore`, matching
 * `useInboxKeys`'s own convention. The `e`/edit flow is split in two: this
 * composable only *starts* an edit (`editingPath`/`editingDraft`, which the
 * row component watches to focus its input) — saving or canceling is the
 * input's own `Enter`/`Esc` handling (`commitEdit`/`cancelEdit`), never the
 * page-level `window` listener here, since focus is inside that input by
 * then and {@link isInert} already leaves it alone.
 */
export function useMergeKeys(options: UseMergeKeysOptions) {
  const store = useMergeStore();
  const editingPath = ref<FieldPath | null>(null);
  const editingDraft = ref("");

  function currentRow(): MergeFieldDto | undefined {
    return toValue(options.rows)[store.rowCursor];
  }

  function moveRow(delta: number): void {
    const rows = toValue(options.rows);
    const maxIndex = Math.max(rows.length - 1, 0);
    store.setRowCursor(Math.min(Math.max(store.rowCursor + delta, 0), maxIndex));
  }

  /** `c`/`C`: jump to the next/previous `Conflict`-classified row, if any. */
  function jumpToConflict(direction: 1 | -1): void {
    const rows = toValue(options.rows);
    for (
      let index = store.rowCursor + direction;
      index >= 0 && index < rows.length;
      index += direction
    ) {
      if (rows[index]?.class === "conflict") {
        store.setRowCursor(index);
        return;
      }
    }
  }

  /** The number of choosable columns: one per owner (column 0 = base) plus the display-only "result" column. */
  function columnCount(): number {
    return toValue(options.owners).length + 1;
  }

  function moveColumn(delta: number): void {
    const maxIndex = Math.max(columnCount() - 1, 0);
    store.setColumnCursor(Math.min(Math.max(store.columnCursor + delta, 0), maxIndex));
  }

  /** Sends `From(modId)` for the row at the cursor. */
  function chooseOwner(modId: string): Promise<MergeStateDto> | undefined {
    const row = currentRow();
    if (!row) {
      return undefined;
    }
    return options.choices.setChoice(asFieldPath(row.path), { choice: "from", modId });
  }

  /** `Enter`/`Space`: choose the column at the cursor — a no-op on the trailing "result" column. */
  function chooseColumn(): Promise<MergeStateDto> | undefined {
    const owners = toValue(options.owners);
    const owner = owners[store.columnCursor];
    return owner ? chooseOwner(owner.modId) : undefined;
  }

  /** `1`..`9`: choose owner column `n` (1-based; `1` is the base, `owners[0]`). */
  function chooseDigit(oneBasedIndex: number): Promise<MergeStateDto> | undefined {
    const owner = toValue(options.owners)[oneBasedIndex - 1];
    return owner ? chooseOwner(owner.modId) : undefined;
  }

  /** `w`: choose the selected-order winner. */
  function chooseWinner(): Promise<MergeStateDto> | undefined {
    return chooseOwner(toValue(options.winner));
  }

  /**
   * `d`: toggles `Drop` for `path`, or the row at the cursor when omitted
   * — the same action `MergeFieldRow`'s own mouse-driven drop button
   * triggers by passing its row's path explicitly, so the page never
   * needs a second copy of this logic.
   */
  function toggleDrop(path?: FieldPath): Promise<MergeStateDto> | undefined {
    if (toValue(options.isPatchCollision ?? false)) {
      return undefined;
    }
    const row =
      path === undefined
        ? currentRow()
        : toValue(options.rows).find((candidate) => asFieldPath(candidate.path) === path);
    if (!row) {
      return undefined;
    }
    const targetPath = path ?? asFieldPath(row.path);
    return row.choice?.choice === "drop"
      ? options.choices.clearChoice(targetPath)
      : options.choices.setChoice(targetPath, { choice: "drop" });
  }

  /** `a`: reverts the row at the cursor back to its automatic result. */
  function revertToAuto(): Promise<MergeStateDto> | undefined {
    const row = currentRow();
    return row ? options.choices.clearChoice(asFieldPath(row.path)) : undefined;
  }

  /** `e`: begins editing the row at the cursor's free value. */
  function beginEdit(): void {
    const row = currentRow();
    if (!row) {
      return;
    }
    editingDraft.value = row.choice?.choice === "value" ? row.choice.text : (row.result ?? "");
    editingPath.value = asFieldPath(row.path);
  }

  /** Saves the in-progress edit as a `Value` choice — the input's own `Enter`. */
  function commitEdit(text: string): Promise<MergeStateDto> | undefined {
    const path = editingPath.value;
    if (path === null) {
      return undefined;
    }
    editingPath.value = null;
    return options.choices.setChoice(path, { choice: "value", text });
  }

  /** Discards the in-progress edit — the input's own `Esc`. */
  function cancelEdit(): void {
    editingPath.value = null;
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (isInert(event.target)) {
      return;
    }
    // A focused radio group moves its own selection on the arrows; every other key is ours.
    if (event.key.startsWith("Arrow") && ownsArrowKeys(event.target)) {
      return;
    }

    if (/^[1-9]$/.test(event.key)) {
      event.preventDefault();
      void chooseDigit(Number(event.key));
      return;
    }

    switch (event.key) {
      case "j":
      case "ArrowDown":
        event.preventDefault();
        moveRow(1);
        break;
      case "k":
      case "ArrowUp":
        event.preventDefault();
        moveRow(-1);
        break;
      case "c":
        event.preventDefault();
        jumpToConflict(1);
        break;
      case "C":
        event.preventDefault();
        jumpToConflict(-1);
        break;
      case "h":
      case "ArrowLeft":
        event.preventDefault();
        moveColumn(-1);
        break;
      case "l":
      case "ArrowRight":
        event.preventDefault();
        moveColumn(1);
        break;
      case "Enter":
      case " ":
        if (ownsEnterNatively(event.target)) {
          break;
        }
        event.preventDefault();
        void chooseColumn();
        break;
      case "w":
        event.preventDefault();
        void chooseWinner();
        break;
      case "e":
        event.preventDefault();
        beginEdit();
        break;
      case "d":
        event.preventDefault();
        void toggleDrop();
        break;
      case "a":
        event.preventDefault();
        void revertToAuto();
        break;
      case "x":
        event.preventDefault();
        store.toggleAuto();
        break;
      case "X":
        event.preventDefault();
        store.toggleUnchanged();
        break;
      case "Escape":
      case "q":
        event.preventDefault();
        options.onExit();
        break;
      case "?":
        event.preventDefault();
        options.onToggleHelp();
        break;
      default:
        break;
    }
  }

  onMounted(() => {
    window.addEventListener("keydown", handleKeydown);
  });
  onScopeDispose(() => {
    window.removeEventListener("keydown", handleKeydown);
  });

  return {
    handleKeydown,
    editingPath,
    editingDraft,
    moveRow,
    jumpToConflict,
    moveColumn,
    chooseColumn,
    chooseDigit,
    chooseWinner,
    toggleDrop,
    revertToAuto,
    beginEdit,
    commitEdit,
    cancelEdit,
  };
}
