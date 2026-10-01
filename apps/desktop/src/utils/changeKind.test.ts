import { describe, expect, it } from "vitest";

import type { ChangeKindDto } from "@/types/generated/ChangeKindDto";
import { CHANGE_KIND_ORDER, changeKindLabel } from "@/utils/changeKind";

const ALL_KINDS: readonly ChangeKindDto[] = [
  "ownsDef",
  "ownsTemplate",
  "patchesDef",
  "overridesTexture",
  "overridesSound",
  "overridesKeyedTranslation",
];

describe("changeKindLabel", () => {
  it.each(ALL_KINDS)("returns a distinct, en-namespaced key for %s", (kind) => {
    expect(changeKindLabel(kind).key).toBe(`changeKind.${kind}`);
  });
});

describe("CHANGE_KIND_ORDER", () => {
  it("lists every kind exactly once", () => {
    expect(new Set(CHANGE_KIND_ORDER)).toEqual(new Set(ALL_KINDS));
    expect(CHANGE_KIND_ORDER).toHaveLength(ALL_KINDS.length);
  });
});
