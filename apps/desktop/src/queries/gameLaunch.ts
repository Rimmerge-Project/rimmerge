import { type QueryCache, useQuery } from "@pinia/colada";

import { queryKeys } from "@/queries/keys";
import { getGameLaunchStatus } from "@/services/ipc";

/**
 * What the Launch RimWorld button shows. Kept fresh by `useGameLaunchPolling`
 * (a 5 s poll while the window is visible) and by `session://changed`'s
 * invalidate-all. The call can wait behind a long command that holds the
 * session lock (a verify), so callers must treat "pending" and "error" as an
 * unknown status, never as a launchable one.
 */
export function useGameLaunchStatusQuery() {
  return useQuery({
    key: queryKeys.gameLaunchStatus,
    query: getGameLaunchStatus,
  });
}

/**
 * The call currently fetching the status entry, if any. Lives beside the query so the entry's
 * key has one owner: `useGameLaunch` waits on it after its own refetch is superseded.
 */
export function pendingGameLaunchStatusCall(queryCache: QueryCache) {
  return queryCache.get(queryKeys.gameLaunchStatus())?.pending ?? null;
}
