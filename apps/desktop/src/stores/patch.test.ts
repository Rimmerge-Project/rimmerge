import { createTestingPinia } from "@pinia/testing";
import { setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { usePatchStore } from "@/stores/patch";

describe("usePatchStore", () => {
  beforeEach(() => {
    setActivePinia(createTestingPinia({ stubActions: false, createSpy: vi.fn }));
  });

  it("starts at cursor 0 with the needs-input filter and nothing expanded", () => {
    const patch = usePatchStore();

    expect(patch.cursor).toBe(0);
    expect(patch.filter).toEqual({ status: "needsInput", kinds: null, modId: null, search: "" });
    expect(patch.expandedKey).toBeNull();
  });

  it("setCursor clamps to a non-negative row", () => {
    const patch = usePatchStore();

    patch.setCursor(5);
    expect(patch.cursor).toBe(5);

    patch.setCursor(-3);
    expect(patch.cursor).toBe(0);
  });

  it("cycleStatusFilter advances through needsInput -> auto -> userOverridden -> all -> needsInput", () => {
    const patch = usePatchStore();

    patch.cycleStatusFilter();
    expect(patch.filter.status).toBe("auto");
    patch.cycleStatusFilter();
    expect(patch.filter.status).toBe("userOverridden");
    patch.cycleStatusFilter();
    expect(patch.filter.status).toBeNull();
    patch.cycleStatusFilter();
    expect(patch.filter.status).toBe("needsInput");
  });

  it("activate resets the cursor/filter only when a different patch becomes active", () => {
    const patch = usePatchStore();

    patch.activate("a");
    patch.setCursor(3);
    patch.activate("a");
    expect(patch.cursor).toBe(3);

    patch.activate("b");
    expect(patch.cursor).toBe(0);
    expect(patch.activePatchId).toBe("b");
  });

  it("reset restores every field to its default — used whenever the active patch changes", () => {
    const patch = usePatchStore();
    patch.setCursor(7);
    patch.setSearch("wall");
    patch.setStatus("auto");
    patch.setExpandedKey("a:b");

    patch.reset();

    expect(patch.cursor).toBe(0);
    expect(patch.filter).toEqual({ status: "needsInput", kinds: null, modId: null, search: "" });
    expect(patch.expandedKey).toBeNull();
  });
});
