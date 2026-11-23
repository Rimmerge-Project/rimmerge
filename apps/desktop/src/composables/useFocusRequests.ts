import { getCurrentScope, nextTick, onScopeDispose } from "vue";

/** How many animation frames {@link useFocusRequests} waits for a scrolled-to row to render. */
const FOCUS_RETRY_FRAMES = 20;

/**
 * The "scroll a virtualized row into view, then focus it" wait both mod tables share. A far row is
 * not in the DOM until the virtualizer's own scroll event has re-rendered, a frame or two after
 * the scroll call, so a single `nextTick` is not enough; and a newer request must supersede an
 * older one that is still waiting, or the older row would steal focus after the newer one got it.
 */
export function useFocusRequests() {
  let latest = 0;

  // A request still waiting when its owner unmounts must not focus a row of a gone table.
  if (getCurrentScope()) {
    onScopeDispose(() => {
      latest += 1;
    });
  }

  /** Starts a request, superseding every earlier one. Call it before any early return. */
  function claim(): number {
    latest += 1;
    return latest;
  }

  /**
   * Focuses the element `find` returns once it exists, retrying over a bounded number of frames.
   * Gives up silently when a newer request was claimed meanwhile or the element never renders.
   */
  async function focusWhenRendered(
    request: number,
    find: () => HTMLElement | null | undefined,
  ): Promise<void> {
    for (let frame = 0; frame <= FOCUS_RETRY_FRAMES; frame++) {
      await nextTick();
      if (request !== latest) {
        return;
      }
      const element = find();
      if (element) {
        element.focus();
        return;
      }
      if (frame < FOCUS_RETRY_FRAMES) {
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      }
    }
  }

  return { claim, focusWhenRendered };
}
