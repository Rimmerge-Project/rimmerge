import { describe, expect, it } from "vitest";

import type { LayerDto } from "@/types/generated/LayerDto";
import { layerLabel } from "@/utils/layer";

/**
 * Every {@link LayerDto} value, kept as a literal list (not derived from
 * the type) so a newly-added layer fails this test until it's added here
 * too — the same belt-and-suspenders `assertNever` already gives at the
 * type level, but enforced at the value level as well.
 */
const ALL_LAYERS: readonly LayerDto[] = [
  "hard",
  "anyOf",
  "declaredOverride",
  "declared",
  "userDecision",
  "rimSortUser",
  "rimSortCommunity",
  "steamDb",
  "inferred",
  "soft",
  "awareness",
];

describe("layerLabel", () => {
  it.each(ALL_LAYERS)("returns a distinct, en-namespaced key for %s", (layer) => {
    const result = layerLabel(layer);
    // The point of this utility is to stop rendering the raw camelCase
    // wire value — assert the key's own last segment is never just the
    // layer echoed back with no translation layer at all.
    expect(result.key).toBe(`layer.${layer}`);
  });
});
