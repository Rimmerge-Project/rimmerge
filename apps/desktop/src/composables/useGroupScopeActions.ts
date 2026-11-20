import { useToast } from "primevue/usetoast";
import { type MaybeRefOrGetter, ref, toValue } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { planGroupScope } from "@/services/ipc";
import { describeCommandError } from "@/utils/errors";
import { type GroupScopeAction, groupScopeLines, scopeIds } from "@/utils/groupScopeText";

/**
 * "Activate all" and "Deactivate all" on a group: plan the group (and its subgroups) against the
 * install, then hand exactly those mods to the existing Activate or Deactivate dialog, with the
 * lines that say how far the action reaches. Nothing is applied here: the dialogs are the same
 * pending-change flow as the selection buttons, and nothing is written until Apply.
 *
 * The mods come from the backend plan, which counts each mod once however many subgroups hold
 * it. The filter-hidden count is read from the rows each list shows right now.
 */
export function useGroupScopeActions({
  openActivate,
  openDeactivate,
  shownInactiveIds,
  shownActiveIds,
}: {
  /** Opens the Activate dialog over exactly these mods. */
  openActivate: (ids: string[]) => Promise<void>;
  /** Opens the Deactivate dialog over exactly these mods. */
  openDeactivate: (ids: string[]) => Promise<void>;
  /** The mods the Inactive list shows now. */
  shownInactiveIds: MaybeRefOrGetter<ReadonlySet<string>>;
  /** The mods the Active list shows now. */
  shownActiveIds: MaybeRefOrGetter<ReadonlySet<string>>;
}) {
  const { t } = useI18n();
  const tm = useTranslateMessage();
  const toast = useToast();
  const activateLines = ref<string[]>([]);
  const deactivateLines = ref<string[]>([]);
  const isPlanning = ref(false);

  /** Forgets a group's scope lines, for a dialog opened from a selection instead. */
  function clearScope(action: GroupScopeAction): void {
    (action === "activate" ? activateLines : deactivateLines).value = [];
  }

  function say(severity: "info" | "error", summary: string, detail?: string): void {
    toast.add({ severity, summary, ...(detail === undefined ? {} : { detail }), life: 6000 });
  }

  async function start(groupId: number, action: GroupScopeAction): Promise<void> {
    if (isPlanning.value) {
      return;
    }
    isPlanning.value = true;
    try {
      const plan = await planGroupScope({ id: groupId });
      const ids = scopeIds(plan, action);
      if (ids.length === 0) {
        say(
          "info",
          t(
            action === "activate"
              ? "mods.bulk.scope.nothingToActivate"
              : "mods.bulk.scope.nothingToDeactivate",
          ),
        );
        return;
      }
      const shown = toValue(action === "activate" ? shownInactiveIds : shownActiveIds);
      const hiddenCount = ids.filter((id) => !shown.has(id)).length;
      const lines = groupScopeLines(t, plan, action, hiddenCount);
      (action === "activate" ? activateLines : deactivateLines).value = lines;
      await (action === "activate" ? openActivate(ids) : openDeactivate(ids));
    } catch (caught: unknown) {
      const failure = describeCommandError(caught);
      say("error", tm(failure.title), tm(failure.detail));
    } finally {
      isPlanning.value = false;
    }
  }

  return {
    activateLines,
    deactivateLines,
    isPlanning,
    clearScope,
    activateGroup: (groupId: number) => start(groupId, "activate"),
    deactivateGroup: (groupId: number) => start(groupId, "deactivate"),
  };
}
