import { useResizeObserver } from "@vueuse/core";
import { type MaybeRefOrGetter, onMounted, type Ref, readonly, ref, toValue } from "vue";

/**
 * Whether `content`, laid out at its natural width, is wider than `container` can show, so the
 * caller can swap it for a compact form. `content` must keep its natural width (`w-max`,
 * `shrink-0`, no wrapping) and stay measurable in both forms (hidden with `invisible absolute`, never
 * `display: none`); then the answer depends only on the two sizes and never on which form is
 * showing, so the swap cannot flip back and forth. Widths are measured, not guessed from a
 * breakpoint, so a longer translation collapses at a wider container than a shorter one.
 *
 * Re-measures on mount and when either element resizes (a locale change resizes `content`).
 * Where `ResizeObserver` is missing (jsdom) it stays `false`.
 */
export function useCollapseToFit(
  container: MaybeRefOrGetter<HTMLElement | null | undefined>,
  content: MaybeRefOrGetter<HTMLElement | null | undefined>,
): Readonly<Ref<boolean>> {
  const isCollapsed = ref(false);

  function measure(): void {
    const containerElement = toValue(container);
    const contentElement = toValue(content);
    if (!containerElement || !contentElement) {
      return;
    }
    // +1 absorbs sub-pixel rounding between the integer widths.
    isCollapsed.value = contentElement.offsetWidth > containerElement.clientWidth + 1;
  }

  onMounted(() => {
    measure();
  });
  useResizeObserver(() => [toValue(container), toValue(content)], measure);
  return readonly(isCollapsed);
}
