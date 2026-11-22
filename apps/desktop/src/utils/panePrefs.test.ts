import { describe, expect, it } from "vitest";

import { DEFAULT_PANE_PREFS, parsePanePrefs, serializePanePrefs } from "@/utils/panePrefs";

const GOOD = {
  active: { sort: { key: "name", direction: "descending" }, view: "groups" },
  inactive: { sort: { key: "lastUpdated", direction: "ascending" }, view: "list" },
} as const;

describe("parsePanePrefs", () => {
  it("reads back exactly what serializePanePrefs wrote", () => {
    expect(parsePanePrefs(serializePanePrefs(GOOD))).toEqual(GOOD);
  });

  it("is the defaults when nothing is stored", () => {
    expect(parsePanePrefs(null)).toEqual(DEFAULT_PANE_PREFS);
  });

  it.each([
    ["malformed JSON", "{not json"],
    ["a JSON array", "[]"],
    ["JSON null", "null"],
    ["a number", "7"],
    ["an oversized value", `{"pad":"${"x".repeat(5000)}"}`],
  ])("is the defaults for %s", (_name, stored) => {
    expect(parsePanePrefs(stored)).toEqual(DEFAULT_PANE_PREFS);
  });

  it.each([
    ["a removed sort key", { sort: { key: "author", direction: "ascending" }, view: "list" }],
    ["a garbage direction", { sort: { key: "name", direction: "sideways" }, view: "list" }],
    ["an unknown view", { sort: { key: "name", direction: "ascending" }, view: "tree" }],
    ["a missing view", { sort: { key: "name", direction: "ascending" } }],
    ["a missing sort", { view: "groups" }],
    ["a non-object entry", "name"],
  ])("gives a pane with %s its own default and leaves the other pane alone", (_name, bad) => {
    const parsed = parsePanePrefs(JSON.stringify({ active: bad, inactive: GOOD.inactive }));

    expect(parsed.active).toEqual(DEFAULT_PANE_PREFS.active);
    expect(parsed.inactive).toEqual(GOOD.inactive);
  });

  it("refuses load order as the Inactive pane's sort", () => {
    const stored = JSON.stringify({
      active: GOOD.active,
      inactive: { sort: { key: "loadOrder", direction: "ascending" }, view: "groups" },
    });

    expect(parsePanePrefs(stored).inactive).toEqual(DEFAULT_PANE_PREFS.inactive);
  });

  it("keeps a non-load-order Active sort as stored, never turning it into load order", () => {
    const stored = JSON.stringify({ active: GOOD.active, inactive: GOOD.inactive });

    expect(parsePanePrefs(stored).active.sort).toEqual({ key: "name", direction: "descending" });
  });

  it("drops fields it does not know instead of carrying them along", () => {
    const stored = JSON.stringify({
      active: { ...GOOD.active, extra: 1, sort: { ...GOOD.active.sort, extra: 2 } },
      inactive: GOOD.inactive,
    });

    expect(parsePanePrefs(stored)).toEqual(GOOD);
  });
});
