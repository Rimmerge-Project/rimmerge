import { describe, expect, it } from "vitest";

import { describeDecisionRejection } from "@/utils/decisionRejection";

const label = (id: string): string => `Label(${id})`;

describe("describeDecisionRejection", () => {
  it("names the rejected action", () => {
    expect(describeDecisionRejection({ kind: "notPatchable", action: "reorder" }, label)).toEqual({
      key: "patches.detail.rejection.notPatchable",
      params: { action: { key: "patches.detail.rejection.actionName.reorder" } },
    });
  });

  it("resolves the out-of-scope mod through the label resolver", () => {
    expect(
      describeDecisionRejection(
        { kind: "choiceOutsideScope", path: "label", modId: "c.mod" },
        label,
      ),
    ).toEqual({
      key: "patches.detail.rejection.choiceOutsideScope",
      params: { path: "label", mod: "Label(c.mod)" },
    });
    expect(
      describeDecisionRejection({ kind: "assetOutsideScope", modId: "c.mod" }, label).params,
    ).toEqual({
      mod: "Label(c.mod)",
    });
  });

  it("has a parameterless message for an out-of-scope key", () => {
    expect(describeDecisionRejection({ kind: "outOfScope" }, label)).toEqual({
      key: "patches.detail.rejection.outOfScope",
    });
  });
});
