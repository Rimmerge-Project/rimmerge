import { useMutation, useQuery, useQueryCache } from "@pinia/colada";

import { awaitInvalidation, startInvalidation } from "@/queries/invalidation";
import { queryKeys } from "@/queries/keys";
import { listTags, setManualTag, setManualTags } from "@/services/ipc";
import type { SetManualTagRequestDto } from "@/types/generated/SetManualTagRequestDto";
import type { SetManualTagsRequestDto } from "@/types/generated/SetManualTagsRequestDto";

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

/**
 * Adds or removes one manual tag on many mods at once (one save). Waits for the refetch so the
 * lists already show the tag when the caller reports the outcome; like the single-mod tag it
 * invalidates everything, which is what the `tagSet` event would refetch anyway.
 */
export function useSetManualTagsMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: SetManualTagsRequestDto) => setManualTags(request),
    onSuccess: () => awaitInvalidation(queryCache),
  });
}
