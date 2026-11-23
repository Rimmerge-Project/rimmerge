import { describe, expect, it } from "vitest";

import {
  foldsStorageKey,
  foldToKey,
  keyToFold,
  MAX_FOLDS_PER_PANE,
  NO_FOLDS,
  parseFolds,
  serializeFolds,
  sortedFolds,
} from "@/utils/groupsCollapse";

describe("parseFolds", () => {
  it("reads what serializeFolds wrote", () => {
    const folds = { active: [1, 4, "ungrouped" as const], inactive: [2] };

    expect(parseFolds(serializeFolds(folds))).toEqual(folds);
  });

  it("means nothing folded when there is no stored value", () => {
    expect(parseFolds(null)).toEqual(NO_FOLDS);
  });

  it.each([
    ["not JSON", "{oops"],
    ["not an object", "[1,2]"],
    ["a missing pane", '{"active":[1]}'],
    ["a pane that is not an array", '{"active":3,"inactive":[]}'],
    ["a zero id", '{"active":[0],"inactive":[]}'],
    ["a negative id", '{"active":[-2],"inactive":[]}'],
    ["a fractional id", '{"active":[1.5],"inactive":[]}'],
    ["a string that is not ungrouped", '{"active":["12"],"inactive":[]}'],
    ["a null", '{"active":[null],"inactive":[]}'],
  ])("means nothing folded for %s", (_label, stored) => {
    expect(parseFolds(stored)).toEqual(NO_FOLDS);
  });

  it("refuses a pane larger than the backend would accept, whole", () => {
    const tooMany = Array.from({ length: MAX_FOLDS_PER_PANE + 1 }, (_, index) => index + 1);

    expect(parseFolds(JSON.stringify({ active: tooMany, inactive: [3] }))).toEqual(NO_FOLDS);
  });

  it("accepts a pane at exactly the bound", () => {
    const atBound = Array.from({ length: MAX_FOLDS_PER_PANE }, (_, index) => index + 1);

    expect(parseFolds(JSON.stringify({ active: atBound, inactive: [] })).active).toHaveLength(
      MAX_FOLDS_PER_PANE,
    );
  });

  it("does not even parse an oversized stored value", () => {
    const huge = JSON.stringify({ active: [], inactive: [], filler: "x".repeat(70_000) });

    expect(parseFolds(huge)).toEqual(NO_FOLDS);
  });

  it("drops duplicate ids", () => {
    expect(parseFolds('{"active":[2,2,"ungrouped","ungrouped"],"inactive":[]}').active).toEqual([
      2,
      "ungrouped",
    ]);
  });
});

describe("fold keys", () => {
  it("round-trips a fold through the request key", () => {
    expect(foldToKey(5)).toEqual({ kind: "group", groupId: 5 });
    expect(foldToKey("ungrouped")).toEqual({ kind: "ungrouped" });
    expect(keyToFold({ kind: "group", groupId: 5 })).toBe(5);
    expect(keyToFold({ kind: "ungrouped" })).toBe("ungrouped");
  });

  it("orders groups by id and puts Ungrouped last, whatever the input order", () => {
    expect(sortedFolds(["ungrouped", 10, 2, 7])).toEqual([2, 7, 10, "ungrouped"]);
  });

  it("keys storage by profile folder", () => {
    expect(foldsStorageKey("C:\\profiles\\a")).not.toBe(foldsStorageKey("C:\\profiles\\b"));
  });
});
