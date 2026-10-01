import { describe, expect, it } from "vitest";

import type { ScanStageDto } from "@/types/generated/ScanStageDto";
import { scanStageLabel } from "@/utils/scanStage";

const ALL_STAGES: readonly ScanStageDto[] = [
  "discovering",
  "scanning",
  "analyzing",
  "collectingTagEvidence",
  "done",
];

describe("scanStageLabel", () => {
  it.each(ALL_STAGES)("returns a distinct, stage-named key for %s", (stage) => {
    expect(scanStageLabel(stage).key).toBe(`setup.stage.${stage}`);
  });
});
