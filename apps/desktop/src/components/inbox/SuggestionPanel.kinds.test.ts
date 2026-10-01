// SuggestionPanel: every finding kind renders non-empty evidence.

import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  buildConflictView,
  buildDetail,
  mountPanel,
} from "@/components/inbox/SuggestionPanel.test-support";
import { installMockIpc } from "@/services/ipc.mock";
import type { FindingDto } from "@/types/generated/FindingDto";
import type { FindingKindDto } from "@/types/generated/FindingKindDto";
import { assertNever } from "@/utils/assertNever";
import { ALL_FINDING_KINDS } from "@/utils/finding";

beforeEach(() => installMockIpc({ list_mod_names: {} }));
afterEach(() => clearMocks());

/**
 * `utils/finding.ts`'s own `findingKindLabel`/`primaryModIdOf` are
 * exhaustive via `assertNever` — which gives false comfort about *this*
 * template: a new kind compiles fine there while `SuggestionPanel.vue`'s
 * own `v-else-if` ladder silently renders nothing for it.
 * Vue's template compiler enforces no exhaustiveness over a `v-else-if`
 * chain at all — so this is a test-time backstop instead: every real
 * `FindingKindDto` (`ALL_FINDING_KINDS`, the same list the inbox's own
 * filter chips are built from) gets a minimal fixture and must render
 * *something* in the evidence section. `minimalFindingFor` below is
 * itself exhaustive via `assertNever`, so a future finding kind forces a
 * compile error here first — a second, earlier nudge to also update the
 * panel, not a replacement for the runtime assertion below actually
 * proving it was.
 */
function minimalFindingFor(kind: FindingKindDto): FindingDto {
  const edge = { after: "b.mod", before: "a.mod", layer: "declared" as const, detail: "d" };
  switch (kind) {
    case "edgeDropped":
      return {
        kind,
        after: "a.mod",
        before: "b.mod",
        edgeKind: "loadAfter",
        detail: "d",
        strength: "declared",
        winner: null,
      };
    case "declarationQuestioned":
      return {
        kind,
        declaredAfter: "a.mod",
        declaredBefore: "b.mod",
        declaredLayer: "declared",
        declaredDetail: "d",
        relationKind: "findMod",
        relationDetail: "d",
      };
    case "declarationOverridden":
      return {
        kind,
        declaredAfter: "a.mod",
        declaredBefore: "b.mod",
        edgeKind: "loadAfter",
        detail: "d",
        by: edge,
      };
    case "anyOfChoice":
      return { kind, after: "a.mod", assembly: "Shared.dll", candidates: ["c.mod"] };
    case "defOverride":
      return {
        kind,
        key: { defType: "ThingDef", defName: "Wall" },
        owners: ["a.mod"],
        winner: "a.mod",
      };
    case "patchCollision":
      return {
        kind,
        key: { defType: "ThingDef", defName: "Wall" },
        selector: "defName",
        subPath: null,
        mods: ["a.mod"],
      };
    case "textureOverride":
      return { kind, texturePath: "Things/Wall", owners: ["a.mod"], winner: "a.mod" };
    case "duplicateAssembly":
      return { kind, assemblyName: "Shared.dll", owners: ["a.mod"] };
    case "duplicateTemplateName":
      return { kind, name: "WallBase", owners: ["a.mod"] };
    case "keyedTranslationCollision":
      return { kind, a: "a.mod", b: "b.mod", keys: ["Greeting"] };
    case "soundOverride":
      return { kind, path: "shot_fire", owners: ["a.mod"] };
    case "undeclaredTypeDependency":
      return { kind, user: "a.mod", provider: "b.mod", typeName: "Framework.Utils" };
    case "runtimePatchCollision":
      return { kind, targetType: "Verse.Pawn", targetMethod: "Kill", owners: ["a.mod"] };
    case "transpilerCollision":
      return {
        kind,
        targetType: "Verse.Verb_LaunchProjectile",
        targetMethod: "TryCastShot",
        owners: ["a.mod"],
      };
    case "likelyDuplicateMod":
      return { kind, a: "a.mod", b: "b.mod", sharedDefs: 3 };
    case "missingMod":
      return { kind, modId: "a.mod" };
    case "missingDependency":
      return { kind, modId: "a.mod", dependency: "b.mod", displayName: null };
    case "incompatiblePair":
      return { kind, a: "a.mod", b: "b.mod" };
    case "unsupportedVersion":
      return { kind, modId: "a.mod" };
    case "undeclaredHardDependency":
      return { kind, after: "a.mod", before: "b.mod", detail: "d" };
    case "lazyReferenceViolated":
      return { kind, after: "a.mod", before: "b.mod", detail: "d" };
    case "tagInferred":
      return { kind, modId: "a.mod", tag: "t", matched: [{ kind: "dependsOn", modId: "m.mod" }] };
    case "ruleOverruled":
      return {
        kind,
        after: "a.mod",
        before: "b.mod",
        origin: "steamDb",
        comment: null,
        winner: edge,
        witnessCycle: [],
      };
    case "placementOverruled":
      return {
        kind,
        modId: "a.mod",
        placement: "bottom",
        origin: "rimSortCommunity",
        by: edge,
        landedAt: 0,
      };
    case "placementQuestioned":
      return {
        kind,
        modId: "a.mod",
        placement: "top",
        other: "b.mod",
        relationKind: "usesType",
        relationDetail: "d",
      };
    case "placementOrderingOverridden":
      return { kind, modId: "a.mod", pinned: "b.mod", placement: "bottom", by: edge };
    case "placementPromotesDependents":
      return { kind, modId: "a.mod", placement: "bottom", promoted: ["c.mod"] };
    case "missingTexturePath":
      return {
        kind,
        referrer: "a.mod",
        def: { defType: "ThingDef", defName: "Wall" },
        field: "texPath",
        path: "things/missing",
      };
    case "patchWillFail":
      return {
        kind,
        modId: "a.mod",
        defKey: { defType: "ThingDef", defName: "Wall" },
        selector: "defName",
        operation: 'Verse.PatchOperationReplace(Defs/ThingDef[defName="Wall"]/statBases)',
        leafXpath: 'Defs/ThingDef[defName="Wall"]/statBases',
        cause: { kind: "removedBy", modId: "b.mod" },
      };
    case "contributesNothing":
      return { kind, modId: "a.mod" };
    case "undecodableTexture":
      return {
        kind,
        modId: "a.mod",
        path: "things/wall",
        width: 65,
        height: 64,
        fourcc: "DXT5",
        hasPngSibling: false,
      };
    case "brokenInheritance":
      return {
        kind,
        modId: "a.mod",
        parentName: "NobodysBase",
        child: { defType: "ThingDef", name: "Leaf", isTemplate: false },
        problem: { kind: "missingParent" },
        affected: [{ defType: "ThingDef", defName: "Leaf" }],
        truncated: 0,
      };
    case "nearMissModReference":
      return {
        kind,
        referrer: "a.mod",
        referenceKind: "findModName",
        written: "Exmaple Mod",
        candidate: "b.mod",
        candidateName: "Example Mod",
        rule: "nearMiss",
        file: "About/Patches/Compat.xml",
      };
    case "discardedAddition":
      return {
        kind,
        replacer: "a.mod",
        adder: "b.mod",
        def: { defType: "ThingDef", defName: "Wall" },
        path: "ThingDef/Wall/comps",
        adderPath: "ThingDef/Wall/comps",
      };
    case "danglingDefReference":
      return {
        kind,
        name: "GhostDef",
        referrers: [
          {
            referrer: {
              kind: "referencedFromDef",
              modId: "a.mod",
              defType: "ThingDef",
              defName: "Wall",
            },
            fieldPath: "researchPrerequisites/li",
          },
        ],
        truncatedReferrers: 0,
        cause: { kind: "definedNowhere" },
        likelySound: false,
      };
    default:
      return assertNever(kind);
  }
}

describe("SuggestionPanel renders non-empty evidence for every finding kind", () => {
  it.each(ALL_FINDING_KINDS)("%s", async (kind) => {
    // `defOverride`/`patchCollision`/`duplicateTemplateName` render
    // through `DefConflictView`, which fetches `get_def_conflict_view`;
    // `textureOverride` fetches a texture per owner. Every other kind
    // renders synchronously from `detail.finding` alone.
    if (kind === "defOverride" || kind === "patchCollision" || kind === "duplicateTemplateName") {
      installMockIpc({
        list_mod_names: {},
        get_def_conflict_view: () =>
          buildConflictView({
            kind: kind === "patchCollision" ? { kind: "patchCollision", subPath: null } : { kind },
          }),
      });
    } else if (kind === "textureOverride") {
      installMockIpc({
        list_mod_names: {},
        read_texture: () => ({
          dataUrl: "data:image/png;base64,AA==",
          format: "png",
          bytes: 1,
          path: "Textures/Things/Wall.png",
        }),
      });
    }

    const wrapper = mountPanel(buildDetail(minimalFindingFor(kind)));
    await flushPromises();

    expect(wrapper.get('[data-testid="finding-payload"]').text().trim().length).toBeGreaterThan(0);
  });
});
