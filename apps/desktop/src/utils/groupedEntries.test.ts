import { describe, expect, it } from "vitest";

import type { GroupedEntryDto } from "@/types/generated/GroupedEntryDto";
import {
  folderRowKey,
  folderTestToken,
  modRowKey,
  rangeLineOf,
  treeRows,
} from "@/utils/groupedEntries";

function folder(id: number, depth: number, isCollapsed = false): GroupedEntryDto {
  return {
    kind: "folder",
    folder: { kind: "group", id, name: `Group ${id}`, color: null },
    depth,
    matched: 1,
    isCollapsed,
  };
}

function ungroupedFolder(): GroupedEntryDto {
  return {
    kind: "folder",
    folder: { kind: "ungrouped" },
    depth: 1,
    matched: 1,
    isCollapsed: false,
  };
}

function mod(modId: string, groupId: number | null, depth: number, alsoIn = 0): GroupedEntryDto {
  return {
    kind: "mod",
    modId,
    folder: groupId === null ? { kind: "ungrouped" } : { kind: "group", groupId },
    depth,
    alsoIn,
  };
}

describe("entry keys", () => {
  it("names a folder by its group id or u, and a mod copy by its folder and id", () => {
    expect(folderRowKey({ kind: "group", groupId: 7 })).toBe("f:7");
    expect(folderRowKey({ kind: "ungrouped" })).toBe("f:u");
    expect(modRowKey({ kind: "group", groupId: 7 }, "a.mod")).toBe("m:7:a.mod");
    expect(modRowKey({ kind: "ungrouped" }, "a.mod")).toBe("m:u:a.mod");
    expect(folderTestToken({ kind: "ungrouped" })).toBe("ungrouped");
  });

  it("gives the two copies of one mod two different keys", () => {
    const rows = treeRows([folder(1, 1), mod("x", 1, 2), folder(2, 1), mod("x", 2, 2, 1)]);

    const keys = rows.map((row) => row.key);
    expect(new Set(keys).size).toBe(keys.length);
  });
});

describe("treeRows", () => {
  it("links a folder to the nearest earlier folder one level up, and a mod to its folder", () => {
    const rows = treeRows([
      folder(1, 1),
      folder(2, 2),
      folder(3, 3),
      mod("deep", 3, 4),
      mod("mid", 2, 3),
      folder(4, 2),
      mod("top", 1, 2),
      ungroupedFolder(),
      mod("loose", null, 2),
    ]);

    expect(rows.map((row) => [row.key, row.parentKey, row.level])).toEqual([
      ["f:1", null, 1],
      ["f:2", "f:1", 2],
      ["f:3", "f:2", 3],
      ["m:3:deep", "f:3", 4],
      ["m:2:mid", "f:2", 3],
      ["f:4", "f:1", 2],
      ["m:1:top", "f:1", 2],
      ["f:u", null, 1],
      ["m:u:loose", "f:u", 2],
    ]);
  });

  it("treats a rebased top level (a group scope) as having no parent", () => {
    const [first, second] = treeRows([folder(9, 1), folder(10, 2)]);

    expect(first?.parentKey).toBeNull();
    expect(second?.parentKey).toBe("f:9");
  });

  it("carries the also-in count of a mod copy", () => {
    const [, copy] = treeRows([folder(1, 1), mod("x", 1, 2, 3)]);

    expect(copy).toMatchObject({ kind: "mod", alsoIn: 3 });
  });
});

describe("rangeLineOf", () => {
  it("keeps headers in the line with no mod, so a header can anchor a range", () => {
    const line = rangeLineOf(
      treeRows([folder(1, 1), mod("a", 1, 2), folder(2, 1), mod("a", 2, 2)]),
    );

    expect(line).toEqual([
      { key: "f:1", modId: null },
      { key: "m:1:a", modId: "a" },
      { key: "f:2", modId: null },
      { key: "m:2:a", modId: "a" },
    ]);
  });
});
