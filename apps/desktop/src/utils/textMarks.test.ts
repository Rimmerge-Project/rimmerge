import { afterEach, describe, expect, it, vi } from "vitest";

import { splitMarked } from "@/utils/textMarks";

/** The runs as `[text, isMarked]` pairs, easiest to read in an assertion. */
function runs(text: string, ranges: { start: number; end: number }[]): [string, boolean][] {
  return splitMarked(text, ranges).map((segment) => [segment.text, segment.isMarked]);
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("splitMarked", () => {
  it("returns the whole text as one plain run when nothing matched", () => {
    expect(runs("Alpha", [])).toEqual([["Alpha", false]]);
  });

  it("returns nothing for an empty text", () => {
    expect(runs("", [])).toEqual([]);
  });

  it("marks a range at the start", () => {
    expect(runs("Alpha", [{ start: 0, end: 2 }])).toEqual([
      ["Al", true],
      ["pha", false],
    ]);
  });

  it("marks a range at the end", () => {
    expect(runs("Alpha", [{ start: 3, end: 5 }])).toEqual([
      ["Alp", false],
      ["ha", true],
    ]);
  });

  it("marks a range in the middle between two plain runs", () => {
    expect(runs("Alpha", [{ start: 1, end: 3 }])).toEqual([
      ["A", false],
      ["lp", true],
      ["ha", false],
    ]);
  });

  it("marks the whole text as one run", () => {
    expect(runs("Alpha", [{ start: 0, end: 5 }])).toEqual([["Alpha", true]]);
  });

  it("keeps two ranges that touch as two marked runs, with no empty run between", () => {
    expect(
      runs("aaaa", [
        { start: 0, end: 2 },
        { start: 2, end: 4 },
      ]),
    ).toEqual([
      ["aa", true],
      ["aa", true],
    ]);
  });

  it("puts a plain run between ranges that are apart", () => {
    expect(
      runs("abcabc", [
        { start: 0, end: 1 },
        { start: 3, end: 4 },
      ]),
    ).toEqual([
      ["a", true],
      ["bc", false],
      ["a", true],
      ["bc", false],
    ]);
  });

  it("sorts ranges that arrive out of order and joins ones that overlap", () => {
    expect(
      runs("abcdefgh", [
        { start: 4, end: 6 },
        { start: 0, end: 3 },
        { start: 2, end: 5 },
      ]),
    ).toEqual([
      ["abcdef", true],
      ["gh", false],
    ]);
  });

  it("slices by UTF-16 code units, so a character outside the BMP counts as two", () => {
    // U+1D7D8 is one character and two code units; "ab" sits at units 2 to 4.
    expect(runs("\u{1d7d8}ab", [{ start: 2, end: 4 }])).toEqual([
      ["\u{1d7d8}", false],
      ["ab", true],
    ]);
  });

  describe("a range that cannot be a slice of the text", () => {
    const text = "Alpha";
    const bad: [string, { start: number; end: number }][] = [
      ["past the end", { start: 3, end: 9 }],
      ["negative", { start: -1, end: 2 }],
      ["empty", { start: 2, end: 2 }],
      ["reversed", { start: 4, end: 1 }],
      ["fractional", { start: 0.5, end: 2 }],
      ["not a number", { start: Number.NaN, end: 2 }],
    ];

    it.each(bad)("is dropped when %s, leaving the text plain and whole", (_why, range) => {
      vi.spyOn(console, "warn").mockImplementation(() => {});

      expect(runs(text, [range])).toEqual([["Alpha", false]]);
    });

    it("is dropped and reported with console.warn in development, while valid ranges still show", () => {
      const warn = vi.spyOn(console, "warn").mockImplementation(() => {});

      const result = runs(text, [
        { start: 0, end: 2 },
        { start: 3, end: 99 },
      ]);

      expect(result).toEqual([
        ["Al", true],
        ["pha", false],
      ]);
      expect(warn).toHaveBeenCalledTimes(1);
    });

    it("is dropped when it would cut a surrogate pair in two", () => {
      vi.spyOn(console, "warn").mockImplementation(() => {});
      const pair = "a\u{1d7d8}b";

      expect(runs(pair, [{ start: 2, end: 3 }])).toEqual([[pair, false]]);
      expect(runs(pair, [{ start: 1, end: 2 }])).toEqual([[pair, false]]);
    });
  });

  it("always rejoins to the original text, whatever the ranges", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const text = "日本 Mod \u{1d7d8}x";
    const ranges = [
      { start: 0, end: 1 },
      { start: 3, end: 6 },
      { start: 50, end: 60 },
      { start: 8, end: 10 },
    ];

    expect(
      splitMarked(text, ranges)
        .map((segment) => segment.text)
        .join(""),
    ).toBe(text);
  });
});
