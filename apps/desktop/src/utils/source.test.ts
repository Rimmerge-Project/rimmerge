import { describe, expect, it } from "vitest";

import type { SourceDto } from "@/types/generated/SourceDto";
import { sourceLabel } from "@/utils/source";

const ALL_SOURCES: readonly SourceDto[] = ["core", "dlc", "local", "workshop"];

describe("sourceLabel", () => {
  it.each(ALL_SOURCES)("returns a distinct, en-namespaced key for %s", (source) => {
    expect(sourceLabel(source).key).toBe(`source.${source}`);
  });
});
