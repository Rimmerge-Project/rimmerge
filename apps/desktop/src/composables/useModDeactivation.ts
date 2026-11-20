import { useToast } from "primevue/usetoast";
import { type Ref, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useDeactivateModsMutation, usePlanDeactivateModsMutation } from "@/queries/activeSet";
import type { DeactivatePlanDto } from "@/types/generated/DeactivatePlanDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";
import { isPlainDeactivation, type PlanMode } from "@/utils/modMove";

/**
 * The Mods page's deactivate flow: plan the active mods, show the plan (dependents still active,
 * refused mods) in a confirm dialog, then commit. {@link openDialog} is the button (the selected
 * mods, always the dialog); {@link deactivateDropped} is a drop into the Inactive list, which
 * commits straight away when the plan removes exactly the dropped mods with no warning and
 * otherwise opens the same dialog.
 *
 * A successful commit closes the dialog, takes the committed mods out of `selectedIds` and
 * toasts; a failed one keeps both and shows the error in the dialog.
 */
export function useModDeactivation({
  selectedIds,
}: {
  /** The active mods the button deactivates; the committed ones leave it after a commit. */
  selectedIds: Ref<string[]>;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const { mutateAsync: planDeactivate, isLoading: isPlanning } = usePlanDeactivateModsMutation();
  const { mutateAsync: commitDeactivate, isLoading: isCommitting } = useDeactivateModsMutation();
  const plan = ref<DeactivatePlanDto | null>(null);
  const isDialogVisible = ref(false);
  const error = ref<CommandErrorDescriptor | null>(null);
  /** What the dialog's confirm commits: the ids that were planned. */
  const plannedIds = ref<string[]>([]);

  async function commit(ids: string[]): Promise<void> {
    await commitDeactivate({ ids });
    isDialogVisible.value = false;
    const committed = new Set(ids);
    selectedIds.value = selectedIds.value.filter((id) => !committed.has(id));
    toast.add({ severity: "success", summary: t("mods.page.deactivatedToast"), life: 4000 });
  }

  async function planAndAsk(ids: string[], mode: PlanMode): Promise<void> {
    error.value = null;
    try {
      const result = await planDeactivate({ ids });
      if (mode === "commitIfPlain" && isPlainDeactivation(result, ids)) {
        await commit(ids);
        return;
      }
      plannedIds.value = ids;
      plan.value = result;
      isDialogVisible.value = true;
    } catch (caught: unknown) {
      // The dialog opens with no plan so the failure is shown where the user
      // is looking; its confirm button stays disabled until a plan exists.
      plannedIds.value = [];
      plan.value = null;
      error.value = describeCommandError(caught);
      isDialogVisible.value = true;
    }
  }

  /** The button: plans the selected mods and opens the dialog. */
  function openDialog(): Promise<void> {
    return planAndAsk([...selectedIds.value], "ask");
  }

  /** The mods of a group: plans exactly `ids` and opens the dialog. */
  function openForIds(ids: string[]): Promise<void> {
    return planAndAsk([...ids], "ask");
  }

  /** A drop of `ids` into the Inactive list. */
  function deactivateDropped(ids: string[]): Promise<void> {
    return planAndAsk(ids, "commitIfPlain");
  }

  async function confirm(): Promise<void> {
    error.value = null;
    try {
      await commit(plannedIds.value);
    } catch (caught: unknown) {
      error.value = describeCommandError(caught);
    }
  }

  return {
    plan,
    isDialogVisible,
    error,
    isPlanning,
    isCommitting,
    openDialog,
    openForIds,
    deactivateDropped,
    confirm,
  };
}
