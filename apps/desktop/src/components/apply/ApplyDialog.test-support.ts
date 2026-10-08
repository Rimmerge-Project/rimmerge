// Fixtures and the mount helper shared by the ApplyDialog test files.

import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { createMemoryHistory, createRouter } from "vue-router";
import ApplyDialog from "@/components/apply/ApplyDialog.vue";
import { useSessionStore } from "@/stores/session";
import type { ApplyPreflightDto } from "@/types/generated/ApplyPreflightDto";
import type { DashboardDto } from "@/types/generated/DashboardDto";
import type { MergeModDto } from "@/types/generated/MergeModDto";
import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";
import type { PendingActiveChangesDto } from "@/types/generated/PendingActiveChangesDto";
import type { PreflightItemDto } from "@/types/generated/PreflightItemDto";
import type { RuleSetDto } from "@/types/generated/RuleSetDto";

export const NEEDS_INPUT_BY_KIND: DashboardDto["needsInputByKind"] = {
  edgeDropped: 0,
  anyOfChoice: 0,
  defOverride: 0,
  patchCollision: 0,
  textureOverride: 0,
  duplicateAssembly: 0,
  likelyDuplicateMod: 0,
  missingMod: 0,
  missingDependency: 0,
  incompatiblePair: 0,
  unsupportedVersion: 0,
  undeclaredHardDependency: 0,
  lazyReferenceViolated: 0,
  declarationQuestioned: 0,
  declarationOverridden: 0,
  duplicateTemplateName: 0,
  keyedTranslationCollision: 0,
  soundOverride: 0,
  undeclaredTypeDependency: 0,
  runtimePatchCollision: 0,
  transpilerCollision: 0,
  tagInferred: 0,
  ruleOverruled: 0,
  placementOverruled: 0,
  placementQuestioned: 0,
  placementOrderingOverridden: 0,
  placementPromotesDependents: 0,
  missingTexturePath: 0,
  patchWillFail: 0,
  contributesNothing: 0,
  undecodableTexture: 0,
  brokenInheritance: 0,
  nearMissModReference: 0,
  discardedAddition: 0,
  danglingDefReference: 0,
};

export function dashboardFixture(needsInput: number): DashboardDto {
  return {
    modCount: 10,
    edgesByStrength: { hard: 1, declared: 1, soft: 1, inferred: 1, awareness: 1 },
    edgesViolatedBySource: {
      assemblyRef: 0,
      forceLoadAfter: 0,
      forceLoadBefore: 0,
      loadAfter: 0,
      loadBefore: 0,
      modDependency: 0,
      findMod: 0,
      ifModActive: 0,
      patchTargetsDef: 0,
      mayRequire: 0,
      patchInjectedNode: 0,
      assemblyVersionPrecedence: 0,
      usesType: 0,
      parentTemplate: 0,
      patchRemovedNode: 0,
      retextureAfterOwner: 0,
      defOverrideAfterOrigin: 0,
      patchSelectsInjectedNode: 0,
      patchInvalidatesPredicate: 0,
      patchRemovedNodeCosmetic: 0,
      replaceDiscardsAddition: 0,
    },
    conflictsByKind: {
      defOverride: 0,
      patchCollision: 0,
      textureOverride: 0,
      duplicateAssembly: 0,
      likelyDuplicateMod: 0,
      duplicateTemplateName: 0,
      keyedTranslationCollision: 0,
      soundOverride: 0,
      runtimePatchCollision: 0,
      transpilerCollision: 0,
      missingTexturePath: 0,
      undecodableTexture: 0,
      brokenInheritance: 0,
      nearMissModReference: 0,
      discardedAddition: 0,
      danglingDefReference: 0,
    },
    ledgerStats: {
      current: { auto: 5, needsInput, overridden: 0, resolvedBySuggested: 0 },
      suggested: { auto: 5, needsInput: 0, overridden: 0, resolvedBySuggested: 0 },
    },
    needsInputByKind: NEEDS_INPUT_BY_KIND,
    movedMods: 4,
    selected: "current",
    fileMatchesSuggested: false,
    fileMatchesCurrent: true,
    sortProvenance: { tieBreak: "rebuild", useImportedPairs: false, useImportedPlacements: true },
  };
}

/** `get_apply_preflight`'s result: no problems unless the test passes some. */
export function preflightFixture(
  items: PreflightItemDto[] = [],
  source: OrderSourceDto = "current",
): ApplyPreflightDto {
  return {
    source,
    items,
    requiresConfirmation: items.some((item) => !item.acknowledged),
  };
}

/** One unanswered missing-mod problem, for the confirmation panel's tests. */
export const UNANSWERED_MISSING_MOD: PreflightItemDto = {
  acknowledged: false,
  problem: { kind: "missingMod", modId: "gone.mod", outcome: "removedFromActiveList" },
};

/** One already-decided incompatible pair. */
export const DECIDED_INCOMPATIBLE_PAIR: PreflightItemDto = {
  acknowledged: true,
  problem: { kind: "incompatiblePair", a: "first.mod", b: "second.mod" },
};

/**
 * The dialog reads the user's own pair
 * rules before writing one, so every mock needs `list_rules`. Empty by
 * default — the one test that cares about a pre-existing rule overrides
 * it.
 */
export const EMPTY_RULE_SET: RuleSetDto = {
  pairs: [],
  placements: [],
  incompatibles: [],
  warnings: [],
};

/**
 * The dialog reads this to gate the write checkbox
 * and show the unapplied added/removed list — every mock needs it too.
 * Empty by default; the dedicated stale/unapplied tests override it.
 */
export const EMPTY_PENDING_ACTIVE_CHANGES: PendingActiveChangesDto = {
  unscanned: { added: [], removed: [] },
  unapplied: { added: [], removed: [] },
};

export async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

/** An empty merge mod — no entries, nothing renders. The default for tests that don't care about the merge checkbox. */
export function emptyMergeModFixture(): MergeModDto {
  return {
    packageId: "rimmerge.merge.abc123def456",
    folderName: "rimmerge_merge_abc123def456",
    modsPath: "C:/RimWorld/Mods/rimmerge_merge_abc123def456",
    exists: false,
    entries: [],
    files: [],
    sourceMods: [],
  };
}

export function mergeModFixtureWithEntry(
  state: MergeModDto["entries"][number]["state"],
  overrides: Partial<MergeModDto["entries"][number]> = {},
): MergeModDto {
  return {
    ...emptyMergeModFixture(),
    entries: [
      {
        key: "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]",
        defKey: { defType: "HediffDef", defName: "BionicHeart" },
        kind: "defOverride",
        state,
        opCount: state.kind === "complete" ? state.opCount : 0,
        dependsOn: ["example.bionicsfork"],
        patchFile: "Patches/rimmerge_HediffDef.xml",
        structuralGuardField: null,
        ...overrides,
      },
    ],
    files: ["About/About.xml", "Patches/rimmerge_HediffDef.xml", "rimmerge.json"],
  };
}

export function mountDialog(
  options: { attachToBody?: boolean; errorHandler?: (error: unknown) => void } = {},
) {
  const pinia = createPinia();
  setActivePinia(pinia);
  const session = useSessionStore();
  session.setLoaded(
    {
      gameDir: "C:/RimWorld",
      workshopDir: "C:/RimWorld/workshop",
      modsConfig: "C:/RimWorld/ModsConfig.xml",
      profileDir: "C:/Profile/rimmerge",
    },
    "current",
  );

  // The dialog links to the Findings page and a merge editor; the routes only have to resolve.
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: { render: () => null } },
      { path: "/inbox", component: { render: () => null } },
      { path: "/merge/:key", name: "merge-editor", component: { render: () => null } },
    ],
  });

  const wrapper = mount(ApplyDialog, {
    props: { visible: true },
    ...(options.attachToBody ? { attachTo: document.body } : {}),
    global: {
      plugins: [pinia, router, PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      stubs: { teleport: true },
      ...(options.errorHandler ? { config: { errorHandler: options.errorHandler } } : {}),
    },
  });
  return { wrapper, session, router, pinia };
}
