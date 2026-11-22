import { useQueryCache } from "@pinia/colada";
import { computed, ref, watch } from "vue";

import { startInvalidation } from "@/queries/invalidation";
import { queryKeys } from "@/queries/keys";
import { useOrganizationQuery } from "@/queries/organization";
import { RimmergeError } from "@/services/ipc";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import { ALL_MODS, findNode, type GroupScopeChoice } from "@/utils/organization";

/** The two list failures that mean the scope names something that is gone. */
const STALE_SCOPE_CODES = new Set(["group_not_found", "organization_unavailable"]);

/**
 * The Mods page group scope: which group (or "Ungrouped", or everything) both lists are narrowed
 * to, plus the organization the group tree reads. The scope is page-local state. It is dropped
 * quietly to "All mods" whenever it can no longer apply: the group was deleted (here or in a
 * refetch after a rescan), or organization is not usable (a rescan that came back Recovered), so
 * the lists never show an error pane for a filter the user did not just type.
 */
export function useGroupScope() {
  const queryCache = useQueryCache();
  const { data: organization, error: organizationError } = useOrganizationQuery();
  const scope = ref<GroupScopeChoice>(ALL_MODS);

  const nodes = computed<readonly GroupNodeDto[]>(() =>
    organization.value?.kind === "loaded" ? organization.value.groups : [],
  );
  const isUsable = computed(() => organization.value?.kind === "loaded");
  // Unusable is an answer: a loaded-but-damaged file, or a failed read. Before the first answer
  // nothing is known, and a remembered view must not be given up on a guess.
  const isKnownUnusable = computed(() =>
    organization.value === undefined
      ? organizationError.value !== null
      : organization.value.kind !== "loaded",
  );

  watch(
    [organization, scope],
    () => {
      const current = scope.value;
      const known = organization.value;
      // Nothing is known until the first answer; a filter is not dropped on a guess.
      if (known === undefined || current.kind === "all") {
        return;
      }
      const isStale =
        known.kind !== "loaded" ||
        (current.kind === "group" && findNode(known.groups, current.id) === undefined);
      if (isStale) {
        scope.value = ALL_MODS;
      }
    },
    { immediate: true },
  );

  /**
   * A list query failed. When the failure says the scope is stale (the group is gone, or groups
   * are unavailable) the scope drops and the organization refetches, so the tree catches up.
   */
  function dropScopeIfStale(failure: unknown): void {
    if (scope.value.kind === "all") {
      return;
    }
    if (failure instanceof RimmergeError && STALE_SCOPE_CODES.has(failure.code)) {
      scope.value = ALL_MODS;
      startInvalidation(queryCache, { key: [...queryKeys.organization()] });
    }
  }

  return { organization, nodes, isUsable, isKnownUnusable, scope, dropScopeIfStale };
}
