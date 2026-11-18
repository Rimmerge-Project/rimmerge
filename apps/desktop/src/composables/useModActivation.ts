import { useToast } from "primevue/usetoast";
import { type MaybeRefOrGetter, type Ref, ref, toValue } from "vue";
import { useI18n } from "vue-i18n";

import { useActivateModsMutation, usePlanActivateModsMutation } from "@/queries/activeSet";
import type { ActivatePlanDto } from "@/types/generated/ActivatePlanDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

/**
 * The Mods page's activate flow: plan the selected inactive mods, show
 * the plan in a confirm dialog, then commit. A successful commit closes
 * the dialog, clears `selectedIds` and toasts; a failed one keeps both
 * and shows the error in the dialog.
 */
export function useModActivation({
  selectedIds,
  withDependencies,
}: {
  /** The inactive mods to activate; cleared after a successful commit. */
  selectedIds: Ref<string[]>;
  /** Whether the plan and the commit also pull in declared dependencies. */
  withDependencies: MaybeRefOrGetter<boolean>;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const { mutateAsync: planActivate, isLoading: isPlanning } = usePlanActivateModsMutation();
  const { mutateAsync: commitActivate, isLoading: isCommitting } = useActivateModsMutation();
  const plan = ref<ActivatePlanDto | null>(null);
  const isDialogVisible = ref(false);
  const error = ref<CommandErrorDescriptor | null>(null);

  async function openDialog(): Promise<void> {
    error.value = null;
    try {
      plan.value = await planActivate({
        ids: selectedIds.value,
        withDependencies: toValue(withDependencies),
      });
      isDialogVisible.value = true;
    } catch (caught: unknown) {
      error.value = describeCommandError(caught);
    }
  }

  async function confirm(): Promise<void> {
    error.value = null;
    try {
      await commitActivate({
        ids: selectedIds.value,
        withDependencies: toValue(withDependencies),
      });
      isDialogVisible.value = false;
      selectedIds.value = [];
      toast.add({ severity: "success", summary: t("mods.page.activatedToast"), life: 4000 });
    } catch (caught: unknown) {
      error.value = describeCommandError(caught);
    }
  }

  return { plan, isDialogVisible, error, isPlanning, isCommitting, openDialog, confirm };
}
