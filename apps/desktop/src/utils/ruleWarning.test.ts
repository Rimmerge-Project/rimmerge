import { describe, expect, it } from "vitest";

import type { ModKnowledgeValueKindDto } from "@/types/generated/ModKnowledgeValueKindDto";
import type { RuleWarningDto } from "@/types/generated/RuleWarningDto";
import {
  describeRuleWarning,
  ruleWarningsDetail,
  ruleWarningTechnicalDetail,
} from "@/utils/ruleWarning";

describe("describeRuleWarning", () => {
  it("lists the dropped cluster rule ids with the count as the plural selector", () => {
    const message = describeRuleWarning(
      { kind: "droppedClusterRules", ruleIds: ["#0", "#1"] },
      "en",
    );

    expect(message.key).toBe("ruleWarning.droppedClusterRules");
    expect(message.count).toBe(2);
    expect(message.params).toEqual({ count: 2, ids: "#0 and #1" });
  });

  it("phrases an unknown value as a translated noun, never as the Rust identifier", () => {
    const message = describeRuleWarning(
      {
        kind: "unknownModKnowledgeValue",
        section: "patch-operations",
        what: "gateBehaviour",
        value: "x",
      },
      "en",
    );

    expect(message.key).toBe("ruleWarning.unknownModKnowledgeValue");
    expect(message.params).toEqual({
      what: { key: "ruleWarning.what.gateBehaviour" },
      value: "x",
      section: "patch-operations",
    });
  });

  it.each<ModKnowledgeValueKindDto>([
    "templateInvalid",
    "captureMissing",
    "markerLength",
    "idTooLong",
    "templateUncompilable",
  ])("gives the refused-row kind %s a sentence of its own", (what) => {
    const message = describeRuleWarning(
      { kind: "unknownModKnowledgeValue", section: "log-shapes", what, value: "row-1" },
      "en",
    );

    expect(message.key).toBe(`ruleWarning.refusedRow.${what}`);
    expect(message.params).toEqual({ value: "row-1", section: "log-shapes" });
  });

  it("uses the row count as the plural selector for ignored rows over the role limit", () => {
    const message = describeRuleWarning(
      {
        kind: "modKnowledgeRowsOverRoleLimit",
        section: "log-shapes",
        role: "texture_fallbacks",
        ignored: 3,
      },
      "en",
    );

    expect(message.key).toBe("ruleWarning.rowsOverRoleLimit");
    expect(message.count).toBe(3);
    expect(message.params).toEqual({ count: 3, role: "texture_fallbacks", section: "log-shapes" });
  });

  it("names the def type of a framework rule that names no framework", () => {
    const message = describeRuleWarning(
      { kind: "precedenceRuleMissingFramework", defType: "example.PartDef" },
      "en",
    );

    expect(message.key).toBe("ruleWarning.precedenceRuleMissingFramework");
    expect(message.params).toEqual({ defType: "example.PartDef" });
  });

  it("keeps the unreadable-cache reason out of the headline", () => {
    const warning: RuleWarningDto = { kind: "modKnowledgeCacheUnreadable", reason: "bad json" };
    const message = describeRuleWarning(warning, "en");

    expect(message.key).toBe("ruleWarning.modKnowledgeCacheUnreadableHeadline");
    expect(message.params).toBeUndefined();
    expect(ruleWarningTechnicalDetail(warning)).toBe("bad json");
  });
});

describe("ruleWarningsDetail", () => {
  const t = (key: string, params?: Record<string, unknown>) =>
    params === undefined ? key : `${key}:${JSON.stringify(params)}`;

  it("renders each headline, then its technical-details line when it has one", () => {
    const warnings: RuleWarningDto[] = [
      { kind: "modKnowledgeCacheUnreadable", reason: "a" },
      { kind: "precedenceRuleMissingFramework", defType: "example.PartDef" },
    ];

    expect(ruleWarningsDetail(warnings, t, "en").split("\n")).toEqual([
      "ruleWarning.modKnowledgeCacheUnreadableHeadline:{}",
      'common.technicalDetail:{"detail":"a"}',
      'ruleWarning.precedenceRuleMissingFramework:{"defType":"example.PartDef"}',
    ]);
  });
});
