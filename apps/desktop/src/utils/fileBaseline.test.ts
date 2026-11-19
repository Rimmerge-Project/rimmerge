import { describe, expect, it } from "vitest";

import { membershipLabel, staleBaselineMessage } from "@/utils/fileBaseline";

describe("membershipLabel", () => {
  it("has no label for a row that matches the file", () => {
    expect(membershipLabel("none")).toBeNull();
  });

  it("words added and removed rows with their own keys", () => {
    expect(membershipLabel("added")?.text.key).toBe("mods.table.membership.added");
    expect(membershipLabel("removed")?.title.key).toBe("mods.table.membership.removedTitle");
  });
});

describe("staleBaselineMessage", () => {
  it.each([
    ["missing", "mods.fileChange.stale.missing"],
    ["unreadable", "mods.fileChange.stale.unreadable"],
    ["malformed", "mods.fileChange.stale.malformed"],
  ] as const)("words a %s file with its own sentence", (reason, key) => {
    expect(staleBaselineMessage(reason).key).toBe(key);
  });
});
