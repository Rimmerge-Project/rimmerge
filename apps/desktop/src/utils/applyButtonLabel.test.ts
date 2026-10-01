import { describe, expect, it } from "vitest";
import { applyButtonLabel } from "@/utils/applyButtonLabel";

describe("applyButtonLabel", () => {
  it.each([
    [true, true, "apply.dialog.applyButton"],
    [true, false, "apply.dialog.applyButton"],
    [false, true, "apply.dialog.writeMergeModButton"],
    [false, false, "apply.dialog.saveDecisionsButton"],
  ])("modsConfig=%s mergeMod=%s -> %s", (writeModsConfig, writeMergeMod, key) => {
    expect(applyButtonLabel({ writeModsConfig, writeMergeMod }).key).toBe(key);
  });
});
