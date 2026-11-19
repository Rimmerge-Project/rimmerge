import { useVisiblePolling } from "@/composables/useVisiblePolling";
import { refreshModsConfigBaseline } from "@/services/ipc";

/** How often the active mod list on disk is re-read while the window is visible. */
export const FILE_BASELINE_POLL_MS = 5000;

/**
 * Re-reads the active mod list on disk every {@link FILE_BASELINE_POLL_MS} while the document
 * is visible, and once when the window regains focus (the common "changed it in the game or
 * another manager, then came back" path). One poller for the whole app, mounted in the shell.
 *
 * The backend only reads, and emits `session://changed` only when the list or its status
 * changed, so a quiet tick invalidates nothing. A tick is skipped while the previous call is
 * still pending: the call takes the session lock briefly twice, and behind a long command
 * (a verify) it can wait a long time; a tick per interval would queue a call each time.
 * A failure of the call itself (no project loaded, a lost session) is ignored: the next
 * event or tick shows the real state. A file that cannot be read is not a failure here; it
 * comes back as the baseline's own stale state.
 */
export function useFileBaselinePolling(): void {
  let isPending = false;

  async function tick(): Promise<void> {
    if (isPending) {
      return;
    }
    isPending = true;
    try {
      await refreshModsConfigBaseline();
    } catch {
      // See above: nothing to show for a failed poll.
    } finally {
      isPending = false;
    }
  }

  useVisiblePolling(() => void tick(), FILE_BASELINE_POLL_MS);
}
