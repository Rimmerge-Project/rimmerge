import { useResizeObserver } from "@vueuse/core";
import {
  type ComputedRef,
  computed,
  type MaybeRefOrGetter,
  onMounted,
  type Ref,
  ref,
  toValue,
  type WatchSource,
  watch,
} from "vue";

/**
 * Expand/collapse state for a CSS line-clamped element, and whether the
 * clamp actually hides anything. The overflow is measured only while
 * collapsed (an expanded element never overflows), so the last collapsed
 * measurement is kept while expanded and "Show less" stays reachable.
 *
 * Re-measures on mount, when the element's size changes, and when `source`
 * (the text shown) changes; a changed source also collapses.
 */
export function useClampedText(
  element: MaybeRefOrGetter<HTMLElement | null | undefined>,
  source: WatchSource,
): { expanded: Ref<boolean>; showToggle: ComputedRef<boolean> } {
  const expanded = ref(false);
  const isOverflowing = ref(false);

  function measure(): void {
    const target = toValue(element);
    if (!target || expanded.value) {
      return;
    }
    // +1 absorbs sub-pixel rounding between the two integer heights.
    isOverflowing.value = target.scrollHeight > target.clientHeight + 1;
  }

  onMounted(() => {
    measure();
    // Web fonts change line widths once loaded, after the first measurement.
    void document.fonts?.ready.then(measure);
  });
  useResizeObserver(() => toValue(element), measure);
  watch(source, () => {
    expanded.value = false;
  });
  watch([source, expanded], measure, { flush: "post" });

  const showToggle = computed(() => expanded.value || isOverflowing.value);
  return { expanded, showToggle };
}
