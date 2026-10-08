import { useMutation, useQuery, useQueryCache } from "@pinia/colada";

import { startInvalidation } from "@/queries/invalidation";
import { queryKeys } from "@/queries/keys";
import { listTags, setManualTag } from "@/services/ipc";
import type { SetManualTagRequestDto } from "@/types/generated/SetManualTagRequestDto";

/** The current tag assignments (inference plus manual overrides). */
export function useTagsQuery() {
  return useQuery({
    key: queryKeys.tags,
    query: listTags,
  });
}

/** Sets (or replaces) a manual tag override, persisting the change. */
export function useSetManualTagMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: SetManualTagRequestDto) => setManualTag(request),
    // Invalidates everything (not just tags/mods/order) so this matches
    // what production's `session://changed` event (see `useSessionEvents`)
    // would refetch anyway — a duplicate refetch there, but the only
    // refetch the Vitest/Playwright mock tier gets, since the mock never
    // emits that event.
    onSuccess: () => {
      startInvalidation(queryCache);
    },
  });
}
