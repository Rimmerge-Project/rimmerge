import { computed, type MaybeRefOrGetter, ref, toValue, watch } from "vue";

import {
  useInactiveModsGroupedQuery,
  useInactiveModsQuery,
  useWorkingModsGroupedQuery,
  useWorkingModsQuery,
} from "@/queries/activeSet";
import { useSessionStore } from "@/stores/session";
import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import type { GroupedListQueryDto } from "@/types/generated/GroupedListQueryDto";
import type { GroupedModListDto } from "@/types/generated/GroupedModListDto";
import type { ModListDto } from "@/types/generated/ModListDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";
import type { PaneKind } from "@/utils/groupsCollapse";

/** How a Mods pane lays its rows out: one flat list, or folders by group. */
export type PaneView = "list" | "groups";

/**
 * Each pane's view. Page-local like the sort: it resets to List when another project is loaded
 * and is not saved anywhere.
 */
export function usePaneViews() {
  const activeView = ref<PaneView>("list");
  const inactiveView = ref<PaneView>("list");

  const session = useSessionStore();
  watch(
    () => session.paths?.profileDir ?? null,
    () => {
      activeView.value = "list";
      inactiveView.value = "list";
    },
  );

  return { activeView, inactiveView };
}

/**
 * One pane's rows from whichever query its view runs, so the page holds one pair of values per
 * pane and not a pair of queries. Only the current view's query is enabled: a pane never fetches
 * both shapes. The List query is the pane's own query; the Groups one adds the folded folders.
 * `list` is the same `ModListDto` shape in both views (the grouped answer is a superset), so every
 * reader of the rows (selection, the info panel, drag sets) is the same code in both.
 *
 * `isPlaceholder` is set while the rows are the previous key's answer (the Groups query keeps it
 * on screen while a new key loads): they are not yet the answer to the current query, so a reader
 * that takes them for the full list (the selection prune) must wait for it to turn false.
 */
export function usePaneList({
  kind,
  view,
  query,
  collapsed,
}: {
  kind: PaneKind;
  view: MaybeRefOrGetter<PaneView>;
  /** The pane's own query (filter, scope, sort). */
  query: MaybeRefOrGetter<ModListQueryDto>;
  /** The folders folded shut in this pane. */
  collapsed: MaybeRefOrGetter<readonly FolderKeyDto[]>;
}) {
  const isGrouped = computed(() => toValue(view) === "groups");
  const groupedQuery = computed<GroupedListQueryDto>(() => ({
    list: toValue(query),
    collapsed: [...toValue(collapsed)],
  }));
  const isListEnabled = computed(() => !isGrouped.value);

  const listResult =
    kind === "active"
      ? useWorkingModsQuery(query, isListEnabled)
      : useInactiveModsQuery(query, isListEnabled);
  const groupedResult =
    kind === "active"
      ? useWorkingModsGroupedQuery(groupedQuery, isGrouped)
      : useInactiveModsGroupedQuery(groupedQuery, isGrouped);

  const grouped = computed<GroupedModListDto | undefined>(() =>
    isGrouped.value ? groupedResult.data.value : undefined,
  );
  const data = computed<ModListDto | undefined>(() =>
    isGrouped.value ? groupedResult.data.value : listResult.data.value,
  );
  const isPending = computed(() =>
    isGrouped.value ? groupedResult.isPending.value : listResult.isPending.value,
  );
  const isPlaceholder = computed(() => isGrouped.value && groupedResult.isPlaceholderData.value);
  const error = computed<Error | null>(() =>
    isGrouped.value ? groupedResult.error.value : listResult.error.value,
  );

  return { data, grouped, isPending, isPlaceholder, error };
}
