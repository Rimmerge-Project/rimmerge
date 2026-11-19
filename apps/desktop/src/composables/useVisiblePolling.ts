import { useDocumentVisibility, useIntervalFn, useWindowFocus } from "@vueuse/core";
import { watch } from "vue";

/**
 * Runs `tick` every `intervalMs` while the document is visible, and once when the window
 * regains focus or becomes visible again. Paused while hidden. The caller's `tick` decides
 * whether to skip (for example while its request is still pending): this only schedules.
 */
export function useVisiblePolling(tick: () => void, intervalMs: number): void {
  const visibility = useDocumentVisibility();
  const isFocused = useWindowFocus();

  const { pause, resume } = useIntervalFn(tick, intervalMs, {
    immediate: visibility.value !== "hidden",
  });

  watch(visibility, (state) => {
    if (state === "hidden") {
      pause();
      return;
    }
    resume();
    tick();
  });
  watch(isFocused, (focused) => {
    if (focused) {
      tick();
    }
  });
}
