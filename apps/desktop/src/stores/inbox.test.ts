import { createTestingPinia } from "@pinia/testing";
import { setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useInboxStore } from "@/stores/inbox";

describe("useInboxStore", () => {
  beforeEach(() => {
    setActivePinia(createTestingPinia({ stubActions: false, createSpy: vi.fn }));
  });

  it("starts at cursor 0 with the needs-input filter and nothing expanded", () => {
    const inbox = useInboxStore();

    expect(inbox.cursor).toBe(0);
    expect(inbox.filter).toEqual({ status: "needsInput", kinds: null, modId: null, search: "" });
    expect(inbox.expandedKey).toBeNull();
  });

  it("setCursor clamps to a non-negative row", () => {
    const inbox = useInboxStore();

    inbox.setCursor(5);
    expect(inbox.cursor).toBe(5);

    inbox.setCursor(-3);
    expect(inbox.cursor).toBe(0);
  });

  it("setSearch updates the search text and resets the cursor", () => {
    const inbox = useInboxStore();
    inbox.setCursor(4);

    inbox.setSearch("wall");

    expect(inbox.filter.search).toBe("wall");
    expect(inbox.cursor).toBe(0);
  });

  it("cycleStatusFilter advances through needsInput -> auto -> userOverridden -> all -> needsInput", () => {
    const inbox = useInboxStore();

    expect(inbox.filter.status).toBe("needsInput");
    inbox.cycleStatusFilter();
    expect(inbox.filter.status).toBe("auto");
    inbox.cycleStatusFilter();
    expect(inbox.filter.status).toBe("userOverridden");
    inbox.cycleStatusFilter();
    expect(inbox.filter.status).toBeNull();
    inbox.cycleStatusFilter();
    expect(inbox.filter.status).toBe("needsInput");
  });

  it("setStatus sets the filter directly and resets the cursor", () => {
    const inbox = useInboxStore();
    inbox.setCursor(3);

    inbox.setStatus("auto");

    expect(inbox.filter.status).toBe("auto");
    expect(inbox.cursor).toBe(0);
  });

  it("toggleKind adds and removes a kind, clearing back to null when empty", () => {
    const inbox = useInboxStore();

    inbox.toggleKind("defOverride");
    expect(inbox.filter.kinds).toEqual(["defOverride"]);

    inbox.toggleKind("missingMod");
    expect(inbox.filter.kinds).toEqual(["defOverride", "missingMod"]);

    inbox.toggleKind("defOverride");
    expect(inbox.filter.kinds).toEqual(["missingMod"]);

    inbox.toggleKind("missingMod");
    expect(inbox.filter.kinds).toBeNull();
  });

  it("setExpandedKey expands and collapses the note editor", () => {
    const inbox = useInboxStore();

    inbox.setExpandedKey("missing_mod:a.mod");
    expect(inbox.expandedKey).toBe("missing_mod:a.mod");

    inbox.setExpandedKey(null);
    expect(inbox.expandedKey).toBeNull();
  });

  it("reset restores every field to its default", () => {
    const inbox = useInboxStore();
    inbox.setCursor(7);
    inbox.setSearch("wall");
    inbox.setStatus("auto");
    inbox.setExpandedKey("a:b");

    inbox.reset();

    expect(inbox.cursor).toBe(0);
    expect(inbox.filter).toEqual({ status: "needsInput", kinds: null, modId: null, search: "" });
    expect(inbox.expandedKey).toBeNull();
  });
});
