import { type Ref, ref, watch } from "vue";

import { RimmergeError } from "@/services/ipc";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

/**
 * The out-dir/install/force-confirm/last-outcome state machine every
 * export panel in this app shares (`components/patches/PatchExportPanel.vue`'s
 * own `ApplyDialog`-style running-game refusal shape), so the assignment
 * export panel doesn't hand-roll a second copy of it.
 * `PatchExportPanel.vue` itself still carries its own copy of this state
 * machine.
 *
 * `runExport` is the caller's own IPC call (`exportPatch`/`exportAssignment`);
 * this composable only manages the surrounding state — directory text,
 * the install checkbox, the pending-force-confirm flag, the last
 * successful outcome, and the display error message.
 *
 * `buildRequest` takes the current `outDir` text explicitly (rather than
 * closing over this composable's own returned ref) so a caller never
 * needs to reference its own destructured `outDir` from inside the very
 * callback passed into this call — that would be a circular initializer
 * TypeScript rightly refuses (`outDir` has no type yet at the point the
 * callback is defined).
 */
export function useExportFlow<TRequest extends { install: boolean; force: boolean }, TOutcome>(
  buildRequest: (outDir: string, overrides: { install: boolean; force: boolean }) => TRequest,
  runExport: (request: TRequest) => Promise<TOutcome>,
  initialOutDir: Ref<string | null>,
) {
  const outDir = ref(initialOutDir.value ?? "");
  watch(initialOutDir, (value) => {
    outDir.value = value ?? "";
  });

  const install = ref(false);
  const awaitingForceConfirm = ref(false);
  const errorMessage = ref<CommandErrorDescriptor | null>(null);
  const lastOutcome = ref<TOutcome | null>(null);
  const isExporting = ref(false);

  watch(install, (value) => {
    if (!value) {
      awaitingForceConfirm.value = false;
    }
  });

  async function submit(force: boolean): Promise<void> {
    errorMessage.value = null;
    isExporting.value = true;
    try {
      const outcome = await runExport(
        buildRequest(outDir.value, { install: install.value, force }),
      );
      awaitingForceConfirm.value = false;
      lastOutcome.value = outcome;
    } catch (error: unknown) {
      if (!force && error instanceof RimmergeError && error.code === "rimworld_running") {
        awaitingForceConfirm.value = true;
        return;
      }
      errorMessage.value = describeCommandError(error);
    } finally {
      isExporting.value = false;
    }
  }

  function doExport(): void {
    void submit(false);
  }

  function exportAnyway(): void {
    void submit(true);
  }

  /** Enter in the directory field: a no-op while empty, "export anyway" once a running-game refusal is pending, a plain attempt otherwise. */
  function handleOutDirEnter(): void {
    if (outDir.value.trim().length === 0) {
      return;
    }
    if (awaitingForceConfirm.value) {
      exportAnyway();
    } else {
      doExport();
    }
  }

  return {
    outDir,
    install,
    awaitingForceConfirm,
    errorMessage,
    lastOutcome,
    isExporting,
    doExport,
    exportAnyway,
    handleOutDirEnter,
  };
}
