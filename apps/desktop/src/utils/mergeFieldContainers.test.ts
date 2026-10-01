import { describe, expect, it } from "vitest";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import {
  containerStats,
  effectiveContainerCollapsed,
  groupConsecutiveByContainer,
  isContainerCollapsedByDefault,
} from "@/utils/mergeFieldContainers";

function field(overrides: Partial<MergeFieldDto> = {}): MergeFieldDto {
  return {
    path: "wildAnimals/Cobra",
    depth: 1,
    isListItem: false,
    entry: "mapEntry",
    container: "wildAnimals",
    class: "oneSided",
    changedBy: ["a.mod"],
    confidence: 95,
    base: "0.2",
    candidates: { "a.mod": "0.2" },
    result: "0.2",
    choice: null,
    preselected: null,
    ...overrides,
  };
}

describe("containerStats", () => {
  it("aggregates total and conflict counts per container, skipping plain leaves", () => {
    const fields: MergeFieldDto[] = [
      field({ path: "wildAnimals/Allosaurus", class: "oneSided" }),
      field({ path: "wildAnimals/Mammoth", class: "oneSided" }),
      field({ path: "wildAnimals/Raptor", class: "conflict" }),
      field({ path: "label", container: null, entry: "leaf" }),
    ];

    const stats = containerStats(fields);

    expect(stats.get("wildAnimals")).toEqual({ total: 3, conflicts: 1 });
    expect(stats.has("label")).toBe(false);
  });

  it("returns an empty map when no field carries a container", () => {
    const fields: MergeFieldDto[] = [field({ container: null, entry: "leaf" })];
    expect(containerStats(fields).size).toBe(0);
  });
});

describe("isContainerCollapsedByDefault", () => {
  it("collapses a large, fully-resolved container", () => {
    expect(isContainerCollapsedByDefault({ total: 13, conflicts: 0 })).toBe(true);
  });

  it("stays open when a container still has an unresolved conflict, however large", () => {
    expect(isContainerCollapsedByDefault({ total: 80, conflicts: 1 })).toBe(false);
  });

  it("stays open when a container is small, even with no conflict", () => {
    expect(isContainerCollapsedByDefault({ total: 8, conflicts: 0 })).toBe(false);
  });
});

describe("effectiveContainerCollapsed", () => {
  it("an explicit override wins over the data-driven default in both directions", () => {
    const stats = { total: 80, conflicts: 0 };
    expect(effectiveContainerCollapsed("wildAnimals", stats, { wildAnimals: false })).toBe(false);
    const smallStats = { total: 2, conflicts: 0 };
    expect(effectiveContainerCollapsed("wildAnimals", smallStats, { wildAnimals: true })).toBe(
      true,
    );
  });

  it("falls back to the default when no override is stored for this container", () => {
    expect(effectiveContainerCollapsed("wildAnimals", { total: 80, conflicts: 0 }, {})).toBe(true);
  });
});

describe("groupConsecutiveByContainer", () => {
  it("groups consecutive same-container fields into one group, in order", () => {
    const fields: MergeFieldDto[] = [
      field({ path: "wildAnimals/Allosaurus" }),
      field({ path: "wildAnimals/Mammoth" }),
      field({ path: "otherField", container: null, entry: "leaf" }),
    ];

    const groups = groupConsecutiveByContainer(fields);

    expect(groups).toHaveLength(2);
    expect(groups[0]?.container).toBe("wildAnimals");
    expect(groups[0]?.fields.map((f) => f.path)).toEqual([
      "wildAnimals/Allosaurus",
      "wildAnimals/Mammoth",
    ]);
    expect(groups[1]?.container).toBeNull();
    expect(groups[1]?.fields).toHaveLength(1);
  });

  it("treats a lone ordinary field as its own single-field group", () => {
    const groups = groupConsecutiveByContainer([field({ container: null, entry: "leaf" })]);
    expect(groups).toEqual([{ container: null, fields: [expect.objectContaining({})] }]);
  });

  it("starts a new group when the same container reappears after a different one interrupts it", () => {
    const fields: MergeFieldDto[] = [
      field({ path: "wildAnimals/Allosaurus" }),
      field({ path: "otherMap/Key", container: "otherMap" }),
      field({ path: "wildAnimals/Mammoth" }),
    ];

    const groups = groupConsecutiveByContainer(fields);

    expect(groups.map((g) => g.container)).toEqual(["wildAnimals", "otherMap", "wildAnimals"]);
  });
});
