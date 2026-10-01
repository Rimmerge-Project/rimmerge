//! Suggestions for def-level findings: def overrides, patch collisions, and patches that will fail.

use std::collections::BTreeMap;

use super::{SuggestContext, confidence};
use crate::domain::{Action, Alternative, DefKey, PatchFailureCause, Rationale};
use rim_analyzer::domain::{
    Conflict, Mod, ModId, PatchCollision, PatchCollisionSeverity, Report, Selector,
};

fn find_def_override<'a>(
    report: &'a Report,
    key: &DefKey,
) -> Option<&'a rim_analyzer::domain::DefOverride> {
    report.conflicts.iter().find_map(|c| match c {
        Conflict::DefOverride(d) if d.def_type == key.def_type && d.def_name == key.def_name => {
            Some(d)
        }
        _ => None,
    })
}

/// The `Merge` alternative offered wherever a def override or patch
/// collision isn't auto-resolved outright — never itself the suggested
/// action here (`Merge` is never auto-suggested by the confidence table,
/// because carrying it out writes a mod folder).
fn merge_alternative(key: &DefKey, rationale: Rationale) -> Alternative {
    Alternative {
        action: Action::Merge {
            key: key.clone(),
            choices: BTreeMap::new(),
        },
        rationale,
    }
}

/// `prefer_winner_alts` plus a trailing `Merge` alternative — the
/// alternatives offered for a def override the ledger can't explain with
/// any stronger signal (confidence 60, both where the analyzer found no
/// `Conflict::DefOverride` entry at all and the ordinary end-of-function
/// fallback).
fn unexplained_def_override_alternatives(
    key: &DefKey,
    prefer_winner_alts: &[Alternative],
) -> Vec<Alternative> {
    let mut alternatives = prefer_winner_alts.to_vec();
    alternatives.push(merge_alternative(
        key,
        Rationale::MergeUnexplainedDefOverride,
    ));
    alternatives
}

/// Which signal, if any, explains a `DefOverride` finding's winner —
/// exactly the branches `def_override`'s confidence table checks, factored
/// out as a pure classifier (rather than left as private control flow
/// inside that function alone) so `rim-session`'s identical-copies pass
/// (`Session::redecide_identical_copies_at`) can gate its own, more
/// expensive per-owner content check on the identical evidence, without
/// duplicating the branching or reaching for the confidence-table's own
/// `Suggestion` output shape — which can't be relied on for this: `SameAuthor`
/// and `LoneNonVanillaOwner` both currently render as `Accept` 90 with only
/// `PreferWinner` alternatives, indistinguishable from the outside except by
/// rationale text.
///
/// [`Self::Unknown`] covers both of `def_override`'s own "unexplained"
/// cases (no [`rim_analyzer::domain::Conflict::DefOverride`] entry at all,
/// or one that matched none of the flags below) — they carry the identical
/// suggestion shape and the same identical-copies eligibility, so nothing
/// downstream needs to tell them apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefOverrideDirection {
    /// The winner explicitly declares a load-order relation to every other
    /// owner.
    WinnerDeclaresRelation,
    /// Every owner shares an author.
    SameAuthor,
    /// Exactly one active, non-vanilla owner overrides a vanilla def.
    LoneNonVanillaOwner,
    /// A leaf mod is quietly shadowing a shared framework's def.
    ShadowsFramework {
        /// The framework owner a `PreferWinner` suggestion would name.
        framework_owner: ModId,
    },
    /// No signal explains the winner — the analyzer found no
    /// `Conflict::DefOverride` entry, or found one but none of its flags
    /// applied.
    Unknown,
}

/// Classifies a `DefOverride` finding's evidence into exactly the branch
/// [`def_override`] itself uses to pick a suggestion — see
/// [`DefOverrideDirection`]'s own doc comment for why this is a separate,
/// public function rather than inline control flow.
#[must_use]
pub fn def_override_direction(
    report: &Report,
    key: &DefKey,
    owners: &[ModId],
    winner: &ModId,
    mods_by_id: &BTreeMap<ModId, &Mod>,
) -> DefOverrideDirection {
    let Some(conflict) = find_def_override(report, key) else {
        return DefOverrideDirection::Unknown;
    };

    if conflict.winner_declares_relation(winner, mods_by_id) {
        return DefOverrideDirection::WinnerDeclaresRelation;
    }
    if conflict.same_author {
        return DefOverrideDirection::SameAuthor;
    }
    if conflict.overrides_vanilla {
        let non_vanilla_owners = owners
            .iter()
            .filter(|owner| {
                mods_by_id
                    .get(*owner)
                    .is_none_or(|m| !m.source.is_vanilla())
            })
            .count();
        // Exactly one non-vanilla owner means there's no real contest: that
        // one mod is the only thing that could have overridden vanilla, so
        // the override is unambiguous. Two or more non-vanilla owners is a
        // genuine multi-mod contest and falls through to `Unknown` below
        // instead.
        if non_vanilla_owners == 1 {
            return DefOverrideDirection::LoneNonVanillaOwner;
        }
    }
    if conflict.shadows_framework(winner, mods_by_id) {
        let framework_owner = owners
            .iter()
            .find(|owner| {
                *owner != winner
                    && mods_by_id
                        .get(*owner)
                        .is_some_and(|m| m.is_framework_candidate)
            })
            .unwrap_or(winner)
            .clone();
        return DefOverrideDirection::ShadowsFramework { framework_owner };
    }

    DefOverrideDirection::Unknown
}

pub(super) fn def_override(
    key: &DefKey,
    owners: &[ModId],
    winner: &ModId,
    ctx: &SuggestContext<'_>,
) -> crate::domain::Suggestion {
    use crate::domain::Suggestion;

    let prefer_winner_alts: Vec<Alternative> = owners
        .iter()
        .map(|owner| Alternative {
            action: Action::PreferWinner {
                key: key.clone(),
                winner: owner.clone(),
            },
            rationale: Rationale::ForceDefOverrideWinner,
        })
        .collect();

    match def_override_direction(ctx.report, key, owners, winner, ctx.mods_by_id) {
        DefOverrideDirection::WinnerDeclaresRelation => Suggestion {
            action: Action::Accept,
            confidence: confidence(95),
            rationale: Rationale::DefOverrideWinnerDeclaresRelation,
            alternatives: prefer_winner_alts,
        },
        DefOverrideDirection::SameAuthor => Suggestion {
            action: Action::Accept,
            confidence: confidence(90),
            rationale: Rationale::DefOverrideSameAuthor,
            alternatives: prefer_winner_alts,
        },
        DefOverrideDirection::LoneNonVanillaOwner => Suggestion {
            action: Action::Accept,
            confidence: confidence(90),
            rationale: Rationale::DefOverrideLoneNonVanillaOwner,
            alternatives: prefer_winner_alts,
        },
        DefOverrideDirection::ShadowsFramework { framework_owner } => {
            let alternatives = vec![
                Alternative {
                    action: Action::Accept,
                    rationale: Rationale::KeepCurrentWinner,
                },
                merge_alternative(key, Rationale::MergeShadowsFrameworkFieldByField),
            ];
            Suggestion {
                action: Action::PreferWinner {
                    key: key.clone(),
                    winner: framework_owner,
                },
                confidence: confidence(40),
                rationale: Rationale::DefOverrideShadowsFramework,
                alternatives,
            }
        }
        DefOverrideDirection::Unknown => Suggestion {
            action: Action::Accept,
            confidence: confidence(60),
            rationale: Rationale::DefOverrideUnexplained,
            alternatives: unexplained_def_override_alternatives(key, &prefer_winner_alts),
        },
    }
}

/// Every distinct contributor to `collision` other than `winner`, in the
/// order the scan saw them (`collision.mods` is already sorted by it).
fn other_patchers(collision: &PatchCollision, winner: &ModId) -> Vec<ModId> {
    let mut others: Vec<ModId> = Vec::new();
    for entry in &collision.mods {
        if entry.mod_id != *winner && !others.contains(&entry.mod_id) {
            others.push(entry.mod_id.clone());
        }
    }
    others
}

/// A [`crate::domain::Finding::PatchCollision`]'s own fields, borrowed.
pub(super) struct CollisionTarget<'a> {
    pub(super) key: &'a DefKey,
    pub(super) selector: Selector,
    pub(super) sub_path: Option<&'a str>,
    /// Every contributing mod.
    pub(super) mods: &'a [ModId],
    /// The contributor that runs last under the ledger's own order.
    pub(super) winner: &'a ModId,
}

pub(super) fn patch_collision(
    target: &CollisionTarget<'_>,
    ctx: &SuggestContext<'_>,
) -> crate::domain::Suggestion {
    use crate::domain::Suggestion;

    let CollisionTarget {
        key,
        selector,
        sub_path,
        mods,
        winner,
    } = *target;
    let collision = ctx.report.conflicts.iter().find_map(|c| match c {
        Conflict::PatchCollision(p)
            if p.def_type == key.def_type
                && p.def_name == key.def_name
                && p.selector == selector
                && p.sub_path.as_deref() == sub_path =>
        {
            Some(p)
        }
        _ => None,
    });
    let severity = collision.map(|p| p.severity);

    let prefer_winner_alts: Vec<Alternative> = mods
        .iter()
        .map(|m| Alternative {
            action: Action::PreferWinner {
                key: key.clone(),
                winner: m.clone(),
            },
            rationale: Rationale::ForcePatchCollisionWinner,
        })
        .collect();

    if severity == Some(PatchCollisionSeverity::Additive) {
        return Suggestion {
            action: Action::Accept,
            confidence: confidence(95),
            rationale: Rationale::PatchCollisionAdditive,
            alternatives: prefer_winner_alts,
        };
    }

    // Merge is listed first among alternatives for a contested collision:
    // it's the resolving move that actually keeps every contributor's
    // work, so it leads the choice list ahead of "just pick one".
    let mut alternatives = vec![merge_alternative(
        key,
        Rationale::MergeContestedPatchCollision,
    )];
    alternatives.extend(prefer_winner_alts);

    if let Some(collision) = collision
        && collision.winner_declares_relation(winner, ctx.mods_by_id)
        && let Some(field) = sub_path
    {
        return Suggestion {
            action: Action::Accept,
            confidence: confidence(80),
            rationale: Rationale::PatchCollisionWinnerDeclaresRelation {
                others: other_patchers(collision, winner),
                winner: winner.clone(),
                field: field.to_string(),
            },
            alternatives,
        };
    }
    Suggestion {
        action: Action::Accept,
        confidence: confidence(50),
        rationale: Rationale::PatchCollisionContested,
        alternatives,
    }
}

/// A real replay predicts this operation fails to apply
/// under the order this pass ran for. Order-fixable causes (a real earlier
/// remover, or a not-yet-loaded injector) offer the matching `Reorder` as
/// an alternative — mirrors `undeclared_hard_dependency`'s/
/// `lazy_reference_violated`'s own "accept by default, but a real
/// load-order fix is one click away" shape, since accepting a predicted
/// failure and reordering to prevent it are both genuinely reasonable
/// defaults depending on whether the user *wants* the failing mod's
/// change at all. `DeadTarget`/`Unknown` have no mod a reorder could name,
/// so neither offers one — see `PatchFailureCause`'s own doc comment for
/// why: a def-order fix doesn't exist for either.
pub(super) fn patch_will_fail(
    mod_id: &ModId,
    operation: &str,
    leaf_xpath: Option<&str>,
    cause: &PatchFailureCause,
) -> crate::domain::Suggestion {
    // The rationale text reads better naming the specific leaf a caveat
    // actually pinpointed (`Defs/ThingDef[...]/comps`) than the
    // top-level operation's own log identity
    // (`Verse.PatchOperationSequence(count=7, ...)`) when one is on
    // hand; falls back to the top-level identity for the rare case a
    // top-level failure traces to no identifiable leaf at all (see
    // `Finding::PatchWillFail::leaf_xpath`'s own doc comment).
    let xpath = leaf_xpath.unwrap_or(operation);
    match cause {
        PatchFailureCause::RemovedBy(remover) => crate::domain::Suggestion {
            action: Action::Accept,
            confidence: confidence(60),
            rationale: Rationale::PatchWillFailRemovedBy {
                remover: remover.clone(),
                mod_id: mod_id.clone(),
                xpath: xpath.to_string(),
            },
            alternatives: vec![Alternative {
                action: Action::Reorder {
                    after: remover.clone(),
                    before: mod_id.clone(),
                },
                rationale: Rationale::ReorderBeforeRemover {
                    mod_id: mod_id.clone(),
                    remover: remover.clone(),
                },
            }],
        },
        PatchFailureCause::NotYetInjected(injector) => crate::domain::Suggestion {
            action: Action::Accept,
            confidence: confidence(60),
            rationale: Rationale::PatchWillFailNotYetInjected {
                mod_id: mod_id.clone(),
                injector: injector.clone(),
                xpath: xpath.to_string(),
            },
            alternatives: vec![Alternative {
                action: Action::Reorder {
                    after: mod_id.clone(),
                    before: injector.clone(),
                },
                rationale: Rationale::ReorderAfterInjector {
                    mod_id: mod_id.clone(),
                    injector: injector.clone(),
                },
            }],
        },
        PatchFailureCause::DeadTarget => crate::domain::Suggestion {
            action: Action::Accept,
            confidence: confidence(80),
            rationale: Rationale::PatchWillFailDeadTarget {
                mod_id: mod_id.clone(),
                xpath: xpath.to_string(),
            },
            alternatives: Vec::new(),
        },
        PatchFailureCause::Unknown => crate::domain::Suggestion {
            action: Action::Accept,
            confidence: confidence(30),
            rationale: Rationale::PatchWillFailUnknownCause {
                mod_id: mod_id.clone(),
                xpath: xpath.to_string(),
            },
            alternatives: Vec::new(),
        },
    }
}

/// See [`crate::domain::Finding::BrokenInheritance`]. Informational, like
/// [`super::assets::undecodable_texture`]: there is no alternative to
/// offer — the fix is the referenced mod's own `ParentName`/template
/// authoring, outside this workspace, and (unlike a def override or
/// patch collision) no load-order change can affect the resolution at
/// all when the problem is `MissingParent` (nothing registers the name
/// in any order) or make the *wrong* registration disappear when it's
/// `ParentTypeMismatch` (the nearest one at this mod's own position is
/// what it is).
pub(super) fn broken_inheritance() -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(90),
        rationale: Rationale::BrokenInheritance,
        alternatives: Vec::new(),
    }
}

/// See [`crate::domain::Finding::DanglingDefReference`]. Informational,
/// like [`broken_inheritance`] just above: cross-references resolve once,
/// after every def and patch has loaded, so this is never an ordering
/// fact, and there is no `Reorder` alternative that could change it. The
/// rationale states the cause in words, per
/// `rim_analyzer::domain::DanglingCause`'s own variants, plus a
/// `SoundDef`-fallback caveat when the referencing field's own resolved
/// values are mostly sounds (`SoundDef.Named` logs a warning and falls
/// back to an "undefined" sound rather than failing hard).
pub(super) fn dangling_def_reference(
    cause: &rim_analyzer::domain::DanglingCause,
    likely_sound: bool,
) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(85),
        rationale: Rationale::DanglingDefReference {
            cause: cause.clone(),
            likely_sound,
        },
        alternatives: Vec::new(),
    }
}

fn find_patch_collision<'a>(
    report: &'a Report,
    def_type: &str,
    def_name: &str,
) -> Option<&'a rim_analyzer::domain::PatchCollision> {
    report.conflicts.iter().find_map(|c| match c {
        Conflict::PatchCollision(p) if p.def_type == def_type && p.def_name == def_name => Some(p),
        _ => None,
    })
}

/// The deliberate-override finding for
/// [`crate::domain::EdgeKind::ReplaceDiscardsAddition`]
/// ([`crate::domain::FindingKey::DiscardedAddition`]): `replacer`'s own
/// `About.xml` already declares it loads after `adder`, so `adder`'s own
/// content being discarded by `replacer`'s `PatchOperationReplace` is a
/// choice `replacer`'s own author made, not a silent accident — this is
/// disclosure, not a decision point, so it's `Accept` with no ordering
/// alternative (there is no edge to reorder against: rule 3 is exactly
/// why one was never emitted in the first place). When a `PatchCollision`
/// exists for the same def, its own `Merge` alternative is offered too,
/// so the user can still keep `adder`'s own content field by field
/// without contradicting `replacer`'s own declared order.
pub(super) fn discarded_addition(
    replacer: &ModId,
    adder: &ModId,
    def: &DefKey,
    path: &str,
    adder_path: &str,
    ctx: &SuggestContext<'_>,
) -> crate::domain::Suggestion {
    let mut alternatives = Vec::new();
    if find_patch_collision(ctx.report, &def.def_type, &def.def_name).is_some() {
        alternatives.push(merge_alternative(
            def,
            Rationale::MergeContestedPatchCollision,
        ));
    }
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(80),
        rationale: Rationale::DiscardedAdditionDeliberate {
            replacer: replacer.clone(),
            adder: adder.clone(),
            path: path.to_string(),
            adder_path: adder_path.to_string(),
        },
        alternatives,
    }
}
