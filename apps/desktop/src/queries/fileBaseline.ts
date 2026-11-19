import { useQuery } from "@pinia/colada";

import { queryKeys } from "@/queries/keys";
import { getModsConfigBaseline } from "@/services/ipc";

/**
 * The live file baseline's status and what someone else changed in it. Kept current by
 * `session://changed` (reason `fileBaselineChanged`, which the backend emits only when the list
 * or the status changed) and by every other invalidate-all; `useFileBaselinePolling` is what
 * makes the backend notice an edit made outside the app.
 */
export function useFileBaselineQuery() {
  return useQuery({
    key: queryKeys.fileBaseline,
    query: getModsConfigBaseline,
  });
}
