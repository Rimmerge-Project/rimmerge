import { describe, expect, it } from "vitest";
import type { RuleOriginDto } from "@/types/generated/RuleOriginDto";
import { isImportedOrigin, ruleOriginLabel } from "@/utils/rule";

describe("isImportedOrigin", () => {
  it("is false only for userDecision", () => {
    expect(isImportedOrigin("userDecision")).toBe(false);
    expect(isImportedOrigin("rimSortUser")).toBe(true);
    expect(isImportedOrigin("rimSortCommunity")).toBe(true);
    expect(isImportedOrigin("steamDb")).toBe(true);
  });
});

describe("ruleOriginLabel", () => {
  const ALL_ORIGINS: readonly RuleOriginDto[] = [
    "userDecision",
    "rimSortUser",
    "rimSortCommunity",
    "steamDb",
  ];

  it.each(ALL_ORIGINS)("reuses the matching layer.* key for %s", (origin) => {
    expect(ruleOriginLabel(origin)).toEqual({ key: `layer.${origin}` });
  });
});
