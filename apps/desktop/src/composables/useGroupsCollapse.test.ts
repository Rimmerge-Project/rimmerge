import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope, nextTick, ref } from "vue";

import { useGroupsCollapse } from "@/composables/useGroupsCollapse";
import { foldsStorageKey } from "@/utils/groupsCollapse";

const PROFILE = "C:\\profiles\\one";

function setup(profileDir: string | null = PROFILE) {
  const profile = ref<string | null>(profileDir);
  const scope = effectScope();
  const collapse = scope.run(() => useGroupsCollapse(profile));
  if (collapse === undefined) {
    throw new Error("the composable did not run");
  }
  return { profile, collapse, scope };
}

function stored(profileDir: string): unknown {
  const raw = localStorage.getItem(foldsStorageKey(profileDir));
  return raw === null ? null : JSON.parse(raw);
}

describe("useGroupsCollapse", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("starts with nothing folded", () => {
    const { collapse } = setup();

    expect(collapse.activeKeys.value).toEqual([]);
    expect(collapse.inactiveKeys.value).toEqual([]);
  });

  it("folds a folder in one pane only, and saves it for this profile", () => {
    const { collapse } = setup();

    collapse.setCollapsed("active", { kind: "group", groupId: 3 }, true);

    expect(collapse.activeKeys.value).toEqual([{ kind: "group", groupId: 3 }]);
    expect(collapse.inactiveKeys.value).toEqual([]);
    expect(stored(PROFILE)).toEqual({ active: [3], inactive: [] });
  });

  it("opens a folded folder again", () => {
    const { collapse } = setup();
    collapse.setCollapsed("active", { kind: "group", groupId: 3 }, true);

    collapse.setCollapsed("active", { kind: "group", groupId: 3 }, false);

    expect(collapse.activeKeys.value).toEqual([]);
    expect(stored(PROFILE)).toEqual({ active: [], inactive: [] });
  });

  it("keeps the folds in a stable order whatever order they were made in", () => {
    const { collapse } = setup();

    collapse.setCollapsed("active", { kind: "ungrouped" }, true);
    collapse.setCollapsed("active", { kind: "group", groupId: 9 }, true);
    collapse.setCollapsed("active", { kind: "group", groupId: 2 }, true);

    expect(collapse.activeKeys.value).toEqual([
      { kind: "group", groupId: 2 },
      { kind: "group", groupId: 9 },
      { kind: "ungrouped" },
    ]);
  });

  it("reads each profile's own folds, and switches when the profile does", async () => {
    localStorage.setItem(foldsStorageKey("other"), JSON.stringify({ active: [8], inactive: [] }));
    const { collapse, profile } = setup();
    collapse.setCollapsed("active", { kind: "group", groupId: 1 }, true);

    profile.value = "other";
    await nextTick();

    expect(collapse.activeKeys.value).toEqual([{ kind: "group", groupId: 8 }]);
    profile.value = PROFILE;
    await nextTick();
    expect(collapse.activeKeys.value).toEqual([{ kind: "group", groupId: 1 }]);
  });

  it("falls back to nothing folded for a stored value that does not parse", () => {
    localStorage.setItem(foldsStorageKey(PROFILE), '{"active":["x"],"inactive":[]}');

    const { collapse } = setup();

    expect(collapse.activeKeys.value).toEqual([]);
  });

  it("falls back to nothing folded, and keeps working, when storage throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });

    const { collapse } = setup();
    collapse.setCollapsed("active", { kind: "group", groupId: 4 }, true);

    expect(collapse.activeKeys.value).toEqual([{ kind: "group", groupId: 4 }]);
  });

  it("collapses every known group and Ungrouped, and expands them all again", () => {
    const { collapse } = setup();

    collapse.collapseAll("inactive", [5, 1]);
    expect(collapse.inactiveKeys.value).toEqual([
      { kind: "group", groupId: 1 },
      { kind: "group", groupId: 5 },
      { kind: "ungrouped" },
    ]);
    expect(collapse.activeKeys.value).toEqual([]);

    collapse.expandAll("inactive");
    expect(collapse.inactiveKeys.value).toEqual([]);
    expect(stored(PROFILE)).toEqual({ active: [], inactive: [] });
  });

  it("prunes folds of groups that no longer exist, in both panes, and keeps Ungrouped", () => {
    const { collapse } = setup();
    collapse.collapseAll("active", [1, 2, 3]);
    collapse.setCollapsed("inactive", { kind: "group", groupId: 2 }, true);

    collapse.pruneTo([2]);

    expect(collapse.activeKeys.value).toEqual([
      { kind: "group", groupId: 2 },
      { kind: "ungrouped" },
    ]);
    expect(collapse.inactiveKeys.value).toEqual([{ kind: "group", groupId: 2 }]);
    expect(stored(PROFILE)).toEqual({ active: [2, "ungrouped"], inactive: [2] });
  });

  it("writes nothing when there is nothing to prune", () => {
    const { collapse } = setup();
    collapse.setCollapsed("active", { kind: "group", groupId: 2 }, true);
    const write = vi.spyOn(Storage.prototype, "setItem");

    collapse.pruneTo([2, 3]);

    expect(write).not.toHaveBeenCalled();
  });
});
