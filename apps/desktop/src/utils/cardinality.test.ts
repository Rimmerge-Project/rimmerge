import { describe, expect, it } from "vitest";

import type { CardinalityDto } from "@/types/generated/CardinalityDto";
import { cardinalityLabel } from "@/utils/cardinality";

const ALL_CARDINALITIES: readonly CardinalityDto[] = ["scalar", "list"];

describe("cardinalityLabel", () => {
  it.each(ALL_CARDINALITIES)("returns a distinct, en-namespaced key for %s", (cardinality) => {
    expect(cardinalityLabel(cardinality).key).toBe(`cardinality.${cardinality}`);
  });
});
