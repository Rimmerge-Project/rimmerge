import { describe, expect, it } from "vitest";

import type { PlacementDto } from "@/types/generated/PlacementDto";
import { placementLabel } from "@/utils/placement";

const ALL_PLACEMENTS: readonly PlacementDto[] = ["top", "bottom"];

describe("placementLabel", () => {
  it.each(ALL_PLACEMENTS)("returns a distinct, en-namespaced key for %s", (placement) => {
    expect(placementLabel(placement).key).toBe(`placement.${placement}`);
  });
});
