import { computed, type MaybeRefOrGetter, type Ref, watch } from "vue";

import { useFolderSelect } from "@/composables/useFolderSelect";
import type { useGroupScope } from "@/composables/useGroupScope";
import { useGroupsCollapse } from "@/composables/useGroupsCollapse";
import { usePaneList, usePaneViews } from "@/composables/usePaneList";
import { useSessionStore } from "@/stores/session";
import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";
import type { PaneKind } from "@/utils/groupsCollapse";
import type { FolderAction } from "@/utils/modRowMenu";
import type { PaneView } from "@/utils/panePrefs";

/** What {@link useGroupsView} reads from the page. */
export interface GroupsViewOptions {
  /** The group scope and organization; the Groups view needs the groups to be usable. */
  groups: Pick<
    ReturnType<typeof useGroupScope>,
    "isUsable" | "isKnownUnusable" | "nodes" | "dropScopeIfStale"
  >;
  /** Each pane's own list query (filter, scope, sort). */
  queries: Readonly<Record<PaneKind, MaybeRefOrGetter<ModListQueryDto>>>;
  /** Each pane's selection, owned by the page. */
  selections: Readonly<Record<PaneKind, Ref<string[]>>>;
  /** Speaks a sentence through the page's polite live region. */
  announce: (text: string) => Promise<void>;
  /** A group menu's "Activate all": plans activating the group's mods. */
  activateGroup: (groupId: number) => Promise<void>;
  /** A group menu's "Deactivate all". */
  deactivateGroup: (groupId: number) => Promise<void>;
}

/**
 * The Mods page's Groups view wiring: each pane's view (List or Groups; the remembered choice is
 * kept, but List is shown while the groups are not usable), its list from whichever query that
 * view runs, the folds kept per profile (pruned to the groups that still exist), and what a folder
 * header's menu does. The page keeps the panes' selections and hands in what the folder actions
 * need.
 */
export function useGroupsView({
  groups,
  queries,
  selections,
  announce,
  activateGroup,
  deactivateGroup,
}: GroupsViewOptions) {
  const { activeView, inactiveView } = usePaneViews();
  const session = useSessionStore();
  const collapse = useGroupsCollapse(() => session.paths?.profileDir ?? null);

  // List is shown only once an answer says the groups cannot be used; while organization is still
  // loading, the remembered view is kept so a restored Groups view does not flash List first.
  const activeShownView = computed<PaneView>(() =>
    groups.isKnownUnusable.value ? "list" : activeView.value,
  );
  const inactiveShownView = computed<PaneView>(() =>
    groups.isKnownUnusable.value ? "list" : inactiveView.value,
  );

  const active = usePaneList({
    kind: "active",
    view: activeShownView,
    query: queries.active,
    collapsed: collapse.activeKeys,
  });
  const inactive = usePaneList({
    kind: "inactive",
    view: inactiveShownView,
    query: queries.inactive,
    collapsed: collapse.inactiveKeys,
  });

  // Folders of a deleted group leave the stored folds once the organization is known.
  watch(
    groups.nodes,
    (nodes) => {
      if (groups.isUsable.value) {
        collapse.pruneTo(nodes.map((node) => node.id));
      }
    },
    { immediate: true },
  );

  const folderSelect = useFolderSelect({
    queries,
    selections,
    nodes: groups.nodes,
    announce,
  });

  function collapseAll(kind: PaneKind): void {
    collapse.collapseAll(
      kind,
      groups.nodes.value.map((node) => node.id),
    );
  }

  /** A choice in a Groups view folder menu: select its mods, or plan activating or deactivating them. */
  function onFolderAction(kind: PaneKind, folder: FolderKeyDto, action: FolderAction): void {
    if (action === "select") {
      void folderSelect.selectFolder(kind, folder);
      return;
    }
    if (folder.kind !== "group") {
      return;
    }
    void (action === "activate" ? activateGroup(folder.groupId) : deactivateGroup(folder.groupId));
  }

  return {
    activeView: activeShownView,
    inactiveView: inactiveShownView,
    setActiveView: (view: PaneView) => {
      activeView.value = view;
    },
    setInactiveView: (view: PaneView) => {
      inactiveView.value = view;
    },
    isActiveGrouped: computed(() => activeShownView.value === "groups"),
    active,
    inactive,
    setFolded: collapse.setCollapsed,
    collapseAll,
    expandAll: collapse.expandAll,
    onFolderAction,
  };
}
