import { useToast } from "primevue/usetoast";
import type { MaybeRefOrGetter } from "vue";
import { toValue } from "vue";
import { useI18n } from "vue-i18n";

import type { DragState } from "@/composables/useModReorder";
import { useOrganizationMutations } from "@/queries/organization";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { OrganizationEditOutcomeDto } from "@/types/generated/OrganizationEditOutcomeDto";
import { assertNever } from "@/utils/assertNever";
import { findNode } from "@/utils/organization";

/**
 * What a drop on the group tree does: mods dropped on a group are added to it (never moved out
 * of another group, D14), and a group dropped on another group, or on "All mods", is re-parented.
 * Both are organization edits only: they never touch the selection, the working set, the load
 * order or `ModsConfig.xml`. A refusal reaches the user through the app-wide mutation toast,
 * which words the backend's own error code.
 */
export function useGroupDrop({
  nodes,
  drag,
  endDrag,
  announce,
}: {
  /** Every group, depth first. */
  nodes: MaybeRefOrGetter<readonly GroupNodeDto[]>;
  /** The mod drag in progress, read at the moment of the drop. */
  drag: MaybeRefOrGetter<DragState | null>;
  /** Ends the mod drag (the source row may be gone, so its `dragend` cannot be relied on). */
  endDrag: () => void;
  /** Speaks a sentence through the page's polite live region. */
  announce: (text: string) => Promise<void>;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const edits = useOrganizationMutations();

  function say(summary: string, detail: string | undefined): void {
    toast.add({
      severity: "success",
      summary,
      ...(detail === undefined ? {} : { detail }),
      life: 4000,
    });
    void announce(detail === undefined ? summary : `${summary}. ${detail.replaceAll("\n", ". ")}`);
  }

  /** The detail lines of an add: mods already in the group, and selected mods the filter hides. */
  function addedDetail(already: number, hidden: number): string | undefined {
    const lines = [
      already > 0 ? t("mods.groups.drop.alreadyIn", { count: already }, already) : null,
      hidden > 0 ? t("mods.groups.drop.hiddenIncluded", { count: hidden }, hidden) : null,
    ].filter((line) => line !== null);
    return lines.length === 0 ? undefined : lines.join("\n");
  }

  /** The toast summary and detail for an add outcome (only an `added` outcome comes back). */
  function addedText(
    outcome: OrganizationEditOutcomeDto,
    name: string,
    hidden: number,
  ): [string, string | undefined] {
    switch (outcome.kind) {
      case "added":
        return [
          outcome.added === 0
            ? t("mods.groups.drop.nothingToAdd", { name })
            : t("mods.groups.drop.added", { count: outcome.added, name }, outcome.added),
          addedDetail(outcome.already, hidden),
        ];
      case "created":
      case "done":
      case "deleted":
      case "removed":
      case "moved":
      case "colored":
        throw new Error(`add_group_members answered with ${outcome.kind}`);
      default:
        return assertNever(outcome);
    }
  }

  /** Adds the dragged mods to `groupId`. */
  async function dropMods(groupId: number): Promise<void> {
    const current = toValue(drag);
    endDrag();
    const group = findNode(toValue(nodes), groupId);
    if (current === null || group === undefined) {
      return;
    }
    let outcome: OrganizationEditOutcomeDto;
    try {
      outcome = await edits.addMembers.mutateAsync({ id: groupId, modIds: [...current.ids] });
    } catch {
      // The app-wide mutation toast already shows the backend's reason.
      return;
    }
    say(...addedText(outcome, group.name, current.hiddenCount));
  }

  /** Moves group `groupId` under `parent` (`null` for the top level). */
  async function dropGroup(groupId: number, parent: number | null): Promise<void> {
    const group = findNode(toValue(nodes), groupId);
    if (group === undefined) {
      return;
    }
    try {
      await edits.reparentGroup.mutateAsync({ id: groupId, parent });
      const target = findNode(toValue(nodes), parent);
      say(
        target === undefined
          ? t("mods.groups.drop.movedTop", { name: group.name })
          : t("mods.groups.drop.movedInside", { name: group.name, parent: target.name }),
        undefined,
      );
    } catch {
      // The app-wide mutation toast already shows the backend's reason.
    }
  }

  return { dropMods, dropGroup };
}
