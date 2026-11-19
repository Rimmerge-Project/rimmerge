import { onScopeDispose } from "vue";

import { edgeScrollStep } from "@/utils/dragScroll";

/**
 * Scrolls a list's scroller while a drag hovers near its top or bottom edge, so a row can be
 * dropped past the first viewport of a virtualized list. Feed it every `dragover`; it keeps a
 * frame loop running while the pointer sits in an edge zone and stops it when the pointer leaves
 * the zone, on {@link stop}, and when the scope ends. A native `dragover` is not frequent enough
 * (and stops while the pointer is still) to scroll smoothly on its own.
 */
export function useDragAutoScroll(getScroller: () => HTMLElement | null) {
  let stepPx = 0;
  let frame: number | null = null;

  function tick(): void {
    const scroller = getScroller();
    if (scroller === null || stepPx === 0) {
      frame = null;
      return;
    }
    scroller.scrollTop += stepPx;
    frame = requestAnimationFrame(tick);
  }

  /** Re-reads the pointer position from a `dragover` and starts or stops the loop. */
  function update(event: DragEvent): void {
    const scroller = getScroller();
    if (scroller === null) {
      return;
    }
    const bounds = scroller.getBoundingClientRect();
    stepPx = edgeScrollStep(event.clientY, bounds.top, bounds.bottom);
    if (stepPx !== 0 && frame === null) {
      frame = requestAnimationFrame(tick);
    }
  }

  /** Stops scrolling (the drag left, dropped or ended). */
  function stop(): void {
    stepPx = 0;
    if (frame !== null) {
      cancelAnimationFrame(frame);
      frame = null;
    }
  }

  onScopeDispose(stop);

  return { update, stop };
}
