import { useMutation, useQuery, useQueryCache } from "@pinia/colada";
import { useToast } from "primevue/usetoast";
import { type MaybeRefOrGetter, toValue } from "vue";
import { useI18n } from "vue-i18n";

import { awaitInvalidation, startInvalidation } from "@/queries/invalidation";
import { queryKeys } from "@/queries/keys";
import {
  activateMods,
  deactivateMods,
  discardPendingChanges,
  getPendingActiveChanges,
  listInactiveMods,
  listWorkingMods,
  moveMods,
  planActivateMods,
  planDeactivateMods,
  rescanProject,
  revertOrderToFile,
} from "@/services/ipc";
import { useSessionStore } from "@/stores/session";
import type { ActivateRequestDto } from "@/types/generated/ActivateRequestDto";
import type { DeactivateRequestDto } from "@/types/generated/DeactivateRequestDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";
import type { MoveRequestDto } from "@/types/generated/MoveRequestDto";
import { ruleWarningsDetail } from "@/utils/ruleWarning";

/**
 * The Mods page's Inactive list. `enabled` (default: always) skips the query
 * while that tab isn't showing, the same as {@link useWorkingModsQuery}.
 */
export function useInactiveModsQuery(
  query: MaybeRefOrGetter<ModListQueryDto>,
  enabled: MaybeRefOrGetter<boolean> = true,
) {
  return useQuery({
    key: () => queryKeys.inactiveMods(toValue(query)),
    query: () => listInactiveMods(toValue(query)),
    enabled: () => toValue(enabled),
  });
}

/**
 * The Mods page's Active list: the working set in its own order. `enabled`
 * lets a page with two tabs skip the query while its own tab isn't showing,
 * rather than fetching both on every keystroke.
 */
export function useWorkingModsQuery(
  query: MaybeRefOrGetter<ModListQueryDto>,
  enabled: MaybeRefOrGetter<boolean> = true,
) {
  return useQuery({
    key: () => queryKeys.workingMods(toValue(query)),
    query: () => listWorkingMods(toValue(query)),
    enabled: () => toValue(enabled),
  });
}

/**
 * The working active-mod set's own pending-change summary: `unscanned`
 * (what a Rescan would pick up — the banner's own count) and `unapplied`
 * (what an Apply would write — the apply dialog's own warning).
 */
export function usePendingActiveChangesQuery() {
  return useQuery({
    key: queryKeys.pendingActiveChanges,
    query: getPendingActiveChanges,
  });
}

/**
 * Plans an activation — a mutation, not a cached query, since it's
 * explicitly triggered (the "Activate selected" button, before the
 * confirm dialog) rather than loaded automatically, and its result is
 * never persisted.
 */
export function usePlanActivateModsMutation() {
  return useMutation({
    mutation: (request: ActivateRequestDto) => planActivateMods(request),
  });
}

/**
 * Commits an activation onto the working set. Invalidates every query
 * directly (not just `queryKeys.pendingActiveChanges`) so the dialog's
 * own caller doesn't have to wait on the `session://changed` event round
 * trip — the same reasoning `useApplyMutation` already documents. An
 * anchored activation (a drop position) can switch the selected order from
 * Suggested to Current; the store adopts the backend's answer here, and the
 * result's `didSwitchToCurrent` lets the caller say so. `activated` is what the commit really
 * added and `alreadyActive` the requested mods its own plan found active (the backend re-plans, so
 * both can differ from the plan the dialog showed); the caller drops both from its selection.
 */
export function useActivateModsMutation() {
  const queryCache = useQueryCache();
  const session = useSessionStore();
  return useMutation({
    mutation: (request: ActivateRequestDto) => activateMods(request),
    onSuccess: async (result) => {
      session.setSelected(result.selected);
      await awaitInvalidation(queryCache);
    },
  });
}

/**
 * Moves active mods next to an anchor. Adopts the selection the move left (the first reorder
 * while Suggested is selected switches to Current) and refetches every query, so the caller can
 * read the new position from the refreshed list.
 */
export function useMoveModsMutation() {
  const queryCache = useQueryCache();
  const session = useSessionStore();
  return useMutation({
    mutation: (request: MoveRequestDto) => moveMods(request),
    onSuccess: async (result) => {
      session.setSelected(result.selected);
      await awaitInvalidation(queryCache);
    },
  });
}

/** Drops every pending membership edit (`working := Current`); no rescan is needed. */
export function useDiscardPendingChangesMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: () => discardPendingChanges(),
    onSuccess: () => awaitInvalidation(queryCache),
  });
}

/** Puts Current's sequence back to `ModsConfig.xml`'s, when both list the same mods. */
export function useRevertOrderToFileMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: () => revertOrderToFile(),
    onSuccess: () => awaitInvalidation(queryCache),
  });
}

/** Plans a deactivation. See {@link usePlanActivateModsMutation}. */
export function usePlanDeactivateModsMutation() {
  return useMutation({
    mutation: (request: DeactivateRequestDto) => planDeactivateMods(request),
  });
}

/**
 * Commits a deactivation onto the working set. See {@link useActivateModsMutation}; the result's
 * `removed` is what the commit really removed and `notActive` the requested mods that were no
 * longer active, which the caller drops from its selection (a refused Core is in neither).
 */
export function useDeactivateModsMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: DeactivateRequestDto) => deactivateMods(request),
    onSuccess: () => awaitInvalidation(queryCache),
  });
}

/**
 * Re-scans the session's own working active-mod set — the Rescan
 * banner's (and the apply dialog's own) Rescan button. Invalidates every
 * query directly, same reason as {@link useActivateModsMutation}.
 *
 * Toasts `warnings`/`ruleWarnings` exactly like `SetupPage.vue`'s own
 * `submit()` does — a rescan is a second, later
 * `LoadProject` run, so its own scan notes and rules-file load warnings
 * are just as real and just as easy to miss with no page navigation to
 * pin them against. Deliberately does **not** call
 * `session.setScanNotes()`: that store field backs the dashboard's own
 * "Scan notes" card and each mod's own page, and updating it from a
 * mutation with no UI of its own to review the change would move
 * established content out from under whatever page the user is on mid-
 * rescan.
 */
export function useRescanMutation() {
  const queryCache = useQueryCache();
  const session = useSessionStore();
  const toast = useToast();
  const { t, locale } = useI18n();
  return useMutation({
    mutation: () => rescanProject(),
    onSuccess: (summary) => {
      // The backend keeps the selected order across a rescan; adopting its
      // answer (rather than trusting the store to still agree) keeps the
      // two from drifting if they ever disagree.
      session.setSelected(summary.selected);
      startInvalidation(queryCache);
      if (summary.warnings.length > 0) {
        toast.add({
          severity: "info",
          summary: t(
            "mods.pendingChanges.rescanFinishedToast",
            { count: summary.warnings.length },
            summary.warnings.length,
          ),
          life: 6_000,
        });
      }
      if (summary.ruleWarnings.length > 0) {
        toast.add({
          severity: "warn",
          summary: t("mods.pendingChanges.rescannedWithWarningsToast"),
          detail: ruleWarningsDetail(summary.ruleWarnings, t, locale.value),
          life: 10_000,
        });
      }
    },
  });
}
