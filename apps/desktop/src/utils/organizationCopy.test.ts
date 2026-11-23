import { describe, expect, it } from "vitest";

import { parentFolder } from "@/utils/organizationCopy";

describe("parentFolder", () => {
  it.each([
    ["C:/Users/me/rimmerge/profiles/abc123", "C:/Users/me/rimmerge/profiles"],
    ["C:\\Users\\me\\profiles\\abc123", "C:\\Users\\me\\profiles"],
    ["C:/Users/me/profiles/abc123/", "C:/Users/me/profiles"],
    ["/home/me/profiles/abc123", "/home/me/profiles"],
    ["C:/profiles", "C:\\"],
  ])("names the folder holding %s", (path, expected) => {
    expect(parentFolder(path)).toBe(expected);
  });

  it.each(["", "profile", "/", "C:"])("has no parent for %j", (path) => {
    expect(parentFolder(path)).toBeNull();
  });
});
