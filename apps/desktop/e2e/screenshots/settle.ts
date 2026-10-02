import type { Page } from "@playwright/test";

/** How long `main` must stay untouched before a route counts as settled. */
const QUIET_MS = 250;

/**
 * Resolves once `main` has stopped changing (no DOM mutation for a short
 * quiet window) and the fonts are ready, i.e. the route's lazily loaded
 * content and its queries have settled. The quiet window is a debounce on a
 * condition, not a fixed sleep: it restarts on every mutation.
 */
export async function waitForQuietMain(page: Page): Promise<void> {
  await page.evaluate(async (quietMs) => {
    await document.fonts.ready;
    const main = document.querySelector("main");
    if (main === null) {
      return;
    }
    await new Promise<void>((resolve) => {
      let timer = window.setTimeout(done, quietMs);
      const observer = new MutationObserver(() => {
        window.clearTimeout(timer);
        timer = window.setTimeout(done, quietMs);
      });
      function done(): void {
        observer.disconnect();
        resolve();
      }
      observer.observe(main, {
        subtree: true,
        childList: true,
        attributes: true,
        characterData: true,
      });
    });
  }, QUIET_MS);
}
