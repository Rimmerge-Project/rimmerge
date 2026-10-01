import { describe, expect, it } from "vitest";

import type { TagProvenanceDto } from "@/types/generated/TagProvenanceDto";
import { tagProvenanceKindLabel } from "@/utils/tagProvenance";

const ALL_KINDS: readonly TagProvenanceDto["kind"][] = ["inferred", "manual"];

describe("tagProvenanceKindLabel", () => {
  it.each(ALL_KINDS)("returns a distinct, en-namespaced key for %s", (kind) => {
    expect(tagProvenanceKindLabel(kind).key).toBe(`tagProvenance.${kind}`);
  });
});
