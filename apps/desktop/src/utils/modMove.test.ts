import { describe, expect, it } from "vitest";

import type { ActivatePlanDto } from "@/types/generated/ActivatePlanDto";
import type { DeactivatePlanDto } from "@/types/generated/DeactivatePlanDto";
import {
  anchorForSlot,
  anchorForStep,
  isNoopSlot,
  isPlainActivation,
  isPlainDeactivation,
  moveSetFor,
} from "@/utils/modMove";

function rows(...ids: string[]) {
  return ids.map((modId) => ({ modId }));
}

describe("anchorForSlot", () => {
  const view = rows("a", "b", "c", "d");

  it("anchors a drop between two rows before the row below the gap", () => {
    expect(anchorForSlot(view, 2, new Set(["a"]))).toEqual({ kind: "before", modId: "c" });
  });

  it("anchors a drop above the first row before that row", () => {
    expect(anchorForSlot(view, 0, new Set(["d"]))).toEqual({ kind: "before", modId: "a" });
  });

  it("anchors a drop after the last row after that row", () => {
    expect(anchorForSlot(view, 4, new Set(["a"]))).toEqual({ kind: "after", modId: "d" });
  });

  it("skips moving rows when looking for the neighbour below the gap", () => {
    // Dragging b and c to the gap right above c: the first row below that is not moving is d.
    expect(anchorForSlot(view, 2, new Set(["b", "c"]))).toEqual({ kind: "before", modId: "d" });
  });

  it("falls back to the row above when only moving rows are below the gap", () => {
    expect(anchorForSlot(view, 3, new Set(["c", "d"]))).toEqual({ kind: "after", modId: "b" });
  });

  it("names a shown neighbour under a filter, not a position", () => {
    // The view shows two rows of a longer list; the hidden ones sit between them.
    const filtered = rows("m", "q");
    expect(anchorForSlot(filtered, 1, new Set(["x"]))).toEqual({ kind: "before", modId: "q" });
    expect(anchorForSlot(filtered, 2, new Set(["x"]))).toEqual({ kind: "after", modId: "q" });
  });

  it("clamps a slot outside the view", () => {
    expect(anchorForSlot(view, 99, new Set(["a"]))).toEqual({ kind: "after", modId: "d" });
    expect(anchorForSlot(view, -3, new Set(["d"]))).toEqual({ kind: "before", modId: "a" });
  });

  it("puts the mods at the end when the view has nothing else", () => {
    expect(anchorForSlot([], 0, new Set(["x"]))).toEqual({ kind: "end" });
    expect(anchorForSlot(rows("a"), 0, new Set(["a"]))).toEqual({ kind: "end" });
  });
});

describe("anchorForStep", () => {
  const view = rows("a", "b", "c", "d");

  it("moves a row up to just before the row above it", () => {
    expect(anchorForStep(view, new Set(["c"]), "up")).toEqual({ kind: "before", modId: "b" });
  });

  it("moves a row down to just after the row below it", () => {
    expect(anchorForStep(view, new Set(["b"]), "down")).toEqual({ kind: "after", modId: "c" });
  });

  it("has nowhere to go from the first row up or the last row down", () => {
    expect(anchorForStep(view, new Set(["a"]), "up")).toBeNull();
    expect(anchorForStep(view, new Set(["d"]), "down")).toBeNull();
  });

  it("steps a block past the nearest row outside it", () => {
    expect(anchorForStep(view, new Set(["b", "c"]), "up")).toEqual({ kind: "before", modId: "a" });
    expect(anchorForStep(view, new Set(["b", "c"]), "down")).toEqual({ kind: "after", modId: "d" });
  });

  it("steps a scattered selection from its outer edges", () => {
    expect(anchorForStep(view, new Set(["a", "c"]), "down")).toEqual({ kind: "after", modId: "d" });
    expect(anchorForStep(view, new Set(["b", "d"]), "up")).toEqual({ kind: "before", modId: "a" });
  });

  it("has no answer when none of the moving mods is shown", () => {
    expect(anchorForStep(view, new Set(["hidden"]), "up")).toBeNull();
  });
});

describe("isNoopSlot", () => {
  const view = rows("a", "b", "c", "d");

  it("is true for a gap right beside or inside a contiguous block that is all shown", () => {
    const moving = new Set(["b", "c"]);
    expect([1, 2, 3].map((slot) => isNoopSlot(view, slot, moving))).toEqual([true, true, true]);
  });

  it("is false for a gap away from the block", () => {
    const moving = new Set(["b", "c"]);
    expect(isNoopSlot(view, 0, moving)).toBe(false);
    expect(isNoopSlot(view, 4, moving)).toBe(false);
  });

  it("is false when the moving mods are not together", () => {
    expect(isNoopSlot(view, 1, new Set(["a", "c"]))).toBe(false);
  });

  it("is false when a moving mod is not shown, since the move could still change the order", () => {
    expect(isNoopSlot(view, 1, new Set(["b", "hidden"]))).toBe(false);
  });
});

describe("moveSetFor", () => {
  const view = rows("a", "b", "c", "d");

  it("is just the row when it is not selected", () => {
    expect(moveSetFor("b", ["c", "d"], view)).toEqual(["b"]);
  });

  it("is the whole selection in list order when the row is part of it", () => {
    expect(moveSetFor("d", ["d", "b"], view)).toEqual(["b", "d"]);
  });

  it("keeps selected rows a filter hides, after the shown ones", () => {
    expect(moveSetFor("b", ["gone", "b"], view)).toEqual(["b", "gone"]);
  });
});

describe("isPlainActivation", () => {
  const plain: ActivatePlanDto = {
    toAdd: ["x", "y"],
    unresolvableDependencies: {},
    alreadyActive: [],
  };

  it("accepts a plan that adds exactly the dropped mods, in any order", () => {
    expect(isPlainActivation(plain, ["y", "x"])).toBe(true);
  });

  it("refuses a plan that pulls in another mod", () => {
    expect(isPlainActivation({ ...plain, toAdd: ["dep", "x", "y"] }, ["x", "y"])).toBe(false);
  });

  it("refuses a plan that skips a dropped mod", () => {
    expect(isPlainActivation({ ...plain, toAdd: ["x"] }, ["x", "y"])).toBe(false);
  });

  it("refuses a plan with a warning", () => {
    expect(isPlainActivation({ ...plain, alreadyActive: ["z"] }, ["x", "y"])).toBe(false);
    expect(
      isPlainActivation({ ...plain, unresolvableDependencies: { x: ["gone"] } }, ["x", "y"]),
    ).toBe(false);
  });
});

describe("isPlainDeactivation", () => {
  const plain: DeactivatePlanDto = { toRemove: ["x"], dependentsStillActive: {}, refused: [] };

  it("accepts a plan that removes exactly the dropped mods", () => {
    expect(isPlainDeactivation(plain, ["x"])).toBe(true);
  });

  it("refuses a plan that refuses Core", () => {
    expect(
      isPlainDeactivation({ toRemove: [], dependentsStillActive: {}, refused: ["core"] }, ["core"]),
    ).toBe(false);
  });

  it("refuses a plan that leaves a dependent active", () => {
    expect(isPlainDeactivation({ ...plain, dependentsStillActive: { x: ["dep"] } }, ["x"])).toBe(
      false,
    );
  });

  it("accepts a plan whose dependents map has only empty lists", () => {
    expect(isPlainDeactivation({ ...plain, dependentsStillActive: { x: [] } }, ["x"])).toBe(true);
  });
});
