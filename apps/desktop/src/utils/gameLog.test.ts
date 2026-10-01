import { describe, expect, it } from "vitest";

import type { PatchFailureSummaryDto } from "@/types/generated/PatchFailureSummaryDto";
import type { VerifyOperationDto } from "@/types/generated/VerifyOperationDto";
import { joinPredictedVsObserved, normalizeLogText } from "@/utils/gameLog";

describe("normalizeLogText", () => {
  it("collapses runs of whitespace, including newlines and tabs, to a single space", () => {
    expect(normalizeLogText("a\n\tb   c")).toBe("a b c");
  });

  it("trims leading and trailing whitespace", () => {
    expect(normalizeLogText("  a b  ")).toBe("a b");
  });

  it("returns an empty string for an empty or all-whitespace input", () => {
    expect(normalizeLogText("")).toBe("");
    expect(normalizeLogText("   ")).toBe("");
  });
});

function operation(modId: string, text: string): VerifyOperationDto {
  return { modId, operation: text, defs: [] };
}

function observedFailure(modId: string, text: string): PatchFailureSummaryDto {
  return {
    attribution: { kind: "mod", modId },
    operation: text,
    sourceFile: null,
    stackTrace: null,
  };
}

describe("joinPredictedVsObserved", () => {
  it("matches a predicted operation to its observed failure by (modId, normalized operation)", () => {
    const predicted = operation("a.mod", "Verse.PatchOperationAdd(Defs/ThingDef)");
    // The log renders the identical operation as one line — a multi-line
    // xpath's whitespace must still join.
    const observed = observedFailure("a.mod", "Verse.PatchOperationAdd(Defs/ThingDef)");

    const result = joinPredictedVsObserved([predicted], [observed]);

    expect(result.matched).toHaveLength(1);
    expect(result.matched[0]?.predicted).toBe(predicted);
    expect(result.matched[0]?.observed).toEqual([observed]);
    expect(result.predictedOnly).toHaveLength(0);
    expect(result.observedOnly).toHaveLength(0);
  });

  it("normalizes whitespace on both sides before comparing (a captured multi-line xpath)", () => {
    const predicted = operation("a.mod", 'Verse.PatchOperationAdd(xpath="\n\t<tabs>Defs/X")');
    const observed = observedFailure("a.mod", 'Verse.PatchOperationAdd(xpath=" <tabs>Defs/X")');

    const result = joinPredictedVsObserved([predicted], [observed]);

    expect(result.matched).toHaveLength(1);
  });

  it("puts an unmatched prediction in predictedOnly", () => {
    const predicted = operation("a.mod", "Verse.PatchOperationAdd(Defs/ThingDef)");

    const result = joinPredictedVsObserved([predicted], []);

    expect(result.predictedOnly).toEqual([predicted]);
    expect(result.matched).toHaveLength(0);
  });

  it("puts an unmatched observed failure in observedOnly, never dropped", () => {
    const observed = observedFailure("a.mod", "Verse.PatchOperationAdd(Defs/Unrelated)");

    const result = joinPredictedVsObserved([], [observed]);

    expect(result.observedOnly).toEqual([observed]);
  });

  it("puts an unattributed observed failure in observedOnly — it can never join a mod-scoped prediction", () => {
    const predicted = operation("a.mod", "Verse.PatchOperationAdd(Defs/ThingDef)");
    const observed: PatchFailureSummaryDto = {
      attribution: { kind: "unattributed", raw: "[Some Mod]" },
      operation: "Verse.PatchOperationAdd(Defs/ThingDef)",
      sourceFile: null,
      stackTrace: null,
    };

    const result = joinPredictedVsObserved([predicted], [observed]);

    expect(result.observedOnly).toEqual([observed]);
    expect(result.predictedOnly).toEqual([predicted]);
  });

  it("never over-counts an OR-list operation shared by many def targets as multiple predictions", () => {
    // One grouped VerifyOperationDto already stands for every def target it
    // matched — a real install can see one operation (a sequence-wrapper
    // mod's compatibility-patch operation, say) share many def targets while
    // RimWorld's own log reports its failure exactly once. Give this fixture
    // genuine multiplicity (3
    // def targets, not the 0- or 1-target shape every other test here
    // uses) so a regression that joins per `predicted.defs[i]` instead
    // of per grouped operation would actually fail this test.
    const predicted: VerifyOperationDto = {
      modId: "a.mod",
      operation:
        'Verse.PatchOperationAdd(Defs/ThingDef[defName="Wall" or defName="Door" or defName="Roof"]/comps)',
      defs: [
        {
          defKey: { defType: "ThingDef", defName: "Wall" },
          selector: "defName",
          leafXpath: null,
          cause: { kind: "deadTarget" },
          reorderKind: null,
          reorder: null,
        },
        {
          defKey: { defType: "ThingDef", defName: "Door" },
          selector: "defName",
          leafXpath: null,
          cause: { kind: "deadTarget" },
          reorderKind: null,
          reorder: null,
        },
        {
          defKey: { defType: "ThingDef", defName: "Roof" },
          selector: "defName",
          leafXpath: null,
          cause: { kind: "deadTarget" },
          reorderKind: null,
          reorder: null,
        },
      ],
    };
    const observed = observedFailure(
      "a.mod",
      'Verse.PatchOperationAdd(Defs/ThingDef[defName="Wall" or defName="Door" or defName="Roof"]/comps)',
    );

    const result = joinPredictedVsObserved([predicted], [observed]);

    expect(result.matched).toHaveLength(1);
    // The per-def detail is kept, not multiplied into three rows.
    expect(result.matched[0]?.predicted.defs).toHaveLength(3);
    expect(result.matched[0]?.observed).toHaveLength(1);
  });
});
