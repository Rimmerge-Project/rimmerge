import type { MessagePassThroughOptions } from "primevue/message";

/**
 * Pass-through for a banner that is part of the page, not an interruption. PrimeVue's `Message`
 * renders `role="alert" aria-live="assertive"`, and a banner behind a `v-if` is read out afresh
 * every time its page mounts. The root's pass-through is bound after the component's own
 * attributes, so it replaces both: the banner is a plain note and says nothing by itself. The
 * shell's one always-mounted live region speaks for it (`useBannerAnnouncement`), since a live
 * region that is mounted together with its text is usually not announced at all.
 */
export const QUIET_MESSAGE_PT = {
  root: { role: "note", "aria-live": "off" },
} satisfies MessagePassThroughOptions;
