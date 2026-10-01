// SuggestionPanel: the evidence each finding kind's payload renders.

import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  buildConflictView,
  buildDetail,
  mountPanel,
} from "@/components/inbox/SuggestionPanel.test-support";
import { installMockIpc } from "@/services/ipc.mock";

// `useModLabel` (via `SuggestionPanel`/`AlternativeList`) always queries
// `list_mod_names` — registered once here (not per test) since every
// test in this file wants the same thing: every id left unresolved, so
// the raw id text asserted on below is `label()`'s own fallback. An
// *unregistered* command's rejection reaches Vitest as an unhandled
// rejection that fails the whole run, not just this file — see the
// identical note in `InboxPage.test.ts`.
beforeEach(() => installMockIpc({ list_mod_names: {} }));
afterEach(() => clearMocks());

describe("SuggestionPanel finding payloads", () => {
  it("def override lists owners in load order", async () => {
    installMockIpc({
      list_mod_names: {},
      get_def_conflict_view: () =>
        buildConflictView({
          touchers: [
            { modId: "a.mod", position: 0, isGenerated: false, role: "owner", opCount: 0 },
            { modId: "b.mod", position: 1, isGenerated: false, role: "owner", opCount: 0 },
            { modId: "c.mod", position: 2, isGenerated: false, role: "owner", opCount: 0 },
          ],
        }),
    });
    const wrapper = mountPanel(
      buildDetail({
        kind: "defOverride",
        key: { defType: "ThingDef", defName: "Wall" },
        owners: ["a.mod", "b.mod", "c.mod"],
        winner: "c.mod",
      }),
    );
    await flushPromises();

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("ThingDef/Wall");
    expect(text).toContain("a.mod");
    expect(text).toContain("c.mod");
    expect(text).toContain("Def override");
  });

  it("patch collision shows the def, sub-path, and every mod's field", async () => {
    installMockIpc({
      list_mod_names: {},
      get_def_conflict_view: () =>
        buildConflictView({
          kind: { kind: "patchCollision", subPath: "statBases/MaxHitPoints" },
          touchers: [
            { modId: "a.mod", position: 0, isGenerated: false, role: "owner", opCount: 0 },
            { modId: "b.mod", position: 1, isGenerated: false, role: "patcher", opCount: 1 },
          ],
          fields: [
            {
              path: "statBases/MaxHitPoints",
              kind: "cleanMerge",
              values: [{ modId: "b.mod", value: "150" }],
              agreedBy: [],
              inGame: { modId: "b.mod", value: "150" },
              afterMerge: { modId: "b.mod", value: "150" },
              preference: { kind: "none" },
            },
          ],
          fieldsTotal: 1,
        }),
    });
    const wrapper = mountPanel(
      buildDetail({
        kind: "patchCollision",
        key: { defType: "ThingDef", defName: "Wall" },
        selector: "defName",
        subPath: "statBases/MaxHitPoints",
        mods: ["a.mod", "b.mod"],
      }),
    );
    await flushPromises();

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("ThingDef/Wall");
    expect(text).toContain("statBases/MaxHitPoints");
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
  });

  it("any-of choice lists every candidate", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "anyOfChoice",
        after: "addon.mod",
        assembly: "Shared.dll",
        candidates: ["framework.a", "framework.b"],
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("addon.mod");
    expect(text).toContain("framework.a");
    expect(text).toContain("framework.b");
  });

  it("declaration questioned names both edges", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "declarationQuestioned",
        declaredAfter: "addon.mod",
        declaredBefore: "framework.mod",
        declaredLayer: "declared",
        declaredDetail: "declared to load after framework.mod",
        relationKind: "findMod",
        relationDetail: "looks up framework.mod by FindMod",
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("addon.mod");
    expect(text).toContain("framework.mod");
    expect(text).toContain("looks up framework.mod by FindMod");
  });

  it("declaration overridden names the declared edge and the overriding rule", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "declarationOverridden",
        declaredAfter: "addon.mod",
        declaredBefore: "framework.mod",
        edgeKind: "loadAfter",
        detail: "declares loadAfter framework.mod",
        by: {
          after: "framework.mod",
          before: "addon.mod",
          layer: "declaredOverride",
          detail: "your own declared-edge override",
        },
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("addon.mod");
    expect(text).toContain("framework.mod");
    expect(text).toContain("your own declared-edge override");
  });

  it("duplicate template name lists every registering owner", async () => {
    installMockIpc({
      list_mod_names: {},
      get_def_conflict_view: () =>
        buildConflictView({
          defRef: "WallBase",
          kind: { kind: "duplicateTemplateName" },
          touchers: [
            { modId: "a.mod", position: 0, isGenerated: false, role: "owner", opCount: 0 },
            { modId: "b.mod", position: 1, isGenerated: false, role: "owner", opCount: 0 },
          ],
        }),
    });
    const wrapper = mountPanel(
      buildDetail({
        kind: "duplicateTemplateName",
        name: "WallBase",
        owners: ["a.mod", "b.mod"],
      }),
    );
    await flushPromises();

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("WallBase");
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
  });

  it("a patch-scoped def override (patchId set) still renders DefConflictView, with patchId threaded into the request", async () => {
    // `get_def_conflict_view` shares `get_merge_preview`'s `patchId`
    // scoping, so `SuggestionPanel` needs no `ChangeSummary` patch-scoped
    // fallback — `DefConflictView` handles both contexts.
    const patchIds: (string | null)[] = [];
    installMockIpc({
      list_mod_names: {},
      get_def_conflict_view: (payload: unknown) => {
        patchIds.push((payload as { request: { patchId: string | null } }).request.patchId);
        return buildConflictView();
      },
    });
    const wrapper = mountPanel(
      buildDetail({
        kind: "defOverride",
        key: { defType: "ThingDef", defName: "Wall" },
        owners: ["a.mod"],
        winner: "a.mod",
      }),
      { patchId: "patch-1" },
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="def-conflict-view"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="change-summary"]').exists()).toBe(false);
    expect(patchIds).toEqual(["patch-1"]);
  });

  it("keyed translation collision names the pair and every shared key", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "keyedTranslationCollision",
        a: "a.mod",
        b: "b.mod",
        keys: ["Greeting", "Farewell"],
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
    expect(text).toContain("Greeting");
    expect(text).toContain("Farewell");
  });

  it("sound override lists every shipping owner", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "soundOverride",
        path: "shot_fire",
        owners: ["a.mod", "b.mod"],
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("shot_fire");
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
  });

  it("undeclared type dependency names the user, provider, and type", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "undeclaredTypeDependency",
        user: "addon.mod",
        provider: "framework.mod",
        typeName: "Framework.Utils",
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("addon.mod");
    expect(text).toContain("framework.mod");
    expect(text).toContain("Framework.Utils");
  });

  it("transpiler collision names the target and every transpiling owner, with no alternatives", () => {
    const wrapper = mountPanel(
      buildDetail(
        {
          kind: "transpilerCollision",
          targetType: "Verse.Verb_LaunchProjectile",
          targetMethod: "TryCastShot",
          owners: ["a.mod", "b.mod"],
        },
        {
          suggestion: {
            action: { kind: "accept" },
            confidence: 55,
            rationale: "2 mods each rewrite this method's IL with a runtime-patch transpiler.",
            rationaleCode: {
              kind: "transpilerCollision",
              targetType: "Verse.Verb_LaunchProjectile",
              targetMethod: "TryCastShot",
              ownerCount: 2,
            },
            alternatives: [],
          },
        },
      ),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
    expect(text).toContain("Verse.Verb_LaunchProjectile::TryCastShot");
    // No `PreferWinner`-style action button — the direction genuinely
    // isn't known, unlike `runtimePatchCollision`'s "Run last" buttons.
    expect(wrapper.find('[data-testid^="runtime-patch-run-last-"]').exists()).toBe(false);
  });

  it("runtime patch collision names the target and every patching owner", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "runtimePatchCollision",
        targetType: "Verse.Pawn",
        targetMethod: "Kill",
        owners: ["a.mod", "b.mod"],
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
    expect(text).toContain("Verse.Pawn::Kill");
  });

  it("runtime patch collision marks the last owner as the current last patcher and offers 'Run last' on the others", () => {
    const wrapper = mountPanel(
      buildDetail(
        {
          kind: "runtimePatchCollision",
          targetType: "Verse.Pawn",
          targetMethod: "Kill",
          owners: ["a.mod", "b.mod", "c.mod"],
        },
        {
          suggestion: {
            action: { kind: "accept" },
            confidence: 85,
            rationale: "3 mods patch Verse.Pawn.Kill; c.mod loads last today.",
            rationaleCode: {
              kind: "runtimePatchCollisionLastPatcher",
              targetType: "Verse.Pawn",
              targetMethod: "Kill",
              ownerCount: 3,
              last: "c.mod",
            },
            alternatives: ["a.mod", "b.mod", "c.mod"].map((winner) => ({
              action: {
                kind: "preferWinner",
                key: { defType: "runtime_target", defName: "Verse.Pawn::Kill" },
                winner,
              },
              rationale: "Force this mod's patch to run last, regardless of load order.",
              rationaleCode: { kind: "forceRuntimePatchLastWinner" as const },
            })),
          },
        },
      ),
    );

    expect(wrapper.get('[data-testid="runtime-patch-last-patcher"]').text()).toBe("runs last");
    // Exactly one marker: `a.mod`/`b.mod` get a button instead.
    expect(wrapper.findAll('[data-testid="runtime-patch-last-patcher"]')).toHaveLength(1);
    expect(wrapper.find('[data-testid="runtime-patch-run-last-c.mod"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="runtime-patch-run-last-a.mod"]').text()).toBe("Run last");
    expect(wrapper.get('[data-testid="runtime-patch-run-last-b.mod"]').text()).toBe("Run last");
    // Per-row aria-labels: several identically-labeled "Run last" buttons
    // on one page are indistinguishable to a screen reader without one.
    expect(
      wrapper.get('[data-testid="runtime-patch-run-last-a.mod"]').attributes("aria-label"),
    ).toBe("Run a.mod last");
    expect(
      wrapper.get('[data-testid="runtime-patch-run-last-b.mod"]').attributes("aria-label"),
    ).toBe("Run b.mod last");
  });

  it("clicking a runtime-patch owner's 'Run last' button emits that owner's own pickAlternative index", async () => {
    const wrapper = mountPanel(
      buildDetail(
        {
          kind: "runtimePatchCollision",
          targetType: "Verse.Pawn",
          targetMethod: "Kill",
          owners: ["a.mod", "b.mod", "c.mod"],
        },
        {
          suggestion: {
            action: { kind: "accept" },
            confidence: 85,
            rationale: "3 mods patch Verse.Pawn.Kill; c.mod loads last today.",
            rationaleCode: {
              kind: "runtimePatchCollisionLastPatcher",
              targetType: "Verse.Pawn",
              targetMethod: "Kill",
              ownerCount: 3,
              last: "c.mod",
            },
            alternatives: ["a.mod", "b.mod", "c.mod"].map((winner) => ({
              action: {
                kind: "preferWinner",
                key: { defType: "runtime_target", defName: "Verse.Pawn::Kill" },
                winner,
              },
              rationale: "Force this mod's patch to run last, regardless of load order.",
              rationaleCode: { kind: "forceRuntimePatchLastWinner" as const },
            })),
          },
        },
      ),
    );

    await wrapper.get('[data-testid="runtime-patch-run-last-b.mod"]').trigger("click");
    // `b.mod` is the second owner, so its `PreferWinner` alternative is
    // index 2 (1-based, matching `AlternativeList`'s own convention).
    expect(wrapper.emitted("pickAlternative")).toEqual([[2]]);
  });

  it("rule overruled names the rule, its origin, and the winner's layer and detail", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "ruleOverruled",
        after: "a.mod",
        before: "b.mod",
        origin: "steamDb",
        comment: null,
        winner: {
          after: "b.mod",
          before: "a.mod",
          layer: "declared",
          detail: "declares a modDependency",
        },
        witnessCycle: [],
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
    expect(text).toContain("SteamDB");
    expect(text).toContain("Declared");
    expect(text).toContain("declares a modDependency");
  });

  it("rule overruled with no direct winner renders the witness cycle instead", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "ruleOverruled",
        after: "a.mod",
        before: "b.mod",
        origin: "steamDb",
        comment: null,
        winner: null,
        witnessCycle: ["a.mod", "c.mod", "b.mod"],
      }),
    );

    expect(wrapper.get('[data-testid="rule-overruled-witness-cycle"]').text()).toBe(
      "cycle: a.mod → c.mod → b.mod",
    );
  });

  it("placement overruled names the placement, origin, winner, and landed-at position", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "placementOverruled",
        modId: "a.mod",
        placement: "bottom",
        origin: "rimSortCommunity",
        by: {
          after: "b.mod",
          before: "a.mod",
          layer: "hard",
          detail: "ships a load-time AssemblyRef",
        },
        landedAt: 7,
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("a.mod");
    expect(text).toContain("Bottom");
    expect(text).toContain("RimSort community");
    expect(text).toContain("ships a load-time AssemblyRef");
    expect(text).toContain("Landed at #8");
  });

  it("placement questioned names the placement and the advisory relation", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "placementQuestioned",
        modId: "a.mod",
        placement: "top",
        other: "b.mod",
        relationKind: "usesType",
        relationDetail: "b.mod uses a type a.mod defines",
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("a.mod");
    expect(text).toContain("Top");
    expect(text).toContain("Uses type");
    expect(text).toContain("b.mod uses a type a.mod defines");
  });

  // This and `placementPromotesDependents` below each render their own
  // evidence here, not only the suggestion rationale.
  it("placement ordering overridden names the forced mod, the pin, and the forcing edge", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "placementOrderingOverridden",
        modId: "a.mod",
        pinned: "b.mod",
        placement: "bottom",
        by: {
          after: "a.mod",
          before: "c.mod",
          layer: "declared",
          detail: "declares a modDependency on c.mod",
        },
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
    expect(text).toContain("Bottom");
    expect(text).toContain("declares a modDependency on c.mod");
  });

  it("placement promotes dependents lists every promoted mod, up to 20, with a 'Show more' beyond that", async () => {
    const leaf = mountPanel(
      buildDetail({
        kind: "placementPromotesDependents",
        modId: "leaf.mod",
        placement: "bottom",
        promoted: [],
      }),
    );
    // The common case (many real pins promote nothing) — no list, no
    // button, just the "0 mods" sentence.
    expect(leaf.get('[data-testid="finding-payload"]').text()).toContain("promoting 0 other mods");
    expect(leaf.find('[data-testid="placement-promotes-dependents-show-more"]').exists()).toBe(
      false,
    );

    // A framework-shaped pin's own real shape is dozens of promoted mods
    // (114 for `example.framework` on the real install — this fixture uses
    // its own count, not that number, since a concurrent crate change is
    // re-baselining it): capped at 20 until "Show more" is clicked.
    const promoted = Array.from({ length: 25 }, (_, index) => `dep.${index}.mod`);
    const framework = mountPanel(
      buildDetail({
        kind: "placementPromotesDependents",
        modId: "framework.mod",
        placement: "bottom",
        promoted,
      }),
    );
    const items = () => framework.findAll('[data-testid="placement-promotes-dependents-list"] li');
    expect(items()).toHaveLength(20);
    expect(framework.get('[data-testid="placement-promotes-dependents-show-more"]').text()).toBe(
      "Show more (20 of 25)",
    );

    await framework.get('[data-testid="placement-promotes-dependents-show-more"]').trigger("click");
    expect(items()).toHaveLength(25);
    expect(framework.find('[data-testid="placement-promotes-dependents-show-more"]').exists()).toBe(
      false,
    );
  });

  it("lazy reference violated shows the edge", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "lazyReferenceViolated",
        after: "a.mod",
        before: "b.mod",
        detail: "AssemblyRef Foo.dll",
      }),
    );

    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
    expect(text).toContain("AssemblyRef Foo.dll");
  });

  it("incompatible pair names both mods", () => {
    const wrapper = mountPanel(buildDetail({ kind: "incompatiblePair", a: "a.mod", b: "b.mod" }));
    expect(wrapper.get('[data-testid="finding-payload"]').text()).toContain("a.mod and b.mod");
  });

  it("duplicate assembly lists the shipping mods", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "duplicateAssembly",
        assemblyName: "Shared.dll",
        owners: ["a.mod", "b.mod"],
      }),
    );
    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("Shared.dll");
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
  });

  it("likely duplicate mod reports the shared def count", () => {
    const wrapper = mountPanel(
      buildDetail({ kind: "likelyDuplicateMod", a: "a.mod", b: "b.mod", sharedDefs: 12 }),
    );
    expect(wrapper.get('[data-testid="finding-payload"]').text()).toContain(
      "12 identical def keys",
    );
  });

  it("tag inferred shows every matched signal", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "tagInferred",
        modId: "addon.mod",
        tag: "framework",
        matched: [
          { kind: "urlContains", needle: "framework" },
          { kind: "assemblyRefTo", modId: "example.mod" },
        ],
      }),
    );
    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("addon.mod");
    expect(text).toContain("framework");
    expect(text).toContain('url contains "framework"');
    expect(text).toContain("assembly ref to example.mod");
  });

  it("missing dependency prefers the author's display name over the raw id", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "missingDependency",
        modId: "addon.mod",
        dependency: "framework.mod",
        displayName: "Example Framework",
      }),
    );
    expect(wrapper.get('[data-testid="finding-payload"]').text()).toContain("Example Framework");
  });

  it("unsupported version and undeclared hard dependency render their own text", () => {
    const version = mountPanel(buildDetail({ kind: "unsupportedVersion", modId: "old.mod" }));
    expect(version.get('[data-testid="finding-payload"]').text()).toContain("old.mod");

    const undeclared = mountPanel(
      buildDetail({
        kind: "undeclaredHardDependency",
        after: "a.mod",
        before: "b.mod",
        detail: "AssemblyRef Bar.dll",
      }),
    );
    const text = undeclared.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
  });

  it("texture override and edge dropped render their own evidence", async () => {
    // `TexturePair`/`TextureTile` call `read_texture` per owner on mount —
    // an unregistered fixture rejects loudly (see the `list_mod_names`
    // note above), so this needs one even though the assertion below
    // only cares about the texture path text, not the images themselves.
    installMockIpc({
      list_mod_names: {},
      read_texture: () => ({
        dataUrl: "data:image/png;base64,AA==",
        format: "png",
        bytes: 1,
        path: "Textures/Things/Wall.png",
      }),
    });
    const texture = mountPanel(
      buildDetail({
        kind: "textureOverride",
        texturePath: "Things/Wall",
        owners: ["a.mod", "b.mod"],
        winner: "b.mod",
      }),
    );
    await flushPromises();
    expect(texture.get('[data-testid="finding-payload"]').text()).toContain("Things/Wall");

    const dropped = mountPanel(
      buildDetail({
        kind: "edgeDropped",
        after: "a.mod",
        before: "b.mod",
        edgeKind: "loadAfter",
        detail: "loadAfter",
        strength: "declared",
        winner: null,
      }),
    );
    const text = dropped.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("a.mod");
    expect(text).toContain("b.mod");
    expect(text).toContain("Declared");
  });

  it("missing mod names the missing id", () => {
    const wrapper = mountPanel(buildDetail({ kind: "missingMod", modId: "gone.mod" }));
    expect(wrapper.get('[data-testid="finding-payload"]').text()).toContain("gone.mod");
  });
});
