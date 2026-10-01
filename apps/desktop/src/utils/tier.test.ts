import { describe, expect, it } from "vitest";

import type { TierDto } from "@/types/generated/TierDto";
import { tierLabel } from "@/utils/tier";

const ALL_TIERS: readonly TierDto[] = ["core", "dlc", "top", "body", "bottom"];

describe("tierLabel", () => {
  it.each(ALL_TIERS)("returns a distinct, en-namespaced key for %s", (tier) => {
    expect(tierLabel(tier).key).toBe(`order.tier.${tier}`);
  });
});
