import { describe, expect, it } from "vitest";

import type { EdgeStrengthDto } from "@/types/generated/EdgeStrengthDto";
import { edgeStrengthLabel } from "@/utils/edgeStrength";

const ALL_STRENGTHS: readonly EdgeStrengthDto[] = [
  "hard",
  "declared",
  "soft",
  "awareness",
  "inferred",
];

describe("edgeStrengthLabel", () => {
  it.each(ALL_STRENGTHS)("returns a distinct, en-namespaced key for %s", (strength) => {
    expect(edgeStrengthLabel(strength).key).toBe(`edgeStrength.${strength}`);
  });
});
