import { describe, expect, it } from "vitest";
import { createI18n } from "vue-i18n";
import { buildPluralRules } from "@/i18n/format";
import en from "@/locales/en.json";
import type { ActionDto } from "@/types/generated/ActionDto";
import type { MergeModEntryDto } from "@/types/generated/MergeModEntryDto";
import type { MergeStateDto } from "@/types/generated/MergeStateDto";
import {
  describeAction,
  describeDefKey,
  describeMergeModGroups,
  describeMergeState,
  describeSkippedMergeReason,
  formatBytes,
  formatPercentLabel,
  inspectHref,
  inspectRoute,
} from "@/utils/format";

/** A real `en` translator, for {@link describeMergeModGroups}'s own tests — it builds a composite, variable-clause sentence a plain `{ key, params }` assertion can't express, so its tests assert the real rendered text instead (using the actual catalogue, not a stub). */
const { t: translateEn } = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: false,
  messages: { en },
  pluralRules: buildPluralRules(),
}).global;

describe("formatBytes", () => {
  it("renders a byte count under 1 KiB with no unit conversion", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
  });

  it("renders KiB with two decimal places", () => {
    expect(formatBytes(1536)).toBe("1.50 KiB");
  });

  it("renders MiB with two decimal places", () => {
    expect(formatBytes(2 * 1024 * 1024)).toBe("2.00 MiB");
  });

  it("renders GiB with two decimal places", () => {
    expect(formatBytes(3 * 1024 * 1024 * 1024)).toBe("3.00 GiB");
  });
});

describe("describeDefKey", () => {
  it("renders a real def as defType/defName", () => {
    expect(describeDefKey({ defType: "ThingDef", defName: "Wall" })).toBe("ThingDef/Wall");
  });

  it("renders a texture override's synthetic key as 'texture <path>', never 'texture/<path>'", () => {
    expect(describeDefKey({ defType: "texture", defName: "Things/Wall.png" })).toBe(
      "texture Things/Wall.png",
    );
  });
});

describe("describeAction", () => {
  it("renders a preferWinner action over a texture override without a literal 'texture/' prefix", () => {
    const action: ActionDto = {
      kind: "preferWinner",
      key: { defType: "texture", defName: "Things/Wall.png" },
      winner: "mod.a",
    };

    expect(describeAction(action)).toEqual({
      key: "action.preferWinner",
      params: { winner: "mod.a", defKey: "texture Things/Wall.png" },
    });
  });

  it("renders a merge action over a real def as defType/defName", () => {
    const action: ActionDto = {
      kind: "merge",
      key: { defType: "ThingDef", defName: "Wall" },
      choices: {},
    };

    expect(describeAction(action)).toEqual({
      key: "action.merge",
      params: { defKey: "ThingDef/Wall" },
    });
  });

  it("calls the label resolver for every embedded mod id", () => {
    const action: ActionDto = { kind: "reorder", after: "a.mod", before: "b.mod" };

    expect(describeAction(action, (id) => `<${id}>`)).toEqual({
      key: "action.reorder",
      params: { after: "<a.mod>", before: "<b.mod>" },
    });
  });

  it("renders dropEdge/keepEdge's own edge kind through the edgeKindText resolver, not the raw camelCase wire value", () => {
    const action: ActionDto = {
      kind: "dropEdge",
      after: "a.mod",
      before: "b.mod",
      edgeKind: "loadAfter",
    };

    // Default resolver (no caller-supplied `edgeKindText`): falls back
    // to the raw wire value, same as before this returned a descriptor.
    expect(describeAction(action).params).toMatchObject({ edgeKind: "loadAfter" });

    // A real caller (e.g. a component passing `(kind) => tm(edgeKindLabel(kind))`)
    // gets a real label instead — the bug this migration fixed along the way.
    expect(describeAction(action, undefined, (kind) => `Label(${kind})`).params).toMatchObject({
      edgeKind: "Label(loadAfter)",
    });
  });
});

describe("inspectRoute", () => {
  it("builds a def route, letting vue-router's own param encoder handle the embedded slash", () => {
    expect(inspectRoute("ThingDef/Wall")).toEqual({
      name: "def",
      params: { defRef: "ThingDef/Wall" },
    });
  });
});

describe("inspectHref", () => {
  it("delegates to inspectRoute for a real defRef", () => {
    expect(inspectHref("ThingDef/Wall")).toEqual(inspectRoute("ThingDef/Wall"));
  });

  it("is null when there is no defRef to link to", () => {
    expect(inspectHref(null)).toBeNull();
  });
});

describe("describeMergeState", () => {
  it("renders a complete state as 'merged'", () => {
    expect(describeMergeState({ kind: "complete", opCount: 3 })).toEqual({
      key: "merge.state.merged",
    });
  });

  it("pluralizes an ordinary needs-field-input state by its own unresolved count", () => {
    expect(describeMergeState({ kind: "needsFieldInput", unresolved: 1, total: 5 })).toEqual({
      key: "merge.state.needsInput",
      params: { count: 1 },
      count: 1,
    });
    expect(translateEn("merge.state.needsInput", { count: 1 }, 1)).toBe("1 field needs input");
    expect(translateEn("merge.state.needsInput", { count: 2 }, 2)).toBe("2 fields need input");
  });

  it("renders a cannotMerge state as 'cannot merge'", () => {
    expect(describeMergeState({ kind: "cannotMerge", reason: "x is missing" })).toEqual({
      key: "merge.state.cannotMerge",
    });
  });

  it("a guarded needs-field-input state names confirming the winner, never a field count", () => {
    expect(
      describeMergeState({ kind: "needsFieldInput", unresolved: 9, total: 9 }, "thingClass"),
    ).toEqual({ key: "merge.state.structuralGuard" });
  });

  it("an ordinary (non-guarded) needs-field-input state is unaffected by a null/omitted guard field", () => {
    const state: MergeStateDto = { kind: "needsFieldInput", unresolved: 1, total: 5 };
    expect(describeMergeState(state, null)).toEqual(describeMergeState(state));
  });
});

function mergeModEntry(overrides: Partial<MergeModEntryDto> = {}): MergeModEntryDto {
  return {
    key: "def_override:ThingDef/Wall:[a.mod,b.mod]",
    defKey: { defType: "ThingDef", defName: "Wall" },
    kind: "defOverride",
    state: { kind: "complete", opCount: 1 },
    opCount: 1,
    dependsOn: [],
    patchFile: null,
    structuralGuardField: null,
    ...overrides,
  };
}

describe("describeSkippedMergeReason", () => {
  it("falls back to 'still needs input' when no matching entry was found", () => {
    expect(describeSkippedMergeReason(undefined)).toEqual({ key: "merge.skipped.stillNeedsInput" });
  });

  it("names confirming the winner for a structurally-guarded entry, the same phrasing describeMergeState uses", () => {
    const entry = mergeModEntry({
      state: { kind: "needsFieldInput", unresolved: 9, total: 9 },
      structuralGuardField: "thingClass",
    });
    expect(describeSkippedMergeReason(entry)).toEqual(
      describeMergeState({ kind: "needsFieldInput", unresolved: 9, total: 9 }, "thingClass"),
    );
  });

  it("still reads 'still needs input' for a genuine (non-guarded) needs-field-input entry", () => {
    const entry = mergeModEntry({ state: { kind: "needsFieldInput", unresolved: 2, total: 5 } });
    expect(describeSkippedMergeReason(entry)).toEqual({ key: "merge.skipped.stillNeedsInput" });
  });

  it("names a cannotMerge entry's own reason verbatim, not a generic message", () => {
    const entry = mergeModEntry({
      kind: "asset",
      defKey: null,
      state: { kind: "cannotMerge", reason: "Things/Foo was not found among a.mod's files" },
    });
    expect(describeSkippedMergeReason(entry)).toBe("Things/Foo was not found among a.mod's files");
  });
});

describe("describeMergeModGroups", () => {
  it("is null when nothing reaches the merge mod", () => {
    expect(describeMergeModGroups([], translateEn, "en")).toBeNull();
    expect(
      describeMergeModGroups(
        [mergeModEntry({ state: { kind: "needsFieldInput", unresolved: 1, total: 1 } })],
        translateEn,
        "en",
      ),
    ).toBeNull();
  });

  it("counts a single patch-collision merge and a single def-override merge separately, singular", () => {
    const entries = [
      mergeModEntry({ kind: "patchCollision", key: "patch_collision:a" }),
      mergeModEntry({ kind: "defOverride", key: "def_override:b" }),
    ];
    expect(describeMergeModGroups(entries, translateEn, "en")).toBe(
      "Will write 1 patch-collision merge (auto-suggested where undecided) and " +
        "1 def-override merge (explicitly decided).",
    );
  });

  it("pluralizes each group independently and omits a zero group entirely", () => {
    const entries = [
      mergeModEntry({ kind: "patchCollision", key: "patch_collision:a" }),
      mergeModEntry({ kind: "patchCollision", key: "patch_collision:b" }),
    ];
    expect(describeMergeModGroups(entries, translateEn, "en")).toBe(
      "Will write 2 patch-collision merges (auto-suggested where undecided).",
    );
  });

  it("excludes an entry that hasn't actually rendered (needsFieldInput/cannotMerge) from either count", () => {
    const entries = [
      mergeModEntry({ kind: "patchCollision", key: "patch_collision:a" }),
      mergeModEntry({
        kind: "defOverride",
        key: "def_override:guarded",
        state: { kind: "needsFieldInput", unresolved: 3, total: 3 },
        structuralGuardField: "thingClass",
      }),
    ];
    expect(describeMergeModGroups(entries, translateEn, "en")).toBe(
      "Will write 1 patch-collision merge (auto-suggested where undecided).",
    );
  });

  // A `ShipAsset` decision gets its own,
  // unlabelled clause — the checkbox this line sits under mentions
  // "merge/asset decision", so a profile whose only decision is a
  // `ShipAsset` must still get a summary line.
  it("counts a shipped asset in its own clause", () => {
    const entries = [mergeModEntry({ kind: "asset", defKey: null, key: "ship_asset:a" })];
    expect(describeMergeModGroups(entries, translateEn, "en")).toBe("Will write 1 shipped asset.");
  });

  it("joins all three clauses when every kind contributes", () => {
    const entries = [
      mergeModEntry({ kind: "patchCollision", key: "patch_collision:a" }),
      mergeModEntry({ kind: "defOverride", key: "def_override:b" }),
      mergeModEntry({ kind: "asset", defKey: null, key: "ship_asset:c" }),
    ];
    expect(describeMergeModGroups(entries, translateEn, "en")).toBe(
      "Will write 1 patch-collision merge (auto-suggested where undecided), " +
        "1 def-override merge (explicitly decided), and 1 shipped asset.",
    );
  });

  // A decided merge whose preview is
  // `Complete { opCount: 0 }` (every field already resolves to the
  // winner's own value) still reaches `RenderMergeMod`'s own `plans`,
  // but writes no real content — it must not inflate a "will write"
  // count.
  it("excludes a complete but zero-op entry from every count", () => {
    const entries = [
      mergeModEntry({
        kind: "defOverride",
        key: "def_override:zero-op",
        state: { kind: "complete", opCount: 0 },
        opCount: 0,
      }),
    ];
    expect(describeMergeModGroups(entries, translateEn, "en")).toBeNull();
  });
});

describe("formatPercentLabel", () => {
  it("renders a whole percentage with no decimal place", () => {
    expect(formatPercentLabel(0)).toBe("0%");
    expect(formatPercentLabel(50)).toBe("50%");
  });

  it("renders a complete bar as 100%, never 100.0%", () => {
    expect(formatPercentLabel(100)).toBe("100%");
  });

  it("rounds a repeating fraction to one decimal place", () => {
    expect(formatPercentLabel(100 / 3)).toBe("33.3%");
  });

  it("rounds the real scan's long expansion to one decimal place", () => {
    expect(formatPercentLabel(99.30174825174825)).toBe("99.3%");
  });

  it("truncates rather than rounds, so a working bar never reads 100%", () => {
    expect(formatPercentLabel(99.96)).toBe("99.9%");
    expect(formatPercentLabel(99.999999)).toBe("99.9%");
  });

  it("drops a trailing zero left by truncation", () => {
    expect(formatPercentLabel(12.04)).toBe("12%");
  });
});
