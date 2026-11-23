import { useToast } from "primevue/usetoast";
import { type MaybeRefOrGetter, type Ref, toValue } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { listInactiveMods, listWorkingMods } from "@/services/ipc";
import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";
import { describeCommandError } from "@/utils/errors";
import type { PaneKind } from "@/utils/groupsCollapse";
import { findNode } from "@/utils/organization";

/**
 * "Select all in folder" on a Groups view header. The mods of a folder are not worked out here:
 * the pane's own list query is asked again with its group filter set to that folder, so the
 * backend applies the pane's search, tag and source, counts a mod once however many subfolders hold
 * it, and ignores folds. The ids it returns are added to the pane's selection (nothing is
 * deselected), and the page's live region says how many the folder held.
 */
export function useFolderSelect({
  queries,
  selections,
  nodes,
  announce,
}: {
  /** Each pane's own list query. */
  queries: Readonly<Record<PaneKind, MaybeRefOrGetter<ModListQueryDto>>>;
  /** Each pane's selection, owned by the page. */
  selections: Readonly<Record<PaneKind, Ref<string[]>>>;
  nodes: MaybeRefOrGetter<readonly GroupNodeDto[]>;
  /** Speaks a sentence through the page's polite live region. */
  announce: (text: string) => Promise<void>;
}) {
  const { t } = useI18n();
  const tm = useTranslateMessage();
  const toast = useToast();

  /** The folder's name for the spoken result; a group the organization lacks is an error toast. */
  function folderName(folder: FolderKeyDto): string {
    if (folder.kind === "ungrouped") {
      return t("mods.groups.ungrouped");
    }
    const node = findNode(toValue(nodes), folder.groupId);
    if (node === undefined) {
      throw new Error(`group ${folder.groupId} is not in the organization`);
    }
    return node.name;
  }

  async function selectFolder(kind: PaneKind, folder: FolderKeyDto): Promise<void> {
    const query: ModListQueryDto = {
      ...toValue(queries[kind]),
      group:
        folder.kind === "group"
          ? { kind: "inScope", groupId: folder.groupId }
          : { kind: "ungrouped" },
    };
    try {
      const name = folderName(folder);
      const list = await (kind === "active" ? listWorkingMods(query) : listInactiveMods(query));
      const selection = selections[kind];
      selection.value = [...new Set([...selection.value, ...list.items.map((row) => row.modId)])];
      const count = list.items.length;
      await announce(
        count === 0
          ? t("mods.groupsView.folderSelectedNone", { name })
          : t("mods.groupsView.folderSelected", { count, name }, count),
      );
    } catch (caught: unknown) {
      const failure = describeCommandError(caught);
      toast.add({
        severity: "error",
        summary: tm(failure.title),
        detail: tm(failure.detail),
        life: 8000,
      });
    }
  }

  return { selectFolder };
}
