import { describe, expect, it } from "vitest";
import { resolutionStatusLabel } from "@/utils/resolutionStatus";

describe("resolutionStatusLabel", () => {
  it("reuses the matching dashboard.table.* key for each status", () => {
    expect(resolutionStatusLabel("auto")).toEqual({ key: "dashboard.table.auto" });
    expect(resolutionStatusLabel("needsInput")).toEqual({ key: "dashboard.table.needsInput" });
    expect(resolutionStatusLabel("userOverridden")).toEqual({ key: "dashboard.table.overridden" });
  });
});
