import { describe, expect, it } from "vitest";

import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import { reparentRefusal } from "@/utils/groupDrop";

function node(id: number, parent: number | null, depth: number): GroupNodeDto {
  return {
    id,
    name: `g${id}`,
    color: null,
    parent,
    depth,
    childCount: 0,
    directCount: 0,
    scopeCount: 0,
  };
}

/** 1 > 2 > 3, 1 > 4, and a lone top-level 5. */
const TREE = [node(1, null, 1), node(2, 1, 2), node(3, 2, 3), node(4, 1, 2), node(5, null, 1)];

describe("reparentRefusal", () => {
  it("allows a move to another group or to the top level", () => {
    expect(reparentRefusal(TREE, 3, 4)).toBeNull();
    expect(reparentRefusal(TREE, 3, null)).toBeNull();
    expect(reparentRefusal(TREE, 5, 1)).toBeNull();
  });

  it("refuses a group dropped on itself or on its own subgroup", () => {
    expect(reparentRefusal(TREE, 1, 1)).toBe("inside-itself");
    expect(reparentRefusal(TREE, 1, 3)).toBe("inside-itself");
  });

  it("refuses the place the group already is", () => {
    expect(reparentRefusal(TREE, 2, 1)).toBe("already-there");
    expect(reparentRefusal(TREE, 5, null)).toBe("already-there");
  });

  it("leaves the depth limit to the backend", () => {
    const deep = Array.from({ length: 12 }, (_, index) =>
      node(index + 1, index === 0 ? null : index, index + 1),
    );

    expect(reparentRefusal([...deep, node(100, null, 1)], 100, 12)).toBeNull();
  });
});
