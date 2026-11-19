import { describe, expect, it } from "vitest";

import type { PendingActiveChangesDto } from "@/types/generated/PendingActiveChangesDto";
import { hasOwnPendingChanges } from "@/utils/pendingActiveChanges";

const NONE: PendingActiveChangesDto = {
  unscanned: { added: [], removed: [] },
  unapplied: { added: [], removed: [] },
  orderUnapplied: false,
};

describe("hasOwnPendingChanges", () => {
  it("is false for an unedited session", () => {
    expect(hasOwnPendingChanges(NONE)).toBe(false);
  });

  it.each([
    ["an unscanned activation", { ...NONE, unscanned: { added: ["a"], removed: [] } }],
    ["an unscanned deactivation", { ...NONE, unscanned: { added: [], removed: ["a"] } }],
    ["an unapplied addition", { ...NONE, unapplied: { added: ["a"], removed: [] } }],
    ["an unapplied removal", { ...NONE, unapplied: { added: [], removed: ["a"] } }],
    ["a settled reorder alone", { ...NONE, orderUnapplied: true }],
  ] satisfies [string, PendingActiveChangesDto][])("is true for %s", (_label, pending) => {
    expect(hasOwnPendingChanges(pending)).toBe(true);
  });
});
