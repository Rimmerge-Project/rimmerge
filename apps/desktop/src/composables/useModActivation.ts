import { useQueryCache } from "@pinia/colada";
import { useToast } from "primevue/usetoast";
import { type MaybeRefOrGetter, type Ref, ref, toValue } from "vue";
import { useI18n } from "vue-i18n";

import { useActivateModsMutation, usePlanActivateModsMutation } from "@/queries/activeSet";
import { startInvalidation } from "@/queries/invalidation";
import { RimmergeError } from "@/services/ipc";
import type { ActivatePlanDto } from "@/types/generated/ActivatePlanDto";
import type { ActivateRequestDto } from "@/types/generated/ActivateRequestDto";
import type { AnchorDto } from "@/types/generated/AnchorDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";
import { isPlainActivation, type PlanMode } from "@/utils/modMove";

/**
 * The Mods page's activate flow: plan the inactive mods, show the plan in a confirm dialog, then
 * commit. Two entry points share it. {@link openDialog} is the button: it plans the selected
 * mods and always shows the dialog. {@link activateAt} is a drop at a position: it plans with the
 * anchor and commits straight away when the plan adds exactly the dropped mods with no warning,
 * and otherwise opens the same dialog, whose confirm then commits the same anchored request.
 *
 * A successful commit closes the dialog, takes the committed mods out of `selectedIds` and
 * toasts; a failed one keeps both and shows the error in the dialog. A failed plan opens the
 * dialog too, with the error and no plan. A stale anchor refreshes the lists, which the error
 * text promises.
 */
export function useModActivation({
  selectedIds,
  withDependencies,
  onSwitchedToCurrent,
}: {
  /** The inactive mods the button activates; the committed ones leave it after a commit. */
  selectedIds: Ref<string[]>;
  /** Whether the plan and the commit also pull in declared dependencies. */
  withDependencies: MaybeRefOrGetter<boolean>;
  /** Called after a commit that switched the selected order from Suggested to Current. */
  onSwitchedToCurrent: () => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const queryCache = useQueryCache();
  const { mutateAsync: planActivate, isLoading: isPlanning } = usePlanActivateModsMutation();
  const { mutateAsync: commitActivate, isLoading: isCommitting } = useActivateModsMutation();
  const plan = ref<ActivatePlanDto | null>(null);
  const isDialogVisible = ref(false);
  const error = ref<CommandErrorDescriptor | null>(null);
  /** What the dialog's confirm commits: the request that was planned. */
  const planned = ref<ActivateRequestDto | null>(null);

  function showFailure(caught: unknown): void {
    if (caught instanceof RimmergeError && caught.code === "move_anchor_not_active") {
      startInvalidation(queryCache);
    }
    error.value = describeCommandError(caught);
  }

  async function commit(request: ActivateRequestDto): Promise<void> {
    const result = await commitActivate(request);
    isDialogVisible.value = false;
    const committed = new Set(request.ids);
    selectedIds.value = selectedIds.value.filter((id) => !committed.has(id));
    toast.add({ severity: "success", summary: t("mods.page.activatedToast"), life: 4000 });
    if (result.didSwitchToCurrent) {
      onSwitchedToCurrent();
    }
  }

  async function planAndAsk(
    ids: string[],
    anchor: AnchorDto | null,
    mode: PlanMode,
  ): Promise<void> {
    error.value = null;
    const request: ActivateRequestDto = {
      ids,
      withDependencies: toValue(withDependencies),
      anchor,
    };
    try {
      const result = await planActivate(request);
      if (mode === "commitIfPlain" && isPlainActivation(result, ids)) {
        await commit(request);
        return;
      }
      planned.value = request;
      plan.value = result;
      isDialogVisible.value = true;
    } catch (caught: unknown) {
      // The dialog opens with no plan so the failure is shown where the user
      // is looking; its confirm button stays disabled until a plan exists.
      planned.value = null;
      plan.value = null;
      showFailure(caught);
      isDialogVisible.value = true;
    }
  }

  /** The button: plans the selected mods at their default place and opens the dialog. */
  function openDialog(): Promise<void> {
    return planAndAsk([...selectedIds.value], null, "ask");
  }

  /** The mods of a group: plans exactly `ids` at their default place and opens the dialog. */
  function openForIds(ids: string[]): Promise<void> {
    return planAndAsk([...ids], null, "ask");
  }

  /** A drop of `ids` at `anchor` (`null`: the default place). */
  function activateAt(ids: string[], anchor: AnchorDto | null): Promise<void> {
    return planAndAsk(ids, anchor, "commitIfPlain");
  }

  async function confirm(): Promise<void> {
    const request = planned.value;
    if (request === null) {
      return;
    }
    error.value = null;
    try {
      await commit(request);
    } catch (caught: unknown) {
      showFailure(caught);
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
    activateAt,
    confirm,
  };
}
