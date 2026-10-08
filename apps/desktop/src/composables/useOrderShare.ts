import { useToast } from "primevue/usetoast";
import { ref } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useAdoptImportedSession } from "@/queries/orderShare";
import { pickModListFile, pickModListSavePath } from "@/services/dialogs";
import {
  exportOrderFile,
  exportOrderText,
  importOrder,
  openWorkshopPage,
  previewOrderImportFile,
  previewOrderImportText,
  RimmergeError,
  suggestedModListPath,
} from "@/services/ipc";
import type { ExportTextDto } from "@/types/generated/ExportTextDto";
import type { OrderImportOutcomeDto } from "@/types/generated/OrderImportOutcomeDto";
import type { ProjectSummaryDto } from "@/types/generated/ProjectSummaryDto";
import { describeCommandError } from "@/utils/errors";
import { ORDER_SHARE_TOAST_GROUP } from "@/utils/orderShare";
import { ruleWarningsDetail } from "@/utils/ruleWarning";

const TOAST_LIFE_MS = 6000;

/** The file name of `path`, either separator, for a toast. */
function fileNameOf(path: string): string {
  return path.split(/[\\/]/).at(-1) ?? path;
}

/**
 * Exporting and importing a load order from the Load order page: the four entry points
 * (save a `.rml`, copy as text, open a file, paste), the preview they lead to and the import.
 *
 * Export, preview and import call the services directly rather than through mutations: the
 * app-wide mutation `onError` would toast every failure with its generic wording, in the
 * default toast group, while an export failure is worded "couldn't save/copy" with the code's
 * sentence beneath and an outdated import has its own hint, each shown once here, in the
 * order-share group. The import's success path (adopt `selected`, invalidate every query)
 * is `useAdoptImportedSession`.
 *
 * `isBusy` covers a file dialog, an export or a preview in flight; every entry point is a
 * no-op while it is set, so a double click cannot start two.
 */
export function useOrderShare() {
  const { t, locale } = useI18n();
  const tm = useTranslateMessage();
  const toast = useToast();
  const adoptImportedSession = useAdoptImportedSession();

  const isBusy = ref(false);
  const isPasteOpen = ref(false);
  const isPreviewOpen = ref(false);
  const outcome = ref<OrderImportOutcomeDto | null>(null);

  const isImporting = ref(false);

  /** `summary` names what failed; without one the code's own title leads. */
  function toastFailure(summary: string | null, error: unknown): void {
    const described = describeCommandError(error);
    toast.add({
      group: ORDER_SHARE_TOAST_GROUP,
      severity: "error",
      summary: summary ?? tm(described.title),
      detail:
        summary === null ? tm(described.detail) : `${tm(described.title)}: ${tm(described.detail)}`,
      life: TOAST_LIFE_MS,
    });
  }

  /** Runs one exclusive action, clearing `isBusy` however it ends. */
  async function exclusive(action: () => Promise<void>): Promise<void> {
    if (isBusy.value) {
      return;
    }
    isBusy.value = true;
    try {
      await action();
    } finally {
      isBusy.value = false;
    }
  }

  function toastLeftOut(unrepresentable: readonly string[]): void {
    if (unrepresentable.length === 0) {
      return;
    }
    toast.add({
      group: ORDER_SHARE_TOAST_GROUP,
      severity: "warn",
      summary: t(
        "orderShare.export.leftOut",
        { count: unrepresentable.length },
        unrepresentable.length,
      ),
      life: TOAST_LIFE_MS,
    });
  }

  async function saveAsFile(): Promise<void> {
    await exclusive(async () => {
      try {
        const defaultPath = await suggestedModListPath();
        const path = await pickModListSavePath({
          filterName: t("orderShare.export.fileFilter"),
          defaultPath,
        });
        if (path === null) {
          return;
        }
        const result = await exportOrderFile({ path });
        toast.add({
          group: ORDER_SHARE_TOAST_GROUP,
          severity: "success",
          summary: t(
            "orderShare.export.saved",
            { count: result.count, file: fileNameOf(path) },
            result.count,
          ),
          life: TOAST_LIFE_MS,
        });
        toastLeftOut(result.unrepresentable);
      } catch (error: unknown) {
        toastFailure(t("orderShare.export.saveFailed"), error);
      }
    });
  }

  /** Writes `text` to the clipboard; a blocked clipboard is toasted here and answers `false`. */
  async function writeToClipboard(text: string): Promise<boolean> {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // The webview can block clipboard access; say so rather than claim a copy.
      toast.add({
        group: ORDER_SHARE_TOAST_GROUP,
        severity: "error",
        summary: t("orderShare.export.copyFailed"),
        life: TOAST_LIFE_MS,
      });
      return false;
    }
  }

  async function copyAsText(): Promise<void> {
    await exclusive(async () => {
      let result: ExportTextDto;
      try {
        result = await exportOrderText();
      } catch (error: unknown) {
        toastFailure(t("orderShare.export.copyFailed"), error);
        return;
      }
      if (!(await writeToClipboard(result.text))) {
        return;
      }
      toast.add({
        group: ORDER_SHARE_TOAST_GROUP,
        severity: "success",
        summary: t("orderShare.export.copied", { count: result.count }, result.count),
        life: TOAST_LIFE_MS,
      });
      toastLeftOut(result.unrepresentable);
    });
  }

  function showPreview(result: OrderImportOutcomeDto): void {
    outcome.value = result;
    isPasteOpen.value = false;
    isPreviewOpen.value = true;
  }

  async function openFile(): Promise<void> {
    await exclusive(async () => {
      try {
        const path = await pickModListFile({
          modLists: t("orderShare.import.fileFilter"),
          allFiles: t("orderShare.import.allFiles"),
        });
        if (path === null) {
          return;
        }
        showPreview(await previewOrderImportFile({ path }));
      } catch (error: unknown) {
        toastFailure(null, error);
      }
    });
  }

  function openPaste(): void {
    isPasteOpen.value = true;
  }

  async function previewPasted(text: string): Promise<void> {
    await exclusive(async () => {
      try {
        showPreview(await previewOrderImportText({ text }));
      } catch (error: unknown) {
        toastFailure(null, error);
      }
    });
  }

  /**
   * A failed import is toasted here, once. A refused order (`invalid_input`) means the install
   * changed since the preview, so the same preview would be refused again: it closes and a
   * hint asks for a fresh one. Any other failure keeps the dialog open so the same preview
   * can be retried or dismissed.
   */
  function handleImportFailure(error: unknown): void {
    if (!(error instanceof RimmergeError && error.code === "invalid_input")) {
      toastFailure(null, error);
      return;
    }
    isPreviewOpen.value = false;
    toast.add({
      group: ORDER_SHARE_TOAST_GROUP,
      severity: "info",
      summary: t("orderShare.previewOutdated"),
      detail: tm(describeCommandError(error).detail),
      life: TOAST_LIFE_MS,
    });
  }

  /** Runs the import for the open preview's `order`; the preview closes only when it lands. */
  async function confirmImport(order: readonly string[]): Promise<void> {
    if (isImporting.value) {
      return;
    }
    isImporting.value = true;
    try {
      const summary = await importOrder({ order: [...order] });
      adoptImportedSession(summary);
      isPreviewOpen.value = false;
      toast.add({
        group: ORDER_SHARE_TOAST_GROUP,
        severity: "success",
        summary: t("orderShare.imported"),
        life: TOAST_LIFE_MS,
      });
      toastScanWarnings(summary);
    } catch (error: unknown) {
      handleImportFailure(error);
    } finally {
      isImporting.value = false;
    }
  }

  /** The import is a scan like a rescan, so its notes and rules warnings are shown the same way. */
  function toastScanWarnings(summary: ProjectSummaryDto): void {
    if (summary.warnings.length > 0) {
      toast.add({
        group: ORDER_SHARE_TOAST_GROUP,
        severity: "info",
        summary: t(
          "orderShare.importedWithWarnings",
          { count: summary.warnings.length },
          summary.warnings.length,
        ),
        life: 10_000,
      });
    }
    if (summary.ruleWarnings.length > 0) {
      toast.add({
        group: ORDER_SHARE_TOAST_GROUP,
        severity: "warn",
        summary: t("orderShare.importedRuleWarnings"),
        detail: ruleWarningsDetail(summary.ruleWarnings, t, locale.value),
        life: 10_000,
      });
    }
  }

  async function openWorkshop(workshopId: number): Promise<void> {
    try {
      await openWorkshopPage({ workshopId: String(workshopId) });
    } catch (error: unknown) {
      toastFailure(null, error);
    }
  }

  async function copyMissing(text: string): Promise<void> {
    if (!(await writeToClipboard(text))) {
      return;
    }
    toast.add({
      group: ORDER_SHARE_TOAST_GROUP,
      severity: "success",
      summary: t("orderShare.preview.missingCopied"),
      life: TOAST_LIFE_MS,
    });
  }

  return {
    isBusy,
    isImporting,
    isPasteOpen,
    isPreviewOpen,
    outcome,
    saveAsFile,
    copyAsText,
    openFile,
    openPaste,
    previewPasted,
    confirmImport,
    openWorkshop,
    copyMissing,
  };
}
