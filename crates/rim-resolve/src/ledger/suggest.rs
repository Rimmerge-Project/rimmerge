//! The confidence table:
//! table-driven, per-finding-kind [`Suggestion`]s.

use std::collections::BTreeMap;

use crate::domain::{Confidence, Finding};
use crate::sort::{Layer, SortOutcome};
use assets::{
    duplicate_template_name, keyed_translation_collision, missing_texture_path, sound_override,
    texture_override, undecodable_texture,
};
use defs::{
    CollisionTarget, broken_inheritance, dangling_def_reference, def_override, discarded_addition,
    patch_collision, patch_will_fail,
};
use edges::{any_of_choice, declaration_overridden, declaration_questioned, edge_dropped};
use mods::{
    contributes_nothing, duplicate_assembly, incompatible_pair, lazy_reference_violated,
    likely_duplicate_mod, missing_dependency, missing_mod, near_miss_mod_reference,
    runtime_patch_collision, transpiler_collision, undeclared_hard_dependency,
    undeclared_type_dependency, unsupported_version,
};
use rim_analyzer::domain::{EdgeStrength, LoadOrder, Mod, ModId, Report};
use rules::{
    placement_ordering_overridden, placement_overruled, placement_promotes_dependents,
    placement_questioned, rule_overruled, tag_inferred,
};

mod assets;
mod defs;
mod edges;
mod mods;
mod rules;

pub use defs::{DefOverrideDirection, def_override_direction};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "suggest/suggest_tests.rs"]
mod tests;

/// Everything a [`suggest`] call needs beyond the [`Finding`] itself: the
/// raw report (for conflict flags the [`Finding`] doesn't carry), the sort
/// outcome (for any-of/cluster-split detail), the order the suggestion is
/// being framed against, and per-mod facts.
pub struct SuggestContext<'a> {
    /// The full analyzer report.
    pub report: &'a Report,
    /// The sorter's outcome for the suggested order.
    pub sort_outcome: &'a SortOutcome,
    /// The order actually active in `ModsConfig.xml` right now — **always
    /// the real current order**, regardless of which
    /// [`OrderSource`](crate::domain::OrderSource) the ledger is being
    /// built for. The any-of rule's "a candidate already before `after`
    /// today" preference (see `any_of_choice` in this module) is a fact
    /// about what the user's list actually looks like today, not about
    /// whichever order this particular ledger happens to be evaluating.
    pub current: &'a LoadOrder,
    /// Every active mod, by id.
    pub mods_by_id: &'a BTreeMap<ModId, &'a Mod>,
}

fn confidence(percent: u8) -> Confidence {
    match Confidence::new(percent) {
        Ok(value) => value,
        Err(_) => unreachable!("suggest.rs only ever passes literal 0..=100 percentages"),
    }
}

/// The [`crate::domain::Suggestion`] for one finding, per the confidence
/// table.
#[must_use]
pub fn suggest(finding: &Finding, ctx: &SuggestContext<'_>) -> crate::domain::Suggestion {
    match finding {
        Finding::EdgeDropped {
            after,
            before,
            kind,
            strength,
            winner,
            ..
        } => edge_dropped(*kind, *strength, after, before, winner.as_ref()),
        Finding::DeclarationQuestioned {
            declared_after,
            declared_before,
            declared_layer,
            declared_detail,
            relation_kind,
            relation_detail,
        } => declaration_questioned(
            declared_after,
            declared_before,
            *declared_layer,
            declared_detail,
            *relation_kind,
            relation_detail,
        ),
        Finding::DeclarationOverridden {
            declared_after,
            declared_before,
            kind,
            detail,
            by,
        } => declaration_overridden(declared_after, declared_before, *kind, detail, by),
        Finding::AnyOfChoice {
            after,
            assembly,
            candidates,
        } => any_of_choice(after, assembly, candidates, ctx),
        Finding::DefOverride {
            key,
            owners,
            winner,
        } => def_override(key, owners, winner, ctx),
        Finding::PatchCollision {
            key,
            selector,
            sub_path,
            mods,
            winner,
        } => patch_collision(
            &CollisionTarget {
                key,
                selector: *selector,
                sub_path: sub_path.as_deref(),
                mods,
                winner,
            },
            ctx,
        ),
        Finding::TextureOverride {
            texture_path,
            owners,
            ..
        } => texture_override(texture_path, owners),
        Finding::DuplicateAssembly { owners, .. } => duplicate_assembly(owners),
        Finding::DuplicateTemplateName { name, owners } => duplicate_template_name(name, owners),
        Finding::KeyedTranslationCollision { keys, .. } => keyed_translation_collision(keys.len()),
        Finding::SoundOverride { path, owners } => sound_override(path, owners),
        Finding::UndeclaredTypeDependency { user, provider, .. } => {
            undeclared_type_dependency(user, provider)
        }
        Finding::RuntimePatchCollision {
            target_type,
            target_method,
            owners,
        } => runtime_patch_collision(target_type, target_method, owners, ctx),
        Finding::TranspilerCollision {
            target_type,
            target_method,
            owners,
        } => transpiler_collision(target_type, target_method, owners),
        Finding::RuleOverruled {
            after,
            before,
            origin,
            winner,
            overrides_declared,
            ..
        } => rule_overruled(after, before, *origin, winner.as_ref(), *overrides_declared),
        Finding::PlacementOverruled {
            mod_id,
            placement,
            origin,
            by,
            ..
        } => placement_overruled(mod_id, *placement, *origin, by),
        Finding::PlacementQuestioned {
            mod_id,
            placement,
            other,
            relation_kind,
            relation_detail,
        } => placement_questioned(mod_id, *placement, other, *relation_kind, relation_detail),
        Finding::PlacementOrderingOverridden {
            mod_id,
            pinned,
            placement,
            by,
        } => placement_ordering_overridden(mod_id, pinned, *placement, by),
        Finding::PlacementPromotesDependents {
            mod_id,
            placement,
            promoted,
        } => placement_promotes_dependents(mod_id, *placement, promoted),
        Finding::LikelyDuplicateMod { a, b, .. } => likely_duplicate_mod(a, b),
        Finding::MissingMod { mod_id } => missing_mod(mod_id),
        Finding::MissingDependency { .. } => missing_dependency(),
        Finding::IncompatiblePair { a, b } => incompatible_pair(a, b),
        Finding::UnsupportedVersion { .. } => unsupported_version(),
        Finding::UndeclaredHardDependency { after, before, .. } => {
            undeclared_hard_dependency(after, before)
        }
        Finding::LazyReferenceViolated { after, before, .. } => {
            lazy_reference_violated(after, before)
        }
        Finding::TagInferred {
            mod_id,
            tag,
            matched,
        } => tag_inferred(mod_id, tag, matched),
        Finding::MissingTexturePath { .. } => missing_texture_path(),
        Finding::UndecodableTexture { .. } => undecodable_texture(),
        Finding::PatchWillFail {
            mod_id,
            operation,
            leaf_xpath,
            cause,
            ..
        } => patch_will_fail(mod_id, operation, leaf_xpath.as_deref(), cause),
        Finding::ContributesNothing { mod_id } => contributes_nothing(mod_id),
        Finding::BrokenInheritance { .. } => broken_inheritance(),
        Finding::NearMissModReference {
            kind,
            rule,
            written,
            candidate_name,
            ..
        } => near_miss_mod_reference(*kind, *rule, written, candidate_name),
        Finding::DiscardedAddition {
            replacer,
            adder,
            def,
            path,
            adder_path,
        } => discarded_addition(replacer, adder, def, path, adder_path, ctx),
        Finding::DanglingDefReference {
            cause,
            likely_sound,
            ..
        } => dangling_def_reference(cause, *likely_sound),
    }
}

/// The confidence split for a [`Finding::EdgeDropped`] whose witness
/// cycle was a direct two-mod contradiction (`winner.is_some()`), scored by
/// *both* the winner's layer and the dropped (loser) edge's own
/// [`EdgeStrength`] — not the winner's layer alone: a same-layer
/// `Declared`-vs-`Declared` contradiction, broken
/// only by the drop tie-break, is not one author's word beating the
/// other's, so it must not be scored as if the winner had out-precedenced
/// a weaker fact.
///
/// - `Hard`/resolved-any-of winner: `Some(95)` regardless of the loser —
///   a load-time fact always wins outright. A `Layer::DeclaredOverride`
///   winner joins this same unconditional bucket (the declared-edge
///   override) rather than the
///   `Declared`/`UserDecision` bucket below it: unlike that bucket, it
///   never faces a same-layer engine-edge tie to guard against — no
///   engine edge is ever added at `Layer::DeclaredOverride`, so every
///   loser this winner ever actually faces (always `Declared`-strength
///   in practice, since that's the only edge this layer exists to beat —
///   see `Layer::DeclaredOverride`'s own doc comment) is a genuine,
///   deliberate cross-layer win, never a tie-break-decided accident.
/// - `Declared`/`UserDecision` winner: `Some(95)` when the loser is
///   weaker than `Declared` (a real precedence win, "`Declared`-over-db"),
///   but `None` — falling through to the existing
///   strength-based table's 55 ("needs input") row — when the loser is
///   *also* `Declared` (two authors, or an author and the user, disagree;
///   the tie-break decided it, not evidence).
/// - An imported-rule (db) winner: `Some(70)` ("db-over-declared") only
///   when the loser is `Declared`; `None` when the loser is `Soft`/
///   `Inferred`/`Awareness`, so dropping a merely-advisory (or heuristic)
///   edge keeps the base table's higher-confidence 90 row rather than
///   being pulled down to 70.
/// - A `Soft`/`Awareness`/`Inferred` winner: always `None` — falls
///   through to the existing strength-based table, since none of "Hard",
///   "db", or "declared" wording fits a heuristic or a merely-advisory
///   edge either. `Layer::Inferred` is reachable here only narrowly: it
///   can only ever beat a
///   `Soft`/`Awareness` loser (the two layers after it in `LAYER_ORDER`),
///   which needs the caller to have turned on `enforce.soft`/
///   `enforce.awareness` too — `EnforcedLayers::inferred`'s own `true`
///   default alone isn't enough to reach this case, since `Inferred`
///   never faces a `Soft`/`Awareness` *loser* unless those are real
///   constraints too.
fn winner_confidence(winner_layer: Layer, loser_strength: EdgeStrength) -> Option<u8> {
    match winner_layer {
        Layer::Hard | Layer::AnyOf | Layer::DeclaredOverride => Some(95),
        Layer::Declared | Layer::UserDecision => match loser_strength {
            EdgeStrength::Declared => None,
            // An `Inferred` loser is weaker than an author declaration
            // either way, same bucket as `Awareness`/`Soft`.
            EdgeStrength::Hard
            | EdgeStrength::Soft
            | EdgeStrength::Inferred
            | EdgeStrength::Awareness => Some(95),
        },
        Layer::RimSortUser | Layer::RimSortCommunity | Layer::SteamDb => match loser_strength {
            EdgeStrength::Declared => Some(70),
            EdgeStrength::Hard
            | EdgeStrength::Soft
            | EdgeStrength::Inferred
            | EdgeStrength::Awareness => None,
        },
        Layer::Soft | Layer::Awareness | Layer::Inferred => None,
    }
}

/// The `Reorder`-with-the-loser alternative on a
/// winner-naming suggestion is only ever offered when the winner is an
/// imported (RimSort/SteamDB) rule — never against a `Hard` fact, an
/// author's own `Declared` order, or the user's own prior `UserDecision`.
/// Those three can still be overridden through other, more deliberate
/// paths (a `KeepEdge`/`DropEdge` override, or a fresh decision on the
/// finding this edge itself produces), but a one-click "reorder to side
/// with the loser" button must not sit next to a fact or a declaration as
/// if flipping it were as casual as picking a side between two databases.
fn is_db_layer(layer: Layer) -> bool {
    matches!(
        layer,
        Layer::RimSortUser | Layer::RimSortCommunity | Layer::SteamDb
    )
}
