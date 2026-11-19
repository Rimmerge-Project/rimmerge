import type { Ref } from "vue";

import { useVisiblePolling } from "@/composables/useVisiblePolling";

/** How often the game-running fact is re-read while the window is visible. */
export const GAME_LAUNCH_POLL_MS = 5000;

/** The slice of a Pinia Colada query the poller needs. */
type PolledQuery = {
  /** True while a request is in flight. */
  readonly isLoading: Readonly<Ref<boolean>>;
  readonly refetch: () => Promise<unknown>;
};

/**
 * Re-reads the launch status every {@link GAME_LAUNCH_POLL_MS} while the document is visible,
 * and once when the window regains focus or becomes visible again.
 *
 * A tick is skipped while a request is still pending. The status call takes the session
 * lock, so behind a long command (a verify) it can stay pending for a long time; a plain
 * `refetch` per tick would start a new backend call each time, all queued on that lock.
 */
export function useGameLaunchPolling(query: PolledQuery): void {
  function tick(): void {
    if (query.isLoading.value) {
      return;
    }
    // A failure is already in the query's own error state, which the button renders.
    query.refetch().catch(() => undefined);
  }

  useVisiblePolling(tick, GAME_LAUNCH_POLL_MS);
}
