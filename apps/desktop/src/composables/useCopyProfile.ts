import { useQueryCache } from "@pinia/colada";
import { computed, ref } from "vue";

import { awaitInvalidation } from "@/queries/invalidation";
import { queryKeys } from "@/queries/keys";
import { pickFolder } from "@/services/dialogs";
import { copyOrganization, previewCopyOrganization } from "@/services/ipc";
import { useSessionStore } from "@/stores/session";
import type { CopyAnswerDto } from "@/types/generated/CopyAnswerDto";
import type { CopyModeDto } from "@/types/generated/CopyModeDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";
import { parentFolder } from "@/utils/organizationCopy";

/** Where the copy flow is: pick a folder, look at the preview, confirm a replace, read the result. */
export type CopyStep = "pick" | "preview" | "confirmReplace" | "done";

/**
 * The flow behind "Copy from another profile": the folder picker, the preview for the chosen mode,
 * the copy and its result. The backend decides everything shown (counts, clashes, refusals); this
 * only sequences the calls. A refusal is an answer kept in `answer`, a failure an `error`. The
 * copy is planned again by the backend from the folder as it is then, never from the preview, and
 * changes organization only (the organization lists are refetched; nothing else is invalidated).
 */
export function useCopyProfile() {
  const queryCache = useQueryCache();
  const session = useSessionStore();
  const step = ref<CopyStep>("pick");
  const sourceDir = ref<string | null>(null);
  const mode = ref<CopyModeDto>("merge");
  const answer = ref<CopyAnswerDto | null>(null);
  const error = ref<CommandErrorDescriptor | null>(null);
  // A preview is loading: the mode may still change (the newest choice wins by serial).
  const isLoadingPreview = ref(false);
  // A copy is running: the dialog may not be closed or reopened under it.
  const isCopying = ref(false);
  // A call that was overtaken by a newer choice, a copy or a reset must not write its result.
  let serial = 0;

  /** Back to the first step, forgetting the folder (the dialog opened again). */
  function reset(): void {
    serial += 1;
    step.value = "pick";
    sourceDir.value = null;
    mode.value = "merge";
    answer.value = null;
    error.value = null;
    isLoadingPreview.value = false;
    isCopying.value = false;
  }

  async function loadPreview(): Promise<void> {
    const dir = sourceDir.value;
    if (dir === null) {
      return;
    }
    serial += 1;
    const mine = serial;
    error.value = null;
    isLoadingPreview.value = true;
    try {
      const result = await previewCopyOrganization({ sourceDir: dir, mode: mode.value });
      if (mine === serial) {
        answer.value = result;
      }
    } catch (caught: unknown) {
      if (mine === serial) {
        answer.value = null;
        error.value = describeCommandError(caught);
      }
    } finally {
      if (mine === serial) {
        isLoadingPreview.value = false;
      }
    }
  }

  /**
   * Opens the native folder picker, starting beside this profile's own folder (profiles sit side by
   * side); a chosen folder is previewed at once. Cancelling changes nothing.
   */
  async function choose(): Promise<void> {
    const profileDir = session.paths?.profileDir;
    const startAt = profileDir === undefined ? null : parentFolder(profileDir);
    const picked = await pickFolder(startAt ?? undefined);
    if (picked === null) {
      return;
    }
    sourceDir.value = picked;
    answer.value = null;
    step.value = "preview";
    await loadPreview();
  }

  async function setMode(next: CopyModeDto): Promise<void> {
    if (next === mode.value) {
      return;
    }
    mode.value = next;
    answer.value = null;
    await loadPreview();
  }

  /** The copy button: a replace asks to be confirmed first, a merge runs. */
  async function requestCopy(): Promise<void> {
    if (mode.value === "replace") {
      step.value = "confirmReplace";
      return;
    }
    await runCopy();
  }

  async function runCopy(): Promise<void> {
    const dir = sourceDir.value;
    if (dir === null) {
      return;
    }
    serial += 1;
    const mine = serial;
    error.value = null;
    isLoadingPreview.value = false;
    isCopying.value = true;
    try {
      const result = await copyOrganization({ sourceDir: dir, mode: mode.value });
      if (mine !== serial) {
        return;
      }
      answer.value = result;
      if (result.kind === "report") {
        // The copy is committed: show the result and let the dialog close at once, so a slow
        // refetch of the lists cannot lock it.
        step.value = "done";
        isCopying.value = false;
        await awaitInvalidation(
          queryCache,
          { key: [...queryKeys.organization()] },
          { key: [...queryKeys.workingModsAll()] },
          { key: [...queryKeys.inactiveModsAll()] },
        );
      } else {
        // Refused at the copy (the folder changed since the preview): show why, nothing changed.
        step.value = "preview";
      }
    } catch (caught: unknown) {
      if (mine === serial) {
        error.value = describeCommandError(caught);
        step.value = "preview";
      }
    } finally {
      if (mine === serial) {
        isCopying.value = false;
      }
    }
  }

  const canCopy = computed(
    () =>
      !isLoadingPreview.value &&
      !isCopying.value &&
      answer.value !== null &&
      answer.value.kind === "report",
  );

  return {
    step,
    sourceDir,
    mode,
    answer,
    error,
    isLoadingPreview,
    isCopying,
    canCopy,
    reset,
    choose,
    setMode,
    requestCopy,
    runCopy,
  };
}
