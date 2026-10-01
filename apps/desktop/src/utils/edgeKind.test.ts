import { describe, expect, it } from "vitest";

import type { EdgeKindDto } from "@/types/generated/EdgeKindDto";
import { edgeKindLabel } from "@/utils/edgeKind";

/**
 * Every {@link EdgeKindDto} value, kept as a literal list (not derived
 * from the type) so a newly-added kind fails this test until it's added
 * here too — the same belt-and-suspenders `assertNever` already gives at
 * the type level, but enforced at the value level as well.
 */
const ALL_EDGE_KINDS: readonly EdgeKindDto[] = [
  "assemblyRef",
  "forceLoadAfter",
  "forceLoadBefore",
  "loadAfter",
  "loadBefore",
  "modDependency",
  "findMod",
  "ifModActive",
  "patchTargetsDef",
  "mayRequire",
  "patchInjectedNode",
  "assemblyVersionPrecedence",
  "usesType",
  "parentTemplate",
  "patchRemovedNode",
  "retextureAfterOwner",
  "defOverrideAfterOrigin",
  "patchSelectsInjectedNode",
  "patchInvalidatesPredicate",
  "patchRemovedNodeCosmetic",
  "replaceDiscardsAddition",
];

describe("edgeKindLabel", () => {
  it.each(ALL_EDGE_KINDS)("returns a distinct, en-namespaced key for %s", (kind) => {
    const descriptor = edgeKindLabel(kind);
    expect(descriptor.key.startsWith("edgeKind.")).toBe(true);
    // The point of this utility is to stop rendering the raw camelCase
    // wire value — assert the key's own last segment is never just the
    // kind echoed back with no translation layer at all.
    expect(descriptor.key).toBe(`edgeKind.${kind}`);
  });

  it("labels the three heuristic (Inferred-layer) edge kinds", () => {
    expect(edgeKindLabel("patchRemovedNode")).toEqual({ key: "edgeKind.patchRemovedNode" });
    expect(edgeKindLabel("retextureAfterOwner")).toEqual({ key: "edgeKind.retextureAfterOwner" });
    expect(edgeKindLabel("defOverrideAfterOrigin")).toEqual({
      key: "edgeKind.defOverrideAfterOrigin",
    });
  });

  it("labels the node-path-selection kind split out of usesType", () => {
    expect(edgeKindLabel("patchSelectsInjectedNode")).toEqual({
      key: "edgeKind.patchSelectsInjectedNode",
    });
  });

  it("labels the predicate-invalidation kind", () => {
    expect(edgeKindLabel("patchInvalidatesPredicate")).toEqual({
      key: "edgeKind.patchInvalidatesPredicate",
    });
  });

  it("labels the cosmetic patch-removed-node kind", () => {
    expect(edgeKindLabel("patchRemovedNodeCosmetic")).toEqual({
      key: "edgeKind.patchRemovedNodeCosmetic",
    });
  });
});
