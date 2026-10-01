import { useMutation } from "@pinia/colada";

import { importGameLog } from "@/services/ipc";

/**
 * Imports a `Player.log` — a mutation, not a
 * cached query, since it's explicitly triggered (the "Import game log"
 * button) and its own result is never persisted or cached on the
 * session (the log is chosen by the user each time). No
 * `onSuccess` invalidation either — an import never mutates the
 * session, so there's nothing to invalidate; the caller stores the
 * result itself (`stores/session.ts`'s `setGameLogSummary`).
 */
export function useImportGameLogMutation() {
  return useMutation({
    mutation: (path: string) => importGameLog(path),
  });
}
