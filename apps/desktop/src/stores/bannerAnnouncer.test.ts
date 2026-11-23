import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it } from "vitest";

import { useBannerAnnouncerStore } from "@/stores/bannerAnnouncer";

describe("bannerAnnouncer", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("holds the text a banner reports", () => {
    const announcer = useBannerAnnouncerStore();

    announcer.report("outsideNote", "The list changed.");

    expect(announcer.message).toBe("The list changed.");
  });

  it("does not speak a banner again when a page change mounts it with the same text", () => {
    const announcer = useBannerAnnouncerStore();
    announcer.report("outsideHint", "The list changed.");
    announcer.report("pendingChanges", "2 mods need a rescan.");

    // The hint's page re-mounts it with the text it already reported.
    announcer.report("outsideHint", "The list changed.");

    expect(announcer.message).toBe("2 mods need a rescan.");
  });

  it("speaks a changed text, and the same text again once the banner went away", () => {
    const announcer = useBannerAnnouncerStore();
    announcer.report("outsideHint", "One mod added.");
    announcer.report("outsideHint", "Two mods added.");
    expect(announcer.message).toBe("Two mods added.");

    announcer.report("outsideHint", null);
    announcer.report("pendingChanges", "Something else.");
    announcer.report("outsideHint", "Two mods added.");

    expect(announcer.message).toBe("Two mods added.");
  });

  it("speaks the same text again once its banner went away, with nothing else reported between", () => {
    const announcer = useBannerAnnouncerStore();
    const spoken: string[] = [];
    announcer.$subscribe(() => spoken.push(announcer.message), { flush: "sync" });
    announcer.report("pendingChanges", "1 change not scanned.");

    announcer.report("pendingChanges", null);
    announcer.report("pendingChanges", "1 change not scanned.");

    expect(spoken).toEqual(["1 change not scanned.", "", "1 change not scanned."]);
  });

  it("empties the region when the banner it holds goes away", () => {
    const announcer = useBannerAnnouncerStore();
    announcer.report("outsideHint", "The list changed.");

    announcer.report("outsideHint", null);

    expect(announcer.message).toBe("");
  });

  it("leaves another banner's text in the region when a banner that is not speaking goes away", () => {
    const announcer = useBannerAnnouncerStore();
    announcer.report("outsideHint", "The list changed.");
    announcer.report("pendingChanges", "2 mods need a rescan.");

    announcer.report("outsideHint", null);

    expect(announcer.message).toBe("2 mods need a rescan.");
  });

  it("forgets what every banner reported when cleared", () => {
    const announcer = useBannerAnnouncerStore();
    announcer.report("outsideNote", "The list changed.");

    announcer.clear();
    expect(announcer.message).toBe("");
    announcer.report("outsideNote", "The list changed.");

    expect(announcer.message).toBe("The list changed.");
  });
});
