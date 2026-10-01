import { formatList } from "@/i18n/format";
import type { EdgeKindDto } from "@/types/generated/EdgeKindDto";
import type { LayerDto } from "@/types/generated/LayerDto";
import type { PlacementDto } from "@/types/generated/PlacementDto";
import type { RationaleDto } from "@/types/generated/RationaleDto";
import type { RuleOriginDto } from "@/types/generated/RuleOriginDto";
import { assertNever } from "@/utils/assertNever";
import { danglingCauseText } from "@/utils/danglingCause";

/**
 * The `t()` shape below takes — a caller's own `useI18n().t`. The
 * `count` argument is vue-i18n's own plural selector, needed for
 * {@link rationaleText}'s `placementPromotesDependents`/
 * `tagInferredSignalCount` arms.
 */
type Translate = (key: string, params?: Record<string, unknown>, count?: number) => string;

/** Mirrors `rim_resolve::domain::rationale::layer_phrase` (Rust). */
function layerPhrase(layer: LayerDto, t: Translate): string {
  switch (layer) {
    case "hard":
      return t("rationale.layerPhrase.hard");
    case "anyOf":
      return t("rationale.layerPhrase.anyOf");
    case "declaredOverride":
      return t("rationale.layerPhrase.declaredOverride");
    case "declared":
      return t("rationale.layerPhrase.declared");
    case "userDecision":
      return t("rationale.layerPhrase.userDecision");
    case "rimSortUser":
      return t("rationale.layerPhrase.rimSortUser");
    case "rimSortCommunity":
      return t("rationale.layerPhrase.rimSortCommunity");
    case "steamDb":
      return t("rationale.layerPhrase.steamDb");
    case "soft":
    case "awareness":
    case "inferred":
      return t("rationale.layerPhrase.other");
    default:
      return assertNever(layer);
  }
}

/** Mirrors `rim_resolve::domain::rationale::placement_phrase` (Rust). */
function placementPhrase(placement: PlacementDto, t: Translate): string {
  switch (placement) {
    case "top":
      return t("rationale.placementPhrase.top");
    case "bottom":
      return t("rationale.placementPhrase.bottom");
    default:
      return assertNever(placement);
  }
}

/**
 * Mirrors `rim_resolve::domain::rationale::relation_explanation` (Rust) —
 * the fixed, per-{@link EdgeKindDto} explanation for why an advisory
 * relation needs no order. Several kinds share one explanation, the same
 * grouping the Rust `match` uses.
 */
function relationExplanation(kind: EdgeKindDto, t: Translate): string {
  switch (kind) {
    case "findMod":
    case "mayRequire":
    case "ifModActive":
      return t("rationale.relationExplanation.presenceOnly");
    case "patchTargetsDef":
      return t("rationale.relationExplanation.patchTargetsDef");
    case "parentTemplate":
      return t("rationale.relationExplanation.parentTemplate");
    case "usesType":
      return t("rationale.relationExplanation.usesType");
    case "patchSelectsInjectedNode":
      return t("rationale.relationExplanation.patchSelectsInjectedNode");
    case "patchRemovedNode":
      return t("rationale.relationExplanation.patchRemovedNode");
    case "retextureAfterOwner":
      return t("rationale.relationExplanation.retextureAfterOwner");
    case "defOverrideAfterOrigin":
      return t("rationale.relationExplanation.defOverrideAfterOrigin");
    case "patchInvalidatesPredicate":
      return t("rationale.relationExplanation.patchInvalidatesPredicate");
    case "patchRemovedNodeCosmetic":
      return t("rationale.relationExplanation.patchRemovedNodeCosmetic");
    case "replaceDiscardsAddition":
      return t("rationale.relationExplanation.replaceDiscardsAddition");
    case "assemblyRef":
    case "forceLoadAfter":
    case "forceLoadBefore":
    case "loadAfter":
    case "loadBefore":
    case "modDependency":
    case "patchInjectedNode":
    case "assemblyVersionPrecedence":
      return t("rationale.relationExplanation.interactionEvidence");
    default:
      return assertNever(kind);
  }
}

/**
 * Mirrors `rim_resolve::domain::rationale::rule_phrase` (Rust) — the
 * phrase naming an overruled rule itself, shared by
 * `ruleOverruledLongerCycle`/`ruleOverruledByWinner`.
 */
function rulePhrase(
  after: string,
  before: string,
  origin: RuleOriginDto,
  overridesDeclared: boolean,
  t: Translate,
  label: (id: string) => string,
): string {
  if (origin === "userDecision") {
    return overridesDeclared
      ? t("rationale.rulePhrase.userDecisionOverride")
      : t("rationale.rulePhrase.userDecision");
  }
  const layer: LayerDto =
    origin === "rimSortUser"
      ? "rimSortUser"
      : origin === "rimSortCommunity"
        ? "rimSortCommunity"
        : "steamDb";
  return t("rationale.rulePhrase.layered", {
    after: label(after),
    before: label(before),
    layerPhrase: layerPhrase(layer, t),
  });
}

/**
 * A localized rendering of one {@link RationaleDto} — the ledger's own
 * structured "why" for a suggestion or alternative.
 * `SuggestionPanel.vue`'s own suggestion rationale and
 * `AlternativeList.vue`'s own per-alternative rationale both render
 * this instead of the raw `rationale`/`rationaleCode`'s English twin
 * (kept only for the pair-rule `--comment` prefill —
 * `useApplyDialog.ts`). Mirrors `rim_resolve::domain::Rationale`'s own
 * `Display` impl one variant at a time.
 *
 * `t`/`label`/`locale` are the caller's own `useI18n().t`/
 * `useModLabel().label`/`useI18n().locale.value` — this function can't
 * call either composable itself (a plain `utils/` helper, not a
 * component). `locale` joins the promoted-mod sample through
 * `Intl.ListFormat`, never a hard-coded separator.
 */
export function rationaleText(
  code: RationaleDto,
  t: Translate,
  label: (id: string) => string,
  locale: string,
): string {
  switch (code.kind) {
    case "edgeDroppedInferred":
      switch (code.edgeKind) {
        case "patchRemovedNode":
          return t("rationale.edgeDroppedInferred.patchRemovedNode", {
            after: label(code.after),
            before: label(code.before),
          });
        case "retextureAfterOwner":
          return t("rationale.edgeDroppedInferred.retextureAfterOwner", {
            after: label(code.after),
            before: label(code.before),
          });
        case "defOverrideAfterOrigin":
          return t("rationale.edgeDroppedInferred.defOverrideAfterOrigin", {
            after: label(code.after),
            before: label(code.before),
          });
        case "patchInvalidatesPredicate":
          return t("rationale.edgeDroppedInferred.patchInvalidatesPredicate", {
            after: label(code.after),
            before: label(code.before),
          });
        case "replaceDiscardsAddition":
          return t("rationale.edgeDroppedInferred.replaceDiscardsAddition", {
            after: label(code.after),
            before: label(code.before),
          });
        // `Rationale::EdgeDroppedInferred` is only ever constructed for
        // the five `Layer::Inferred` edge kinds above (see the Rust
        // `Display` impl's own `unreachable!`) — every other edge kind
        // here falls back to a plain, unlocalized rendering rather than
        // throwing, since a DTO field's own domain invariant isn't
        // something the frontend can prove exhaustively.
        default:
          return `${label(code.after)} / ${label(code.before)}`;
      }
    case "edgeDroppedCosmetic":
      return t("rationale.edgeDroppedCosmetic");
    case "keepEdgeInCycle":
      return t("rationale.keepEdgeInCycle");
    case "reorderOverridingEdgeWinner":
      return t("rationale.reorderOverridingEdgeWinner");
    case "edgeDroppedWithWinner":
      return t("rationale.edgeDroppedWithWinner", {
        after: label(code.after),
        before: label(code.before),
        layerPhrase: layerPhrase(code.winner.layer, t),
        detail: code.winner.detail,
      });
    case "edgeDroppedSoftOrAwareness":
      return t("rationale.edgeDroppedSoftOrAwareness");
    case "edgeDroppedDeclared":
      return t("rationale.edgeDroppedDeclared");
    case "edgeDroppedHard":
      return t("rationale.edgeDroppedHard");
    case "removeDependentModBrokenOrder":
      return t("rationale.removeDependentModBrokenOrder");
    case "declarationQuestioned":
      return t("rationale.declarationQuestioned", {
        declaredAfter: label(code.declaredAfter),
        declaredBefore: label(code.declaredBefore),
        layerPhrase: layerPhrase(code.declaredLayer, t),
        declaredDetail: code.declaredDetail,
        relationExplanation: relationExplanation(code.relationKind, t),
        relationDetail: code.relationDetail,
      });
    case "reorderOppositeOfRelation":
      return t("rationale.reorderOppositeOfRelation");
    case "declarationOverridden":
      return t("rationale.declarationOverridden", {
        declaredAfter: label(code.declaredAfter),
        declaredBefore: label(code.declaredBefore),
        detail: code.detail,
        layerPhrase: layerPhrase(code.by.layer, t),
        byAfter: label(code.by.after),
        byBefore: label(code.by.before),
        byDetail: code.by.detail,
      });
    case "anyOfNoCandidate":
      return t("rationale.anyOfNoCandidate");
    case "forceAnyOfCandidateAcceptingCycle":
      return t("rationale.forceAnyOfCandidateAcceptingCycle");
    case "anyOfChosenAlreadyBefore":
      return t("rationale.anyOfChosenAlreadyBefore");
    case "anyOfAlternativeCandidate":
      return t("rationale.anyOfAlternativeCandidate");
    case "anyOfUniqueMaxDependents":
      return t("rationale.anyOfUniqueMaxDependents");
    case "anyOfSmallestId":
      return t("rationale.anyOfSmallestId");
    case "ruleOverruledLongerCycle":
      return t("rationale.ruleOverruledLongerCycle", {
        rulePhrase: rulePhrase(
          code.after,
          code.before,
          code.origin,
          code.overridesDeclared,
          t,
          label,
        ),
      });
    case "ruleOverruledByWinner":
      return t("rationale.ruleOverruledByWinner", {
        rulePhrase: rulePhrase(
          code.after,
          code.before,
          code.origin,
          code.overridesDeclared,
          t,
          label,
        ),
        layerPhrase: layerPhrase(code.winner.layer, t),
        before: label(code.before),
        after: label(code.after),
        detail: code.winner.detail,
      });
    case "reorderOverridingRuleWinner":
      return t("rationale.reorderOverridingRuleWinner");
    case "promoteRuleDespiteOverruled":
      return t("rationale.promoteRuleDespiteOverruled");
    case "placementOverruled":
      return t("rationale.placementOverruled", {
        modId: label(code.modId),
        placementPhrase: placementPhrase(code.placement, t),
        layerPhrase: layerPhrase(code.by.layer, t),
        detail: code.by.detail,
      });
    case "promotePlacementDespiteOverruled":
      return t("rationale.promotePlacementDespiteOverruled", {
        placementPhrase: placementPhrase(code.placement, t),
      });
    case "dropRuleOverrulingPlacement":
      return t("rationale.dropRuleOverrulingPlacement", {
        modId: label(code.modId),
        placementPhrase: placementPhrase(code.placement, t),
      });
    case "placementQuestioned":
      return t("rationale.placementQuestioned", {
        modId: label(code.modId),
        placementPhrase: placementPhrase(code.placement, t),
        relationExplanation: relationExplanation(code.relationKind, t),
        relationDetail: code.relationDetail,
      });
    case "enforceRelationAsUserDecision":
      return t("rationale.enforceRelationAsUserDecision");
    case "placementOrderingOverridden": {
      const key =
        code.placement === "bottom"
          ? "rationale.placementOrderingOverridden.loadsAfter"
          : "rationale.placementOrderingOverridden.loadsBefore";
      return t(key, {
        pinned: label(code.pinned),
        placementPhrase: placementPhrase(code.placement, t),
        modId: label(code.modId),
        layerPhrase: layerPhrase(code.by.layer, t),
        detail: code.by.detail,
      });
    }
    case "placementPromotesDependents": {
      const names = code.promoted.map(label);
      const sample = names.slice(0, 3);
      const sampleText =
        sample.length === 0
          ? ""
          : names.length > sample.length
            ? t("rationale.promotedSample.truncated", { sample: formatList(locale, sample) })
            : t("rationale.promotedSample.exact", { sample: formatList(locale, sample) });
      return t(
        "rationale.placementPromotesDependents",
        {
          modId: label(code.modId),
          placementPhrase: placementPhrase(code.placement, t),
          count: code.promoted.length,
          sample: sampleText,
        },
        code.promoted.length,
      );
    }
    case "tagInferredSignalCount":
      return t("rationale.tagInferredSignalCount", { count: code.count }, code.count);
    case "rejectInferredTag":
      return t("rationale.rejectInferredTag");
    case "forceDefOverrideWinner":
      return t("rationale.forceDefOverrideWinner");
    case "defOverrideWinnerDeclaresRelation":
      return t("rationale.defOverrideWinnerDeclaresRelation");
    case "defOverrideSameAuthor":
      return t("rationale.defOverrideSameAuthor");
    case "defOverrideLoneNonVanillaOwner":
      return t("rationale.defOverrideLoneNonVanillaOwner");
    case "defOverrideShadowsFramework":
      return t("rationale.defOverrideShadowsFramework");
    case "keepCurrentWinner":
      return t("rationale.keepCurrentWinner");
    case "defOverrideUnexplained":
      return t("rationale.defOverrideUnexplained");
    case "mergeUnexplainedDefOverride":
      return t("rationale.mergeUnexplainedDefOverride");
    case "mergeShadowsFrameworkFieldByField":
      return t("rationale.mergeShadowsFrameworkFieldByField");
    case "forcePatchCollisionWinner":
      return t("rationale.forcePatchCollisionWinner");
    case "patchCollisionAdditive":
      return t("rationale.patchCollisionAdditive");
    case "patchCollisionContested":
      return t("rationale.patchCollisionContested");
    case "patchCollisionWinnerDeclaresRelation":
      return t("rationale.patchCollisionWinnerDeclaresRelation", {
        winner: label(code.winner),
        others: formatList(locale, code.others.map(label)),
        field: code.field,
      });
    case "mergeContestedPatchCollision":
      return t("rationale.mergeContestedPatchCollision");
    case "patchWillFailRemovedBy":
      return t("rationale.patchWillFailRemovedBy", {
        remover: label(code.remover),
        modId: label(code.modId),
        xpath: code.xpath,
      });
    case "reorderBeforeRemover":
      return t("rationale.reorderBeforeRemover", {
        modId: label(code.modId),
        remover: label(code.remover),
      });
    case "patchWillFailNotYetInjected":
      return t("rationale.patchWillFailNotYetInjected", {
        modId: label(code.modId),
        injector: label(code.injector),
        xpath: code.xpath,
      });
    case "reorderAfterInjector":
      return t("rationale.reorderAfterInjector", {
        modId: label(code.modId),
        injector: label(code.injector),
      });
    case "patchWillFailDeadTarget":
      return t("rationale.patchWillFailDeadTarget", {
        modId: label(code.modId),
        xpath: code.xpath,
      });
    case "patchWillFailUnknownCause":
      return t("rationale.patchWillFailUnknownCause", {
        modId: label(code.modId),
        xpath: code.xpath,
      });
    case "brokenInheritance":
      return t("rationale.brokenInheritance");
    case "danglingDefReference":
      return t("rationale.danglingDefReference", {
        cause: danglingCauseText(code.cause, t, label),
        soundCaveat: code.likelySound ? t("rationale.danglingDefReferenceSoundCaveat") : "",
      });
    case "discardedAdditionDeliberate":
      return t("rationale.discardedAdditionDeliberate", {
        replacer: label(code.replacer),
        adder: label(code.adder),
        path: code.path,
        adderPath: code.adderPath,
      });
    case "forceTextureWinner":
      return t("rationale.forceTextureWinner");
    case "copyTextureIntoMergeMod":
      return t("rationale.copyTextureIntoMergeMod");
    case "textureOverrideCosmetic":
      return t("rationale.textureOverrideCosmetic");
    case "forceTemplateRegistrationWinner":
      return t("rationale.forceTemplateRegistrationWinner");
    case "duplicateTemplateNameExplanation":
      return t(
        "rationale.duplicateTemplateNameExplanation",
        { ownerCount: code.ownerCount },
        code.ownerCount,
      );
    case "keyedTranslationCollision":
      return t("rationale.keyedTranslationCollision", { keyCount: code.keyCount }, code.keyCount);
    case "soundOverrideCosmetic":
      return t("rationale.soundOverrideCosmetic");
    case "forceSoundWinner":
      return t("rationale.forceSoundWinner");
    case "missingTexturePath":
      return t("rationale.missingTexturePath");
    case "undecodableTexture":
      return t("rationale.undecodableTexture");
    case "contributesNothing":
      return t("rationale.contributesNothing", { modId: label(code.modId) });
    case "undeclaredTypeDependency":
      return t("rationale.undeclaredTypeDependency", {
        user: label(code.user),
        provider: label(code.provider),
      });
    case "pinRelationExplicitly":
      return t("rationale.pinRelationExplicitly");
    case "runtimePatchCollisionNoLastPatcher":
      return t("rationale.runtimePatchCollisionNoLastPatcher", {
        targetType: code.targetType,
        targetMethod: code.targetMethod,
      });
    case "runtimePatchCollisionLastPatcher":
      return t(
        "rationale.runtimePatchCollisionLastPatcher",
        {
          targetType: code.targetType,
          targetMethod: code.targetMethod,
          ownerCount: code.ownerCount,
          last: label(code.last),
        },
        code.ownerCount,
      );
    case "forceRuntimePatchLastWinner":
      return t("rationale.forceRuntimePatchLastWinner");
    case "transpilerCollision":
      return t(
        "rationale.transpilerCollision",
        {
          targetType: code.targetType,
          targetMethod: code.targetMethod,
          ownerCount: code.ownerCount,
        },
        code.ownerCount,
      );
    case "duplicateAssemblyFirstLoadedWins":
      return t("rationale.duplicateAssemblyFirstLoadedWins");
    case "removeDuplicateAssemblyCopy":
      return t("rationale.removeDuplicateAssemblyCopy");
    case "likelyDuplicateMod":
      return t("rationale.likelyDuplicateMod");
    case "removeLikelyDuplicateModA":
      return t("rationale.removeLikelyDuplicateModA");
    case "removeLikelyDuplicateModB":
      return t("rationale.removeLikelyDuplicateModB");
    case "missingModNotInstalled":
      return t("rationale.missingModNotInstalled");
    case "keepMissingModId":
      return t("rationale.keepMissingModId");
    case "missingDependencyNotEnforced":
      return t("rationale.missingDependencyNotEnforced");
    case "incompatiblePair":
      return t("rationale.incompatiblePair");
    case "removeIncompatibleModA":
      return t("rationale.removeIncompatibleModA");
    case "removeIncompatibleModB":
      return t("rationale.removeIncompatibleModB");
    case "unsupportedVersionOftenWorks":
      return t("rationale.unsupportedVersionOftenWorks");
    case "undeclaredHardDependencyHonored":
      return t("rationale.undeclaredHardDependencyHonored");
    case "pinRelationExplicitlyAnyway":
      return t("rationale.pinRelationExplicitlyAnyway");
    case "lazyReferenceViolatedJitResolved":
      return t("rationale.lazyReferenceViolatedJitResolved");
    case "nearMissModReference":
      return t(
        code.referenceKind === "findModName"
          ? "rationale.nearMissModReference.findModName"
          : "rationale.nearMissModReference.mayRequireId",
        { written: code.written, candidateName: code.candidateName },
      );
    case "mergeCompleteNothingToMerge":
      return t("rationale.mergeCompleteNothingToMerge");
    case "mergeLeadDefOverrideCombine":
      return t("rationale.mergeLeadDefOverrideCombine");
    case "mergePromotedPatchCollisionKeepsBoth":
      return t("rationale.mergePromotedPatchCollisionKeepsBoth");
    case "keepLoadOrderWinner":
      return t("rationale.keepLoadOrderWinner");
    case "mergeStructuralGuard":
      return t("rationale.mergeStructuralGuard", { field: code.field });
    case "mergeFieldsNeedChoice":
      return t("rationale.mergeFieldsNeedChoice", { unresolved: code.unresolved }, code.unresolved);
    case "identicalCopiesOrderIrrelevant":
      return t("rationale.identicalCopiesOrderIrrelevant");
    case "notAddressedByPatch":
      return t("rationale.notAddressedByPatch");
    default:
      return assertNever(code);
  }
}
