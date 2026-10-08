import { useQueryCache } from "@pinia/colada";

import { startInvalidation } from "@/queries/invalidation";
import { useSessionStore } from "@/stores/session";
import type { ProjectSummaryDto } from "@/types/generated/ProjectSummaryDto";

/**
 * Adopts the session an import (`import_order`) just swapped in: the backend's `selected`
 * (always Current after an import) the way `useRescanMutation` does, and every query
 * invalidated directly. The whole session was replaced, so nothing cached is trustworthy, and
 * the caller need not wait for the `session://changed` round trip. The import itself is not a
 * mutation (see `useOrderShare`), so this is the success path's one shared step.
 *
 * The invalidation is **not awaited**, as in `useRescanMutation`: Pinia Colada's
 * `invalidateQueries` resolves only once every active query has refetched, and some of those
 * refetches are slow on a freshly swapped session (the merge-mod render behind the Merge mod
 * page or an open Apply dialog, for one). Waiting for them would hold the preview's spinner long after the swap
 * landed; pages keep their current data until their refetch lands.
 */
export function useAdoptImportedSession(): (summary: ProjectSummaryDto) => void {
  const queryCache = useQueryCache();
  const session = useSessionStore();
  return (summary) => {
    session.setSelected(summary.selected);
    startInvalidation(queryCache);
  };
}
