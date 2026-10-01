import { describe, expect, it } from "vitest";

import { describeFindingKey, describesFindingWhenAccepted } from "@/utils/finding";

describe("describeFindingKey", () => {
  it("falls back to the kind's own label for a recognized prefix", () => {
    expect(describeFindingKey("duplicate_template_name:Base:[a.mod,b.mod]")).toEqual({
      key: "finding.kind.duplicateTemplateName",
    });
    expect(
      describeFindingKey("runtime_patch_collision:Verse.Pawn:Kill:[a.mod,b.mod,c.mod]"),
    ).toEqual({ key: "finding.kind.runtimePatchCollision" });
  });

  it("falls back to a verbatim descriptor of the raw key when the prefix doesn't match a known kind", () => {
    expect(describeFindingKey("not_a_real_kind:a.mod")).toEqual({
      key: "common.verbatim",
      params: { text: "not_a_real_kind:a.mod" },
    });
  });
});

describe("describesFindingWhenAccepted", () => {
  it("is true for the four kinds whose Accept suggestion carries no data of its own", () => {
    expect(describesFindingWhenAccepted("runtime_patch_collision:Verse.Pawn:Kill:[a.mod]")).toBe(
      true,
    );
    expect(describesFindingWhenAccepted("rule_overruled:a.mod:b.mod:steam_db")).toBe(true);
    expect(describesFindingWhenAccepted("placement_overruled:a.mod:bottom:steam_db")).toBe(true);
    expect(describesFindingWhenAccepted("placement_questioned:a.mod:top:uses_type")).toBe(true);
  });

  it("is false for a kind whose Accept action already names its own def or mod", () => {
    expect(describesFindingWhenAccepted("def_override:ThingDef/Wall:[a.mod,b.mod]")).toBe(false);
    expect(describesFindingWhenAccepted("missing_mod:a.mod")).toBe(false);
  });
});
