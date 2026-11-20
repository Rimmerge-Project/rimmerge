import { describe, expect, it } from "vitest";

import type { GroupScopePlanDto } from "@/types/generated/GroupScopePlanDto";
import { groupScopeLines, scopeIds } from "@/utils/groupScopeText";

/** A `t` that shows the key and the count it was asked to word, so each line is checkable. */
const t = (key: string, named: Record<string, unknown>, plural: number) =>
  `${key}:${String(named["count"])}:${plural}`;

function plan(overrides: Partial<GroupScopePlanDto> = {}): GroupScopePlanDto {
  return {
    groupCount: 3,
    modCount: 10,
    activeIds: ["a", "b"],
    inactiveIds: ["c", "d", "e"],
    notInstalled: 0,
    notInstalledIds: [],
    ...overrides,
  };
}

describe("scopeIds", () => {
  it("activates the inactive mods and deactivates the active ones", () => {
    expect(scopeIds(plan(), "activate")).toEqual(["c", "d", "e"]);
    expect(scopeIds(plan(), "deactivate")).toEqual(["a", "b"]);
  });
});

describe("groupScopeLines", () => {
  it("says subgroups are included when the scope holds more than one group", () => {
    expect(groupScopeLines(t, plan(), "deactivate", 0)).toEqual([
      "mods.bulk.scope.deactivateTree:2:2",
    ]);
  });

  it("uses the single group wording for a group without subgroups", () => {
    expect(groupScopeLines(t, plan({ groupCount: 1 }), "activate", 0)).toEqual([
      "mods.bulk.scope.activateGroup:3:3",
    ]);
  });

  it("adds the not installed and hidden lines, each with its own count", () => {
    expect(groupScopeLines(t, plan({ notInstalled: 5 }), "activate", 2)).toEqual([
      "mods.bulk.scope.activateTree:3:3",
      "mods.bulk.scope.notInstalled:5:5",
      "mods.bulk.scope.hidden:2:2",
    ]);
  });

  it("does not mention uninstalled mods when deactivating", () => {
    const lines = groupScopeLines(t, plan({ notInstalled: 5 }), "deactivate", 0);

    expect(lines.some((line) => line.includes("notInstalled"))).toBe(false);
  });
});
