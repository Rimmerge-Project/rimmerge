// The element a list scrolls inside, handed down to the thumbnails it holds.

import { type InjectionKey, inject, type Ref, ref } from "vue";

/**
 * The scrolling ancestor a list sits in (the shell's `<main>`, or an item picker's own
 * list). A thumbnail's "near the viewport" margin must be measured against it: an
 * IntersectionObserver rooted at the viewport clips to every scrolling ancestor first, so
 * its margin would never reach past the scroller's own edge.
 */
export const SCROLL_CONTAINER_KEY: InjectionKey<Readonly<Ref<HTMLElement | null>>> =
  Symbol("scrollContainer");

const NO_SCROLL_CONTAINER: Readonly<Ref<HTMLElement | null>> = ref(null);

/** The enclosing scroll container, or a ref to `null` (the viewport) when none was provided. */
export function useScrollContainer(): Readonly<Ref<HTMLElement | null>> {
  return inject(SCROLL_CONTAINER_KEY, NO_SCROLL_CONTAINER);
}
