import { describe, expect, it } from "vitest";

import { defCacheBuildSeconds } from "@/utils/defCache";

describe("defCacheBuildSeconds", () => {
  it("extracts the seconds from a real cache-created line, stripping color tags", () => {
    const lines = [
      "DEFCACHE: <color=green>Cache not found or got purged!</color>",
      "DEFCACHE: <color=white>Cache created!</color> creating cache took <color=green>8 seconds</color>",
    ];

    expect(defCacheBuildSeconds(lines)).toBe(8);
  });

  it("returns null when no line carries a duration", () => {
    const lines = [
      "DEFCACHE: <color=yellow>Cache disabled from</color>",
      "DEFCACHE: Mod list changed! Deleting cache",
    ];

    expect(defCacheBuildSeconds(lines)).toBeNull();
  });

  it("returns null for an empty list (no log imported)", () => {
    expect(defCacheBuildSeconds([])).toBeNull();
  });

  it("returns the last build's duration when the cache was rebuilt more than once", () => {
    const lines = [
      "DEFCACHE: <color=white>Cache created!</color> creating cache took <color=green>8 seconds</color>",
      "DEFCACHE: Mod list changed! Deleting cache",
      "DEFCACHE: <color=white>Cache created!</color> creating cache took <color=green>3 seconds</color>",
    ];

    expect(defCacheBuildSeconds(lines)).toBe(3);
  });
});
