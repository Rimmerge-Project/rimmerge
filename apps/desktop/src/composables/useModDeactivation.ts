import { useToast } from "primevue/usetoast";
import { type Ref, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useDeactivateModsMutation, usePlanDeactivateModsMutation } from "@/queries/activeSet";
import type { DeactivatePlanDto } from "@/types/generated/DeactivatePlanDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

/**
 * The Mods page's deactivate flow: plan the selected active mods, show
 * the plan (dependents still active, refused mods) in a confirm dialog,
 * then commit. A successful commit closes the dialog, clears
 * `selectedIds` and toasts; a failed one keeps both and shows the error
 * in the dialog.
 */
export function useModDeactivation({
  selectedIds,
}: {
  /** The active mods to deactivate; cleared after a successful commit. */
  selectedIds: Ref<string[]>;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const { mutateAsync: planDeactivate, isLoading: isPlanning } = usePlanDeactivateModsMutation();
  const { mutateAsync: commitDeactivate, isLoading: isCommitting } = useDeactivateModsMutation();
  const plan = ref<DeactivatePlanDto | null>(null);
  const isDialogVisible = ref(false);
  const error = ref<CommandErrorDescriptor | null>(null);

  async function openDialog(): Promise<void> {
    error.value = null;
    try {
      plan.value = await planDeactivate({ ids: selectedIds.value });
      isDialogVisible.value = true;
    } catch (caught: unknown) {
      // The dialog opens with no plan so the failure is shown where the user
      // is looking; its confirm button stays disabled until a plan exists.
      plan.value = null;
      error.value = describeCommandError(caught);
      isDialogVisible.value = true;
    }
  }

  async function confirm(): Promise<void> {
    error.value = null;
    try {
      await commitDeactivate({ ids: selectedIds.value });
      isDialogVisible.value = false;
      selectedIds.value = [];
      toast.add({ severity: "success", summary: t("mods.page.deactivatedToast"), life: 4000 });
    } catch (caught: unknown) {
      error.value = describeCommandError(caught);
    }
  }

  return { plan, isDialogVisible, error, isPlanning, isCommitting, openDialog, confirm };
}
