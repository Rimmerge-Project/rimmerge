import { describe, expect, it } from "vitest";

import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import {
  ALL_MODS,
  COLOR_NAME_KEY,
  COLOR_SWATCH_CLASS,
  COLOR_TOKENS,
  groupFilterOf,
  isSameScope,
  pathTo,
  searchTree,
  subtreeIds,
  visibleNodes,
} from "@/utils/organization";

function node(id: number, name: string, parent: number | null, depth: number): GroupNodeDto {
  return { id, name, color: null, parent, depth, childCount: 0, directCount: 0, scopeCount: 0 };
}

/** Weapons(1) > Melee(2) > Swords(3); Weapons > Ranged(4); Armor(5). */
const TREE = [
  node(1, "Weapons", null, 1),
  node(2, "Melee", 1, 2),
  node(3, "Swords", 2, 3),
  node(4, "Ranged", 1, 2),
  node(5, "Armor", null, 1),
];

const ids = (nodes: GroupNodeDto[]): number[] => nodes.map((entry) => entry.id);

describe("palette tables", () => {
  it("name and style every colour token", () => {
    for (const token of COLOR_TOKENS) {
      expect(COLOR_SWATCH_CLASS[token]).toBe(`bg-mark-${token}`);
      expect(COLOR_NAME_KEY[token]).toBe(`mods.colors.names.${token}`);
    }
    expect(COLOR_TOKENS).toHaveLength(12);
  });
});

describe("groupFilterOf", () => {
  it("maps each scope to the list filter and no scope to none", () => {
    expect(groupFilterOf(ALL_MODS)).toBeNull();
    expect(groupFilterOf({ kind: "ungrouped" })).toEqual({ kind: "ungrouped" });
    expect(groupFilterOf({ kind: "group", id: 7 })).toEqual({ kind: "inScope", groupId: 7 });
  });

  it("tells two scopes apart by kind and group", () => {
    expect(isSameScope({ kind: "group", id: 1 }, { kind: "group", id: 1 })).toBe(true);
    expect(isSameScope({ kind: "group", id: 1 }, { kind: "group", id: 2 })).toBe(false);
    expect(isSameScope(ALL_MODS, { kind: "ungrouped" })).toBe(false);
  });
});

describe("tree helpers", () => {
  it("collects a group and everything below it", () => {
    expect([...subtreeIds(TREE, 1)].toSorted()).toEqual([1, 2, 3, 4]);
    expect([...subtreeIds(TREE, 3)]).toEqual([3]);
  });

  it("reads the path from the top level down", () => {
    expect(pathTo(TREE, 3).map((entry) => entry.name)).toEqual(["Weapons", "Melee", "Swords"]);
    expect(pathTo(TREE, 99)).toEqual([]);
  });

  it("draws a collapsed group's children only once it is expanded", () => {
    expect(ids(visibleNodes(TREE, new Set(), null))).toEqual([1, 5]);
    expect(ids(visibleNodes(TREE, new Set([1]), null))).toEqual([1, 2, 4, 5]);
    expect(ids(visibleNodes(TREE, new Set([1, 2]), null))).toEqual([1, 2, 3, 4, 5]);
  });
});

describe("searchTree", () => {
  it("keeps every match ancestors and draws them expanded", () => {
    const result = searchTree(TREE, "sword");

    expect([...(result?.matches ?? [])]).toEqual([3]);
    expect([...(result?.shown ?? [])].toSorted()).toEqual([1, 2, 3]);
    expect(ids(visibleNodes(TREE, new Set(), result?.shown ?? null))).toEqual([1, 2, 3]);
  });

  it("is no search for an empty query and finds nothing for an unknown name", () => {
    expect(searchTree(TREE, "  ")).toBeNull();
    expect(searchTree(TREE, "zzz")?.shown.size).toBe(0);
  });
});
