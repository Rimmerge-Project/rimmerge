import { describe, expect, it } from "vitest";

import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";
import { orderSourceLabel, orderSourceSentenceLabel } from "@/utils/orderSource";

const ALL_SOURCES: readonly OrderSourceDto[] = ["current", "suggested"];

describe("orderSourceLabel", () => {
  it.each(ALL_SOURCES)("reuses the shell's own shell.orderSource.%s key", (source) => {
    expect(orderSourceLabel(source).key).toBe(`shell.orderSource.${source}`);
  });
});

describe("orderSourceSentenceLabel", () => {
  it.each([
    ["current", "shell.orderSource.inSentenceCurrent"],
    ["suggested", "shell.orderSource.inSentenceSuggested"],
  ] as const)("maps %s to %s", (source, key) => {
    expect(orderSourceSentenceLabel(source).key).toBe(key);
  });
});
