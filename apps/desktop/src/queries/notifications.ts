import { useMutation, useQuery, useQueryCache } from "@pinia/colada";

import { queryKeys } from "@/queries/keys";
import {
  checkForUpdate,
  completeWelcome,
  dismissNotification,
  listMutedNotificationKinds,
  listNotifications,
  muteNotificationKind,
  resetSettings,
  runLaunchNetworkChecks,
  unmuteNotificationKind,
} from "@/services/ipc";
import type { NotificationKeyDto } from "@/types/generated/NotificationKeyDto";
import type { NotificationKindDto } from "@/types/generated/NotificationKindDto";

/** Every active notice — the bell and the Welcome card both read this one query. */
export function useNotificationsQuery() {
  return useQuery({
    key: queryKeys.notifications,
    query: listNotifications,
  });
}

/**
 * Dismisses one notice occurrence. Also invalidates the Dashboard step: dismissing
 * `Welcome` answers the first-run notice, which the step's `awaitingFirstRun` state reads.
 */
export function useDismissNotificationMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (key: NotificationKeyDto) => dismissNotification(key),
    onSuccess: () => {
      void queryCache.invalidateQueries({ key: queryKeys.notifications() });
      void queryCache.invalidateQueries({ key: queryKeys.recommendedRulesStep() });
    },
  });
}

/** The muted notice kinds — Settings → Network's own "muted kinds" list. */
export function useMutedNotificationKindsQuery() {
  return useQuery({
    key: queryKeys.mutedNotificationKinds,
    query: listMutedNotificationKinds,
  });
}

/** Mutes a whole notice kind — "Don't remind me again". */
export function useMuteNotificationKindMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (kind: NotificationKindDto) => muteNotificationKind(kind),
    onSuccess: () => {
      void queryCache.invalidateQueries({ key: queryKeys.notifications() });
      void queryCache.invalidateQueries({ key: queryKeys.mutedNotificationKinds() });
    },
  });
}

/** Reverses {@link useMuteNotificationKindMutation}. */
export function useUnmuteNotificationKindMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (kind: NotificationKindDto) => unmuteNotificationKind(kind),
    onSuccess: () => {
      void queryCache.invalidateQueries({ key: queryKeys.notifications() });
      void queryCache.invalidateQueries({ key: queryKeys.mutedNotificationKinds() });
    },
  });
}

/** Answers the first-run (Welcome) notice. Also invalidates the Dashboard step, which waits on it. */
export function useCompleteWelcomeMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: () => completeWelcome(),
    onSuccess: () => {
      void queryCache.invalidateQueries({ key: queryKeys.notifications() });
      void queryCache.invalidateQueries({ key: queryKeys.recommendedRulesStep() });
    },
  });
}

/**
 * Resets this profile's sorter/ledger settings to their built-in
 * defaults and saves immediately — the Welcome notice's own "Use
 * recommended settings" action (one click, unlike the Settings page's
 * own "Reset to defaults", which only refills the form).
 */
export function useResetSettingsMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: () => resetSettings(),
    onSuccess: () => {
      // Also affects the Welcome notice's own `settingsMatchRecommended`
      // field, so notifications refetch alongside settings.
      void queryCache.invalidateQueries({ key: queryKeys.settings() });
      void queryCache.invalidateQueries({ key: queryKeys.notifications() });
    },
  });
}

/**
 * "Check now" — neither this command nor {@link useRunLaunchNetworkChecksMutation}
 * emits `session://changed` (see `commands/notifications.rs`'s own
 * module doc: neither touches session state), so this mutation
 * invalidates the notice list itself on success.
 */
export function useCheckForUpdateMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: () => checkForUpdate(),
    onSuccess: () => {
      void queryCache.invalidateQueries({ key: queryKeys.notifications() });
    },
  });
}

/**
 * The once-per-launch automatic check. Also refreshes rule-database
 * state, so the Databases card's own query is invalidated alongside
 * notifications.
 */
export function useRunLaunchNetworkChecksMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: () => runLaunchNetworkChecks(),
    onSuccess: () => {
      void queryCache.invalidateQueries({ key: queryKeys.notifications() });
      void queryCache.invalidateQueries({ key: queryKeys.ruleDatabases() });
    },
  });
}
