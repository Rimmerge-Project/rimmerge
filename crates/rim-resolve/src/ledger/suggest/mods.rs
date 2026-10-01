//! Suggestions for whole-mod findings: duplicates, missing mods and dependencies, runtime-patch/transpiler collisions, contributes-nothing.

use super::{SuggestContext, confidence};
use crate::domain::{Action, Alternative, DefKey, Rationale};
use rim_analyzer::domain::{ModId, ModReferenceKind, NearMissRule};

/// Every observable contribution `mod_id` makes is
/// inert under the order this was checked against. `Accept` only — there
/// is no alternative action to offer: disabling the mod is the user's
/// own act in their mod manager, not something Rimmerge does.
/// Confidence is deliberately under the ordinary 80 `Auto`
/// threshold: a false positive here costs the user a mod, so this always
/// lands in the inbox as needs-input rather than being auto-accepted.
pub(super) fn contributes_nothing(mod_id: &ModId) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(70),
        rationale: Rationale::ContributesNothing {
            mod_id: mod_id.clone(),
        },
        alternatives: Vec::new(),
    }
}

/// A def in `user` names a type from `provider`'s DLL with no
/// declared relation onto `provider` — the def breaks if `provider` is
/// ever absent.
pub(super) fn undeclared_type_dependency(
    user: &ModId,
    provider: &ModId,
) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(70),
        rationale: Rationale::UndeclaredTypeDependency {
            user: user.clone(),
            provider: provider.clone(),
        },
        alternatives: vec![Alternative {
            action: Action::Reorder {
                after: user.clone(),
                before: provider.clone(),
            },
            rationale: Rationale::PinRelationExplicitly,
        }],
    }
}

/// Two or more mods patch the same `(type, method)` at equal runtime-patch
/// priority — which prefix/postfix runs last depends on load order, and
/// neither the patching library nor this analyzer can say which is
/// "right". One finding per target (see
/// `FindingKey::RuntimePatchCollision`'s own doc comment
/// for why): `Accept` at 85, naming the current order's last patcher, with
/// a `PreferWinner` alternative per owner (via
/// `DefKey::synthesize_for_runtime_target`, recognized back by
/// `decision::owners_of`) rather than a two-`Reorder` shape, which only
/// makes sense for exactly two owners.
pub(super) fn runtime_patch_collision(
    target_type: &str,
    target_method: &str,
    owners: &[ModId],
    ctx: &SuggestContext<'_>,
) -> crate::domain::Suggestion {
    use crate::domain::Suggestion;

    // The owner with the latest position in the current order is the one
    // whose patch actually runs last today; falls back to the first owner
    // when none has a current position (shouldn't happen for active mods,
    // but this stays total rather than panicking).
    let last_patcher = owners
        .iter()
        .max_by_key(|owner| ctx.current.position(owner))
        .or_else(|| owners.first());

    let rationale = last_patcher.map_or_else(
        || Rationale::RuntimePatchCollisionNoLastPatcher {
            target_type: target_type.to_string(),
            target_method: target_method.to_string(),
        },
        |last| Rationale::RuntimePatchCollisionLastPatcher {
            target_type: target_type.to_string(),
            target_method: target_method.to_string(),
            owner_count: owners.len(),
            last: last.clone(),
        },
    );

    Suggestion {
        action: Action::Accept,
        confidence: confidence(85),
        rationale,
        alternatives: owners
            .iter()
            .map(|owner| Alternative {
                action: Action::PreferWinner {
                    key: DefKey::synthesize_for_runtime_target(target_type, target_method),
                    winner: owner.clone(),
                },
                rationale: Rationale::ForceRuntimePatchLastWinner,
            })
            .collect(),
    }
}

/// Two or more mods each rewrite the same method's IL via a runtime-patch
/// transpiler — a warning about fragility, not a known defect.
/// **Disclosure only, deliberately with no alternative**: neither this
/// analyzer nor the sorter can
/// say which transpiler must run first (that depends on whose IL pattern
/// survives whose rewrite, not on anything in the metadata), so offering
/// a one-click `Reorder`/`PreferWinner` here would assert an order this
/// crate has no basis for — the same mistake a directionless edge
/// resolved by the current-order tie-break would make silently.
/// Confidence is deliberately below
/// the sibling `runtime_patch_collision`'s 85: most instances of this
/// finding are presumably fine (prefixes/postfixes compose safely no
/// matter how many mods add one; only a transpiler-vs-transpiler pattern
/// match can actually break), so this reads as "worth a look" rather
/// than "here's how it resolves". A user who does see a patch failure
/// naming one of these mods can state the working order directly via
/// `rimmerge rule set-pair`.
pub(super) fn transpiler_collision(
    target_type: &str,
    target_method: &str,
    owners: &[ModId],
) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(55),
        rationale: Rationale::TranspilerCollision {
            target_type: target_type.to_string(),
            target_method: target_method.to_string(),
            owner_count: owners.len(),
        },
        alternatives: Vec::new(),
    }
}

pub(super) fn duplicate_assembly(owners: &[ModId]) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(30),
        rationale: Rationale::DuplicateAssemblyFirstLoadedWins,
        alternatives: owners
            .iter()
            .map(|m| Alternative {
                action: Action::RemoveMod { mod_id: m.clone() },
                rationale: Rationale::RemoveDuplicateAssemblyCopy,
            })
            .collect(),
    }
}

pub(super) fn likely_duplicate_mod(a: &ModId, b: &ModId) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(20),
        rationale: Rationale::LikelyDuplicateMod,
        alternatives: vec![
            Alternative {
                action: Action::RemoveMod { mod_id: a.clone() },
                rationale: Rationale::RemoveLikelyDuplicateModA,
            },
            Alternative {
                action: Action::RemoveMod { mod_id: b.clone() },
                rationale: Rationale::RemoveLikelyDuplicateModB,
            },
        ],
    }
}

pub(super) fn missing_mod(mod_id: &ModId) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::RemoveMod {
            mod_id: mod_id.clone(),
        },
        confidence: confidence(85),
        rationale: Rationale::MissingModNotInstalled,
        alternatives: vec![Alternative {
            action: Action::Ignore,
            rationale: Rationale::KeepMissingModId,
        }],
    }
}

pub(super) fn missing_dependency() -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Ignore,
        confidence: confidence(30),
        rationale: Rationale::MissingDependencyNotEnforced,
        alternatives: Vec::new(),
    }
}

pub(super) fn incompatible_pair(a: &ModId, b: &ModId) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(20),
        rationale: Rationale::IncompatiblePair,
        alternatives: vec![
            Alternative {
                action: Action::RemoveMod { mod_id: a.clone() },
                rationale: Rationale::RemoveIncompatibleModA,
            },
            Alternative {
                action: Action::RemoveMod { mod_id: b.clone() },
                rationale: Rationale::RemoveIncompatibleModB,
            },
        ],
    }
}

pub(super) fn unsupported_version() -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Ignore,
        confidence: confidence(85),
        rationale: Rationale::UnsupportedVersionOftenWorks,
        alternatives: Vec::new(),
    }
}

pub(super) fn undeclared_hard_dependency(
    after: &ModId,
    before: &ModId,
) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(95),
        rationale: Rationale::UndeclaredHardDependencyHonored,
        alternatives: vec![Alternative {
            action: Action::Reorder {
                after: after.clone(),
                before: before.clone(),
            },
            rationale: Rationale::PinRelationExplicitly,
        }],
    }
}

/// A lazily-resolved `AssemblyRef` (`EdgeStrength::Soft`) that the order in
/// effect doesn't honor. Accepted at high confidence by default: these
/// resolve at JIT time, after every mod's assemblies are already loaded, so
/// RimWorld itself is unaffected by which of the two mods loads first — see
/// `EnforcedLayers::default`. Surfaced (rather than silently dropped, the
/// way an advisory edge otherwise would be) so a genuinely order-sensitive
/// lazy reference is never hidden from "show all", and `Reorder` is still
/// offered for the rare case where it matters.
pub(super) fn lazy_reference_violated(after: &ModId, before: &ModId) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(90),
        rationale: Rationale::LazyReferenceViolatedJitResolved,
        alternatives: vec![Alternative {
            action: Action::Reorder {
                after: after.clone(),
                before: before.clone(),
            },
            rationale: Rationale::PinRelationExplicitlyAnyway,
        }],
    }
}

/// See [`crate::domain::Finding::NearMissModReference`]. Informational
/// only — the confidence table's own "possible typo" tier: worded as a
/// possibility, never a fact (unlike [`crate::domain::Finding::BrokenInheritance`],
/// which *is* a proven load-time resolution), and always below the
/// default `Auto` threshold (80) so it never silently auto-accepts. No
/// alternative: the fix is the referencing mod's own authoring, outside
/// this workspace — "report upstream" is the whole action.
pub(super) fn near_miss_mod_reference(
    kind: ModReferenceKind,
    rule: NearMissRule,
    written: &str,
    candidate_name: &str,
) -> crate::domain::Suggestion {
    let confidence_percent = match rule {
        NearMissRule::CaseOnly => 70,
        NearMissRule::Normalized => 60,
        NearMissRule::NearMiss | NearMissRule::LeadingToken => 45,
    };
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(confidence_percent),
        rationale: Rationale::NearMissModReference {
            kind,
            written: written.to_string(),
            candidate_name: candidate_name.to_string(),
        },
        alternatives: Vec::new(),
    }
}
