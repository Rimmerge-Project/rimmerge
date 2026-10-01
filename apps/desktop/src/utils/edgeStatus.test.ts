import { describe, expect, it } from "vitest";

import type { EdgeStatusDto } from "@/types/generated/EdgeStatusDto";
import { edgeStatusLabel } from "@/utils/edgeStatus";

const ALL_STATUSES: readonly EdgeStatusDto[] = ["satisfied", "violated", "unevaluated"];

describe("edgeStatusLabel", () => {
  it.each(ALL_STATUSES)("returns a distinct, en-namespaced key for %s", (status) => {
    expect(edgeStatusLabel(status).key).toBe(`edgeStatus.${status}`);
  });
});
