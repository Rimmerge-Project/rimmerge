import { describe, expect, it } from "vitest";

import { modRow } from "@/components/mods/modListRow.test-support";
import { marksForLabel } from "@/utils/modRowMarks";

const searched = {
  ...modRow({ modId: "author.alpha", name: "Alpha Tools" }),
  search: { name: [{ start: 0, end: 5 }], id: [{ start: 7, end: 12 }] },
};

describe("marksForLabel", () => {
  it("uses the name's ranges when the label is the name", () => {
    expect(marksForLabel(searched, "Alpha Tools")).toEqual([{ start: 0, end: 5 }]);
  });

  it("uses the id's ranges when the label is the id", () => {
    expect(marksForLabel(searched, "author.alpha")).toEqual([{ start: 7, end: 12 }]);
  });

  it("marks nothing in a label that is neither the name nor the id", () => {
    expect(marksForLabel(searched, "Some Other Name")).toEqual([]);
  });

  it("marks nothing when the row carries no search", () => {
    expect(marksForLabel(modRow({ modId: "a", name: "A" }), "A")).toEqual([]);
  });

  it("prefers the name's ranges when the name is the id's own text", () => {
    const missing = {
      ...modRow({ modId: "x.y", name: "x.y", missing: true }),
      search: { name: [{ start: 2, end: 3 }], id: [{ start: 0, end: 1 }] },
    };

    expect(marksForLabel(missing, "x.y")).toEqual([{ start: 2, end: 3 }]);
  });
});
