import { describe, expect, it } from "vitest";
import { createI18n } from "vue-i18n";
import en from "@/locales/en.json";
import type { EdgeWinnerDto } from "@/types/generated/EdgeWinnerDto";
import type { FindingDto } from "@/types/generated/FindingDto";
import { findingTitle } from "@/utils/findingTitle";

const { t } = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: false,
  messages: { en },
}).global;

/** Renders `findingTitle`'s own descriptor back to plain text, the way `useTranslateMessage()` would. */
function renderTitle(finding: FindingDto): string {
  const desc = findingTitle(finding, t, (id) => id);
  return desc.count === undefined
    ? t(desc.key, desc.params ?? {})
    : t(desc.key, desc.params ?? {}, desc.count);
}

function winner(after: string, before: string): EdgeWinnerDto {
  return { after, before, layer: "declared", detail: "declares a modDependency" };
}

describe("findingTitle", () => {
  it("edgeDropped without a winner", () => {
    const finding: FindingDto = {
      kind: "edgeDropped",
      after: "a.mod",
      before: "b.mod",
      edgeKind: "loadAfter",
      detail: "loadAfter",
      strength: "declared",
      winner: null,
    };
    expect(renderTitle(finding)).toBe("b.mod before a.mod dropped to break a cycle");
  });

  it("edgeDropped with a winner", () => {
    const finding: FindingDto = {
      kind: "edgeDropped",
      after: "a.mod",
      before: "b.mod",
      edgeKind: "loadAfter",
      detail: "loadAfter",
      strength: "declared",
      winner: winner("b.mod", "a.mod"),
    };
    expect(renderTitle(finding)).toBe("b.mod before a.mod dropped, overruled by b.mod");
  });

  it("declarationQuestioned names the relation's translated label", () => {
    const finding: FindingDto = {
      kind: "declarationQuestioned",
      declaredAfter: "a.mod",
      declaredBefore: "b.mod",
      declaredLayer: "declared",
      declaredDetail: "declares a modDependency",
      relationKind: "usesType",
      relationDetail: "b.mod uses a type a.mod defines",
    };
    expect(renderTitle(finding)).toBe("a.mod after b.mod questioned by a Uses type relation");
  });

  it("declarationOverridden", () => {
    const finding: FindingDto = {
      kind: "declarationOverridden",
      declaredAfter: "a.mod",
      declaredBefore: "b.mod",
      edgeKind: "loadAfter",
      detail: "loadAfter",
      by: winner("b.mod", "a.mod"),
    };
    expect(renderTitle(finding)).toBe("a.mod after b.mod overridden by b.mod");
  });

  it("anyOfChoice", () => {
    const finding: FindingDto = {
      kind: "anyOfChoice",
      after: "a.mod",
      assembly: "Shared.dll",
      candidates: ["b.mod", "c.mod"],
    };
    expect(renderTitle(finding)).toBe("a.mod needs one of 2 mods providing Shared.dll");
  });

  it("defOverride names the def key", () => {
    const finding: FindingDto = {
      kind: "defOverride",
      key: { defType: "ThingDef", defName: "Wall" },
      owners: ["a.mod", "b.mod"],
      winner: "b.mod",
    };
    expect(renderTitle(finding)).toBe("ThingDef/Wall");
  });

  it("patchCollision names the def key", () => {
    const finding: FindingDto = {
      kind: "patchCollision",
      key: { defType: "ThingDef", defName: "Wall" },
      selector: "defName",
      subPath: null,
      mods: ["a.mod", "b.mod"],
    };
    expect(renderTitle(finding)).toBe("ThingDef/Wall");
  });

  it("textureOverride prefixes the path", () => {
    const finding: FindingDto = {
      kind: "textureOverride",
      texturePath: "Things/Wall.png",
      owners: ["a.mod"],
      winner: "a.mod",
    };
    expect(renderTitle(finding)).toBe("texture Things/Wall.png");
  });

  it("duplicateAssembly pluralizes correctly", () => {
    const oneOwner: FindingDto = {
      kind: "duplicateAssembly",
      assemblyName: "0ExampleLib.dll",
      owners: ["a.mod"],
    };
    expect(renderTitle(oneOwner)).toBe("0ExampleLib.dll shipped by 1 mod");

    const twoOwners: FindingDto = {
      kind: "duplicateAssembly",
      assemblyName: "0ExampleLib.dll",
      owners: ["a.mod", "b.mod"],
    };
    expect(renderTitle(twoOwners)).toBe("0ExampleLib.dll shipped by 2 mods");
  });

  it("duplicateTemplateName pluralizes correctly", () => {
    const oneOwner: FindingDto = {
      kind: "duplicateTemplateName",
      name: "Base",
      owners: ["a.mod"],
    };
    expect(renderTitle(oneOwner)).toBe("Template Base registered by 1 mod");

    const twoOwners: FindingDto = {
      kind: "duplicateTemplateName",
      name: "Base",
      owners: ["a.mod", "b.mod"],
    };
    expect(renderTitle(twoOwners)).toBe("Template Base registered by 2 mods");
  });

  it("keyedTranslationCollision pluralizes correctly", () => {
    const oneKey: FindingDto = {
      kind: "keyedTranslationCollision",
      a: "a.mod",
      b: "b.mod",
      keys: ["Greeting"],
    };
    expect(renderTitle(oneKey)).toBe("a.mod and b.mod share 1 translation key");

    const manyKeys: FindingDto = {
      kind: "keyedTranslationCollision",
      a: "a.mod",
      b: "b.mod",
      keys: ["Greeting", "Farewell"],
    };
    expect(renderTitle(manyKeys)).toBe("a.mod and b.mod share 2 translation keys");
  });

  it("soundOverride pluralizes correctly", () => {
    const oneOwner: FindingDto = {
      kind: "soundOverride",
      path: "Sounds/Boom.ogg",
      owners: ["a.mod"],
    };
    expect(renderTitle(oneOwner)).toBe("Sound Sounds/Boom.ogg shipped by 1 mod");

    const twoOwners: FindingDto = {
      kind: "soundOverride",
      path: "Sounds/Boom.ogg",
      owners: ["a.mod", "b.mod"],
    };
    expect(renderTitle(twoOwners)).toBe("Sound Sounds/Boom.ogg shipped by 2 mods");
  });

  it("undeclaredTypeDependency", () => {
    const finding: FindingDto = {
      kind: "undeclaredTypeDependency",
      user: "a.mod",
      provider: "b.mod",
      typeName: "B.Thing",
    };
    expect(renderTitle(finding)).toBe("a.mod uses b.mod's B.Thing with no declared relation");
  });

  it("runtimePatchCollision pluralizes correctly", () => {
    const oneOwner: FindingDto = {
      kind: "runtimePatchCollision",
      targetType: "Verse.Pawn",
      targetMethod: "Kill",
      owners: ["a.mod"],
    };
    expect(renderTitle(oneOwner)).toBe("Verse.Pawn.Kill patched by 1 mod");

    const threeOwners: FindingDto = {
      kind: "runtimePatchCollision",
      targetType: "Verse.Pawn",
      targetMethod: "Kill",
      owners: ["a.mod", "b.mod", "c.mod"],
    };
    expect(renderTitle(threeOwners)).toBe("Verse.Pawn.Kill patched by 3 mods");
  });

  it("transpilerCollision pluralizes correctly", () => {
    const oneOwner: FindingDto = {
      kind: "transpilerCollision",
      targetType: "Verse.Verb_LaunchProjectile",
      targetMethod: "TryCastShot",
      owners: ["a.mod"],
    };
    expect(renderTitle(oneOwner)).toBe(
      "Verse.Verb_LaunchProjectile.TryCastShot transpiled by 1 mod",
    );

    const threeOwners: FindingDto = {
      kind: "transpilerCollision",
      targetType: "Verse.Verb_LaunchProjectile",
      targetMethod: "TryCastShot",
      owners: ["a.mod", "b.mod", "c.mod"],
    };
    expect(renderTitle(threeOwners)).toBe(
      "Verse.Verb_LaunchProjectile.TryCastShot transpiled by 3 mods",
    );
  });

  it("likelyDuplicateMod", () => {
    const finding: FindingDto = {
      kind: "likelyDuplicateMod",
      a: "a.mod",
      b: "b.mod",
      sharedDefs: 12,
    };
    expect(renderTitle(finding)).toBe("a.mod and b.mod look like duplicates (12 shared defs)");
  });

  it("missingMod", () => {
    const finding: FindingDto = { kind: "missingMod", modId: "a.mod" };
    expect(renderTitle(finding)).toBe("a.mod is missing");
  });

  it("missingDependency prefers the display name", () => {
    const withDisplayName: FindingDto = {
      kind: "missingDependency",
      modId: "a.mod",
      dependency: "b.mod",
      displayName: "B Mod",
    };
    expect(renderTitle(withDisplayName)).toBe("a.mod is missing its dependency B Mod");

    const withoutDisplayName: FindingDto = {
      kind: "missingDependency",
      modId: "a.mod",
      dependency: "b.mod",
      displayName: null,
    };
    expect(renderTitle(withoutDisplayName)).toBe("a.mod is missing its dependency b.mod");
  });

  it("incompatiblePair", () => {
    const finding: FindingDto = { kind: "incompatiblePair", a: "a.mod", b: "b.mod" };
    expect(renderTitle(finding)).toBe("a.mod and b.mod are marked incompatible");
  });

  it("unsupportedVersion", () => {
    const finding: FindingDto = { kind: "unsupportedVersion", modId: "a.mod" };
    expect(renderTitle(finding)).toBe("a.mod doesn't support this RimWorld version");
  });

  it("undeclaredHardDependency", () => {
    const finding: FindingDto = {
      kind: "undeclaredHardDependency",
      after: "a.mod",
      before: "b.mod",
      detail: "load-time AssemblyRef",
    };
    expect(renderTitle(finding)).toBe("a.mod needs b.mod at load time with no declaration");
  });

  it("lazyReferenceViolated", () => {
    const finding: FindingDto = {
      kind: "lazyReferenceViolated",
      after: "a.mod",
      before: "b.mod",
      detail: "lazy AssemblyRef",
    };
    expect(renderTitle(finding)).toBe("a.mod references b.mod before it loads");
  });

  it("tagInferred", () => {
    const finding: FindingDto = {
      kind: "tagInferred",
      modId: "a.mod",
      tag: "framework",
      matched: [],
    };
    expect(renderTitle(finding)).toBe("a.mod tagged framework");
  });

  it("ruleOverruled without a winner", () => {
    const finding: FindingDto = {
      kind: "ruleOverruled",
      after: "a.mod",
      before: "b.mod",
      origin: "steamDb",
      comment: null,
      winner: null,
      witnessCycle: ["a.mod", "c.mod", "b.mod"],
    };
    expect(renderTitle(finding)).toBe("Rule b.mod before a.mod overruled");
  });

  it("ruleOverruled with a winner", () => {
    const finding: FindingDto = {
      kind: "ruleOverruled",
      after: "a.mod",
      before: "b.mod",
      origin: "steamDb",
      comment: null,
      winner: winner("b.mod", "a.mod"),
      witnessCycle: [],
    };
    expect(renderTitle(finding)).toBe("Rule b.mod before a.mod overruled by b.mod");
  });

  it("placementOverruled", () => {
    const finding: FindingDto = {
      kind: "placementOverruled",
      modId: "a.mod",
      placement: "bottom",
      origin: "rimSortCommunity",
      by: winner("c.mod", "a.mod"),
      landedAt: 7,
    };
    expect(renderTitle(finding)).toBe("Bottom placement of a.mod overruled");
  });

  it("placementQuestioned", () => {
    const finding: FindingDto = {
      kind: "placementQuestioned",
      modId: "a.mod",
      placement: "top",
      other: "b.mod",
      relationKind: "usesType",
      relationDetail: "b.mod uses a type a.mod defines",
    };
    expect(renderTitle(finding)).toBe("Top placement of a.mod questioned by b.mod");
  });

  it("placementOrderingOverridden", () => {
    const finding: FindingDto = {
      kind: "placementOrderingOverridden",
      modId: "a.mod",
      pinned: "b.mod",
      placement: "bottom",
      by: winner("a.mod", "b.mod"),
    };
    expect(renderTitle(finding)).toBe("a.mod overrides b.mod's own placement ordering");
  });

  it("placementPromotesDependents pluralizes correctly", () => {
    const oneMod: FindingDto = {
      kind: "placementPromotesDependents",
      modId: "a.mod",
      placement: "top",
      promoted: ["b.mod"],
    };
    expect(renderTitle(oneMod)).toBe("a.mod's placement promotes 1 other mod");

    const twoMods: FindingDto = {
      kind: "placementPromotesDependents",
      modId: "a.mod",
      placement: "top",
      promoted: ["b.mod", "c.mod"],
    };
    expect(renderTitle(twoMods)).toBe("a.mod's placement promotes 2 other mods");
  });

  it("missingTexturePath", () => {
    const finding: FindingDto = {
      kind: "missingTexturePath",
      referrer: "a.mod",
      def: { defType: "ThingDef", defName: "Wall" },
      field: "texPath",
      path: "Things/Wall",
    };
    expect(renderTitle(finding)).toBe(
      "ThingDef/Wall field texPath names a texture no active mod ships",
    );
  });

  it("patchWillFail", () => {
    const finding: FindingDto = {
      kind: "patchWillFail",
      modId: "a.mod",
      defKey: { defType: "ThingDef", defName: "Wall" },
      selector: "defName",
      operation: 'Verse.PatchOperationReplace(Defs/ThingDef[defName="Wall"]/statBases)',
      leafXpath: null,
      cause: { kind: "deadTarget" },
    };
    expect(renderTitle(finding)).toBe("a.mod's operation on ThingDef/Wall is predicted to fail");
  });

  it("contributesNothing", () => {
    const finding: FindingDto = { kind: "contributesNothing", modId: "a.mod" };
    expect(renderTitle(finding)).toBe("a.mod contributes nothing");
  });

  it("undecodableTexture", () => {
    const finding: FindingDto = {
      kind: "undecodableTexture",
      modId: "a.mod",
      path: "Things/Wall.dds",
      width: 3,
      height: 5,
      fourcc: "DXT1",
      hasPngSibling: false,
    };
    expect(renderTitle(finding)).toBe("a.mod's Things/Wall.dds won't decode");
  });

  it("brokenInheritance", () => {
    const finding: FindingDto = {
      kind: "brokenInheritance",
      modId: "a.mod",
      parentName: "WallBase",
      child: { defType: "ThingDef", name: "Wall", isTemplate: false },
      problem: { kind: "missingParent" },
      affected: [],
      truncated: 0,
    };
    expect(renderTitle(finding)).toBe("a.mod's inheritance from 'WallBase' is broken");
  });

  it("nearMissModReference", () => {
    const finding: FindingDto = {
      kind: "nearMissModReference",
      referrer: "a.mod",
      referenceKind: "findModName",
      written: "Exmaple Framework",
      candidate: "b.mod",
      candidateName: "Example Framework",
      rule: "nearMiss",
      file: null,
    };
    expect(renderTitle(finding)).toBe("a.mod's reference to 'Exmaple Framework' may be a typo");
  });

  it("discardedAddition", () => {
    const finding: FindingDto = {
      kind: "discardedAddition",
      replacer: "a.mod",
      adder: "b.mod",
      def: { defType: "ThingDef", defName: "Wall" },
      path: "researchPrerequisites",
      adderPath: "researchPrerequisites/li",
    };
    expect(renderTitle(finding)).toBe("a.mod deliberately discards b.mod's own addition");
  });

  it("danglingDefReference", () => {
    const finding: FindingDto = {
      kind: "danglingDefReference",
      name: "MissingDef",
      referrers: [],
      truncatedReferrers: 0,
      cause: { kind: "definedNowhere" },
      likelySound: false,
    };
    expect(renderTitle(finding)).toBe("'MissingDef' resolves to no active def");
  });

  it("resolves every mod id through the caller's own label function", () => {
    const finding: FindingDto = { kind: "missingMod", modId: "a.mod" };
    const desc = findingTitle(finding, t, (id) => `Label(${id})`);
    expect(t(desc.key, desc.params ?? {})).toBe("Label(a.mod) is missing");
  });
});
