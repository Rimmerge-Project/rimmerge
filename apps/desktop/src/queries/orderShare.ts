import { useQueryCache } from "@pinia/colada";

import { useSessionStore } from "@/stores/session";
import type { ProjectSummaryDto } from "@/types/generated/ProjectSummaryDto";

/**
 * Adopts the session an import (`import_order`) just swapped in: the backend's `selected`
 * (always Current after an import) the way `useRescanMutation` does, and every query
 * invalidated directly. The whole session was replaced, so nothing cached is trustworthy, and
 * the caller need not wait for the `session://changed` round trip. The import itself is not a
 * mutation (see `useOrderShare`), so this is the success path's one shared step.
 */
export function useAdoptImportedSession(): (summary: ProjectSummaryDto) => Promise<void> {
  const queryCache = useQueryCache();
  const session = useSessionStore();
  return async (summary) => {
    session.setSelected(summary.selected);
    await queryCache.invalidateQueries();
  };
}
