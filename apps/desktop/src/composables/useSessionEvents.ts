import { useQueryCache } from "@pinia/colada";

import { useTauriEvent } from "@/composables/useTauriEvent";
import { startInvalidation } from "@/queries/invalidation";
import type { SessionChangedEventDto } from "@/types/generated/SessionChangedEventDto";
import type { SessionChangeReasonDto } from "@/types/generated/SessionChangeReasonDto";

/**
 * Reasons `useSetMergeChoicesMutation`'s own `onSuccess` already
 * invalidates completely (the `merge`/`findings` key prefixes — see its
 * doc comment) — `set_merge_choices` is this app's only source of either
 * reason, so invalidating everything again here would just refetch the
 * same two (expensive: `rim_session::use_cases::PlanMerge` replays every
 * contributing patch from scratch) preview queries a second time for no
 * gain.
 */
export const HANDLED_BY_MERGE_MUTATION = new Set<SessionChangeReasonDto>([
  "mergeChanged",
  "patchDecided",
]);

/**
 * The actual per-`reason` decision, factored out of {@link useSessionEvents}
 * so it's directly unit-testable without simulating a real Tauri event
 * round trip — `invalidateAll` is `startInvalidation(queryCache)` (no
 * filter: the existing "invalidate everything" blast radius) in production,
 * a spy in a test.
 */
export function handleSessionChanged(
  event: SessionChangedEventDto,
  invalidateAll: () => void,
): void {
  if (HANDLED_BY_MERGE_MUTATION.has(event.reason)) {
    return;
  }
  invalidateAll();
}

/**
 * Listens for the backend's `session://changed` event and invalidates
 * every query, so open pages refetch without the user doing anything.
 * `session://changed` fires after any mutating command (decide/revert/
 * rule/tag/import/settings/apply), and any of those can move counts a
 * mutation's own `onSuccess` doesn't otherwise touch (e.g. deciding a
 * finding changes the dashboard's ledger stats too) — invalidating
 * everything is simpler and cheap enough than tracking each command's
 * full blast radius by hand, *except* for a merge decision (see
 * {@link HANDLED_BY_MERGE_MUTATION}), which skips this handler entirely.
 * Call once per page/layout that needs it — each call registers (and, on
 * scope disposal, tears down) its own listener via `useTauriEvent`.
 */
export function useSessionEvents(): void {
  const queryCache = useQueryCache();

  useTauriEvent<SessionChangedEventDto>("session://changed", (event) => {
    handleSessionChanged(event, () => startInvalidation(queryCache));
  });
}
