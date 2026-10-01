import { describe, expect, it } from "vitest";
import { createI18n } from "vue-i18n";
import en from "@/locales/en.json";
import type { RationaleDto } from "@/types/generated/RationaleDto";
import { rationaleText } from "@/utils/rationale";

const { t } = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: false,
  messages: { en },
}).global;

const label = (id: string): string => `Label(${id})`;

describe("rationaleText", () => {
  it("resolves the mod id through the label resolver for a contributes-nothing rationale", () => {
    const code: RationaleDto = { kind: "contributesNothing", modId: "a.mod" };
    expect(rationaleText(code, t, label, "en")).toContain("Label(a.mod)");
  });

  it("picks the matching template for an inferred-edge-drop's own edge kind", () => {
    const code: RationaleDto = {
      kind: "edgeDroppedInferred",
      edgeKind: "patchRemovedNode",
      after: "a.mod",
      before: "b.mod",
    };
    const text = rationaleText(code, t, label, "en");
    expect(text).toContain("Label(a.mod)");
    expect(text).toContain("Label(b.mod)");
    expect(text).toContain("removes a node");
  });

  it("resolves mod ids through the label resolver in a winner-carrying rationale", () => {
    const code: RationaleDto = {
      kind: "edgeDroppedWithWinner",
      after: "a.mod",
      before: "b.mod",
      winner: { after: "a.mod", before: "c.mod", layer: "hard", detail: "a load-time fact" },
    };
    const text = rationaleText(code, t, label, "en");
    expect(text).toContain("Label(a.mod)");
    expect(text).toContain("Label(b.mod)");
    expect(text).toContain("a hard (load-time) requirement");
    expect(text).toContain("a load-time fact");
  });

  it("names a user decision's own declared-edge override, not the layered phrase", () => {
    const code: RationaleDto = {
      kind: "ruleOverruledLongerCycle",
      after: "a.mod",
      before: "b.mod",
      origin: "userDecision",
      overridesDeclared: true,
    };
    expect(rationaleText(code, t, label, "en")).toContain("Your own declared-edge override");
  });

  it("names a db-origin rule by its layer phrase and the two resolved mod ids", () => {
    const code: RationaleDto = {
      kind: "ruleOverruledLongerCycle",
      after: "a.mod",
      before: "b.mod",
      origin: "rimSortCommunity",
      overridesDeclared: false,
    };
    const text = rationaleText(code, t, label, "en");
    expect(text).toContain("Label(a.mod)");
    expect(text).toContain("Label(b.mod)");
    expect(text).toContain("a RimSort community rule");
  });

  it("picks the after-direction sentence for a placement pinned at the bottom", () => {
    const code: RationaleDto = {
      kind: "placementOrderingOverridden",
      modId: "a.mod",
      pinned: "b.mod",
      placement: "bottom",
      by: { after: "a.mod", before: "c.mod", layer: "hard", detail: "detail" },
    };
    expect(rationaleText(code, t, label, "en")).toContain("must load after it anyway");
  });

  it("picks the before-direction sentence for a placement pinned at the top", () => {
    const code: RationaleDto = {
      kind: "placementOrderingOverridden",
      modId: "a.mod",
      pinned: "b.mod",
      placement: "top",
      by: { after: "a.mod", before: "c.mod", layer: "hard", detail: "detail" },
    };
    expect(rationaleText(code, t, label, "en")).toContain("must load before it anyway");
  });

  it("pluralizes the promoted-dependents count and lists every promoted mod under the sample threshold", () => {
    const code: RationaleDto = {
      kind: "placementPromotesDependents",
      modId: "a.mod",
      placement: "top",
      promoted: ["b.mod", "c.mod"],
    };
    const text = rationaleText(code, t, label, "en");
    expect(text).toContain("2 other mods are attributed");
    expect(text).toContain("(Label(b.mod) and Label(c.mod))");
  });

  it("uses the singular form and truncates the sample past three promoted mods", () => {
    const code: RationaleDto = {
      kind: "placementPromotesDependents",
      modId: "a.mod",
      placement: "top",
      promoted: ["b.mod"],
    };
    expect(rationaleText(code, t, label, "en")).toContain("1 other mod is attributed");

    const truncated: RationaleDto = {
      kind: "placementPromotesDependents",
      modId: "a.mod",
      placement: "top",
      promoted: ["b.mod", "c.mod", "d.mod", "e.mod"],
    };
    const text = rationaleText(truncated, t, label, "en");
    expect(text).toContain("4 other mods are attributed");
    expect(text).toContain("(e.g. Label(b.mod), Label(c.mod), and Label(d.mod))");
  });

  it("joins the promoted sample with the caller's locale, not a hard-coded comma", () => {
    const code: RationaleDto = {
      kind: "placementPromotesDependents",
      modId: "a.mod",
      placement: "top",
      promoted: ["b.mod", "c.mod"],
    };
    // `en` catalogue text, `pt-BR` list rules: the separator word is the
    // only thing that may differ between the two calls.
    const english = rationaleText(code, t, label, "en");
    const portuguese = rationaleText(code, t, label, "pt-BR");
    expect(english).toContain("(Label(b.mod) and Label(c.mod))");
    expect(portuguese).toContain("(Label(b.mod) e Label(c.mod))");
  });

  it("pluralizes the matching-signal count", () => {
    expect(rationaleText({ kind: "tagInferredSignalCount", count: 1 }, t, label, "en")).toBe(
      "1 matching signal",
    );
    expect(rationaleText({ kind: "tagInferredSignalCount", count: 3 }, t, label, "en")).toBe(
      "3 matching signals",
    );
  });

  it("renders a plain unit-variant rationale", () => {
    expect(rationaleText({ kind: "rejectInferredTag" }, t, label, "en")).toBe(
      "Reject the inferred tag.",
    );
  });

  it("resolves mod ids for a patch-will-fail removed-by rationale", () => {
    const code: RationaleDto = {
      kind: "patchWillFailRemovedBy",
      remover: "a.mod",
      modId: "b.mod",
      xpath: 'Defs/ThingDef[defName="Wall"]/comps',
    };
    const text = rationaleText(code, t, label, "en");
    expect(text).toContain("Label(a.mod)");
    expect(text).toContain("Label(b.mod)");
    expect(text).toContain('Defs/ThingDef[defName="Wall"]/comps');
  });

  it("singularizes and pluralizes the keyed-translation-collision count, naming it in both forms", () => {
    expect(rationaleText({ kind: "keyedTranslationCollision", keyCount: 1 }, t, label, "en")).toBe(
      "These two mods share 1 translation key; the last-loaded one wins.",
    );
    expect(rationaleText({ kind: "keyedTranslationCollision", keyCount: 3 }, t, label, "en")).toBe(
      "These two mods share 3 translation keys; the last-loaded one wins each one.",
    );
  });

  it.each([
    ["findModName", "FindMod name"],
    ["mayRequireId", "MayRequire id"],
  ] as const)(
    "renders a near-miss %s reference as one whole sentence with nothing unfilled",
    (referenceKind, fieldName) => {
      const code: RationaleDto = {
        kind: "nearMissModReference",
        referenceKind,
        written: "exmaple.mod",
        candidateName: "Example Mod",
      };

      const text = rationaleText(code, t, label, "en");

      expect(text).toContain(`This ${fieldName} 'exmaple.mod'`);
      expect(text).toContain("'Example Mod'");
      expect(text).not.toMatch(/[{}]/);
    },
  );

  it("counts the owners of a duplicate template name, a runtime patch and a transpiler in their own sentence", () => {
    expect(
      rationaleText({ kind: "duplicateTemplateNameExplanation", ownerCount: 1 }, t, label, "en"),
    ).toContain("1 mod registers this template name");
    expect(
      rationaleText({ kind: "duplicateTemplateNameExplanation", ownerCount: 3 }, t, label, "en"),
    ).toContain("3 mods register this template name");
    expect(
      rationaleText(
        {
          kind: "runtimePatchCollisionLastPatcher",
          targetType: "T",
          targetMethod: "M",
          ownerCount: 2,
          last: "a.mod",
        },
        t,
        label,
        "en",
      ),
    ).toContain("2 mods patch T.M");
    expect(
      rationaleText(
        { kind: "transpilerCollision", targetType: "T", targetMethod: "M", ownerCount: 1 },
        t,
        label,
        "en",
      ),
    ).toContain("1 mod rewrites T.M");
  });

  it("composes the dangling-def-reference cause and sound caveat", () => {
    const withoutSound: RationaleDto = {
      kind: "danglingDefReference",
      cause: { kind: "definedNowhere" },
      likelySound: false,
    };
    const withoutSoundText = rationaleText(withoutSound, t, label, "en");
    expect(withoutSoundText).toContain("no installed mod, active or not, defines it");
    expect(withoutSoundText).not.toContain("sound reference");

    const withSound: RationaleDto = {
      kind: "danglingDefReference",
      cause: { kind: "onlyInInactiveMod", modId: "a.mod" },
      likelySound: true,
    };
    const withSoundText = rationaleText(withSound, t, label, "en");
    expect(withSoundText).toContain("Label(a.mod)");
    expect(withSoundText).toContain("sound reference");
  });

  it("names the declared winner and every other patcher of the field", () => {
    const code: RationaleDto = {
      kind: "patchCollisionWinnerDeclaresRelation",
      winner: "c.mod",
      others: ["a.mod", "b.mod"],
      field: "label",
    };
    const text = rationaleText(code, t, label, "en");
    expect(text).toContain("Label(c.mod)");
    expect(text).toContain("Label(a.mod)");
    expect(text).toContain("Label(b.mod)");
    expect(text).toContain("'label'");
  });

  it("resolves every mod id in a deliberate-discard rationale", () => {
    const code: RationaleDto = {
      kind: "discardedAdditionDeliberate",
      replacer: "a.mod",
      adder: "b.mod",
      path: "Defs/ThingDef/statBases",
      adderPath: "Defs/ThingDef/statBases/MoveSpeed",
    };
    const text = rationaleText(code, t, label, "en");
    expect(text).toContain("Label(a.mod)");
    expect(text).toContain("Label(b.mod)");
    expect(text).toContain("Defs/ThingDef/statBases/MoveSpeed");
  });
});
