import { defineStore } from "pinia";
import { ref } from "vue";

/** The banners that announce themselves through the shell's live region. */
export type BannerId = "pendingChanges" | "outsideHint" | "outsideNote" | "staleBaseline";

/**
 * The text the shell's always-mounted live region holds. A banner behind a `v-if` is not itself a
 * live region (one mounted together with its text is usually not announced), so it reports its
 * text here. A banner that is mounted again with the text it already announced (a page change
 * re-mounts it) says nothing: only a different text, or a text that came back after the banner
 * went away, is spoken. When the banner whose text the region holds goes away the region is
 * emptied, so the same text coming back is a real change of the region (a region that kept the
 * old text would see the same string and speak nothing). UI state only, never persisted.
 */
export const useBannerAnnouncerStore = defineStore("bannerAnnouncer", () => {
  const message = ref("");
  // The banner whose text `message` holds (`null`: none). Not reactive: nothing renders it.
  let speaker: BannerId | null = null;
  // The last text each banner reported (`null`: it was gone). Not reactive either.
  const lastReported = new Map<BannerId, string | null>();

  /** Reports what `id` shows now, `null` when it is not shown. */
  function report(id: BannerId, text: string | null): void {
    if (lastReported.get(id) === text) {
      return;
    }
    lastReported.set(id, text);
    if (text === null) {
      if (speaker === id) {
        message.value = "";
        speaker = null;
      }
      return;
    }
    message.value = text;
    speaker = id;
  }

  /** Forgets everything, for a newly loaded project: its banners are new banners. */
  function clear(): void {
    message.value = "";
    speaker = null;
    lastReported.clear();
  }

  return { message, report, clear };
});
