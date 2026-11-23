import { type MaybeRefOrGetter, toValue, watch } from "vue";

import { type BannerId, useBannerAnnouncerStore } from "@/stores/bannerAnnouncer";

/**
 * Has the shell's live region speak `text` (a banner's own words) when the banner appears or its
 * text changes, and not again when a page change mounts the banner with the same text. `null`
 * says the banner is gone, so the same text speaks again if it returns; `undefined` says it is
 * only hidden here (another page shows it) and changes nothing.
 */
export function useBannerAnnouncement(
  id: BannerId,
  text: MaybeRefOrGetter<string | null | undefined>,
): void {
  const announcer = useBannerAnnouncerStore();
  watch(
    () => toValue(text),
    (next) => {
      if (next !== undefined) {
        announcer.report(id, next);
      }
    },
    { immediate: true },
  );
}
