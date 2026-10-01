//! [`Confidence`], [`Action`], and the [`Suggestion`]/[`Resolution`] the
//! ledger attaches to a [`super::finding::Finding`].

use std::collections::BTreeMap;

use rim_analyzer::domain::{EdgeKind, ModId};
use serde::{Deserialize, Serialize};

use super::decision::Decision;
use super::finding::{DefKey, Finding, FindingKey};
use super::merge::{FieldPath, MergeChoice};
use super::patch::ScopeMembership;
use super::rationale::Rationale;
use super::rule::ClusterRuleId;
use super::tag::Tag;

/// A 0..=100 confidence score, validated at construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Confidence(u8);

/// [`Confidence::new`] rejects anything over 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("confidence {0} out of range: must be 0..=100")]
pub struct ConfidenceError(u8);

impl Confidence {
    /// Validates and constructs a confidence score.
    ///
    /// # Errors
    ///
    /// Returns [`ConfidenceError`] when `percent` is over 100.
    pub const fn new(percent: u8) -> Result<Self, ConfidenceError> {
        if percent <= 100 {
            Ok(Self(percent))
        } else {
            Err(ConfidenceError(percent))
        }
    }

    /// The raw 0..=100 value.
    #[must_use]
    pub const fn percent(self) -> u8 {
        self.0
    }

    /// Whether this confidence meets or exceeds `threshold`.
    #[must_use]
    pub const fn meets(self, threshold: Confidence) -> bool {
        self.0 >= threshold.0
    }
}

impl TryFrom<u8> for Confidence {
    type Error = ConfidenceError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Confidence> for u8 {
    fn from(value: Confidence) -> Self {
        value.0
    }
}

/// A resolving move the user (or an auto-applied suggestion) can take on
/// a finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// Keep whatever the selected order already produces.
    Accept,
    /// Suppress this finding; it's never re-queued.
    Ignore,
    /// Becomes a [`super::rule::PairRule`] at
    /// [`super::rule::RuleOrigin::UserDecision`].
    Reorder {
        /// The mod that must load after `before`.
        after: ModId,
        /// The mod that must load before `after`.
        before: ModId,
    },
    /// Expands to [`Action::Reorder`] with `winner` after every other
    /// owner of `key`.
    PreferWinner {
        /// The contested def.
        key: DefKey,
        /// The owner whose def should win.
        winner: ModId,
    },
    /// Resolves an any-of constraint to a specific candidate.
    ChooseCandidate {
        /// The mod requiring one of the candidates.
        after: ModId,
        /// The chosen candidate.
        chosen: ModId,
    },
    /// Forces the sorter to not add this edge.
    DropEdge {
        /// The edge's dependent side.
        after: ModId,
        /// The edge's dependency side.
        before: ModId,
        /// The kind of edge to drop.
        kind: EdgeKind,
    },
    /// Vetoes dropping this edge as a cycle-break; the sorter drops the
    /// next-weakest edge in the cycle instead.
    KeepEdge {
        /// The edge's dependent side.
        after: ModId,
        /// The edge's dependency side.
        before: ModId,
        /// The kind of edge to keep.
        kind: EdgeKind,
    },
    /// Adds a manual tag.
    AddTag {
        /// The mod to tag.
        mod_id: ModId,
        /// The tag to add.
        tag: Tag,
    },
    /// Removes a manual tag, overriding inference.
    RemoveTag {
        /// The mod to untag.
        mod_id: ModId,
        /// The tag to remove.
        tag: Tag,
    },
    /// Excludes one member from a cluster rule's contiguity requirement.
    ExcludeFromCluster {
        /// The cluster rule.
        rule: ClusterRuleId,
        /// The member to exclude.
        mod_id: ModId,
    },
    /// Removes a mod from the active list. Never auto-applied except for
    /// `MissingMod`.
    RemoveMod {
        /// The mod to remove.
        mod_id: ModId,
    },
    /// Generates a merge patch for the contested def, folding in each
    /// stored per-field choice. A field absent from `choices` takes
    /// whichever value [`rim_merge`'s `diff` module derives
    /// automatically; reverting a field to automatic is
    /// `choices.remove(path)`, never a `MergeChoice` variant.
    ///
    /// `#[serde(default)]` on `choices` keeps every `decisions.json` v1
    /// record readable: a v1 record never stores a `Merge` decision, but a
    /// hand-written or future-written record lacking `choices` still
    /// parses as an empty map.
    Merge {
        /// The contested def.
        key: DefKey,
        /// Per-field choices, keyed by the field's path under the def.
        #[serde(default)]
        choices: BTreeMap<FieldPath, MergeChoice>,
    },
    /// Copies one owner's file for a `TextureOverride` into the generated
    /// merge mod, so the choice is order-independent.
    ShipAsset {
        /// The shared, normalized texture path.
        texture_path: String,
        /// The owner whose file to copy in.
        from: ModId,
    },
    /// "Promote to user rule", reached as a decision from
    /// `RuleOverruled`'s/`PlacementOverruled`'s own "Promote" alternative:
    /// a copy of the
    /// named imported rule survives at [`super::rule::RuleOrigin::UserDecision`]
    /// even though the imported original stays overruled. Never itself a
    /// [`SorterOverrides`](super::decision::SorterOverrides) entry — a
    /// promotion changes the [`super::rule::RuleSet`] a session holds, not
    /// a per-decision override layered on top of it, so
    /// `DecisionSet::sorter_overrides` treats it the same silent way it
    /// already treats [`Action::RemoveMod`]/[`Action::Merge`]/
    /// [`Action::ShipAsset`]: an application-layer effect this crate has
    /// no rule set to apply to, carried out by `rim-session::Session::decide`
    /// calling its own `promote_imported_rule` the moment this action is
    /// recorded.
    PromoteRule {
        /// Which rule to promote.
        rule: PromotedRuleKey,
    },
    /// "Drop this pair rule, keeping the placement": a fresh
    /// [`super::rule::RuleOrigin::UserDecision`]-equivalent override that
    /// keeps `(after, before)` from ever feeding the sorter again, without
    /// touching the underlying `RuleSet` the way deleting the rule outright
    /// would — the imported original stays listed (and takes effect again
    /// the moment this decision is reverted). Unlike `Action::DropEdge`
    /// (which names an `EdgeKind` because an engine edge's identity
    /// includes one), a pair rule has no `EdgeKind` of its own.
    DropRule {
        /// The dropped rule's dependent side.
        after: ModId,
        /// The dropped rule's dependency side.
        before: ModId,
    },
}

/// Which rule [`Action::PromoteRule`] promotes: a pair or a placement
/// (promotion only ever applies to those two shapes — never an
/// [`super::rule::IncompatibleRule`], which has nothing ordering-related
/// to promote). Mirrors `rim_session::RuleKey`'s own two ordering-relevant
/// variants rather than depending on that type directly: `domain` stays
/// independent of the application layer that actually applies a promotion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PromotedRuleKey {
    /// See [`super::rule::PairRule`].
    Pair {
        /// The dependent side.
        after: ModId,
        /// The dependency side.
        before: ModId,
    },
    /// See [`super::rule::PlacementRule`].
    Placement {
        /// The pinned mod.
        mod_id: ModId,
    },
}

/// An action the user didn't take, offered alongside the suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alternative {
    /// The alternative action.
    pub action: Action,
    /// Why it's offered.
    pub rationale: Rationale,
}

/// What the ledger recommends for one finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// The recommended action.
    pub action: Action,
    /// How confident the recommendation is.
    pub confidence: Confidence,
    /// Why this action was recommended.
    pub rationale: Rationale,
    /// Other actions the user could take instead.
    pub alternatives: Vec<Alternative>,
}

/// Whether a finding still needs the user's attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    /// The suggestion's confidence meets the threshold; applied without
    /// asking.
    Auto,
    /// Below threshold and undecided; the inbox should surface it.
    NeedsInput,
    /// The user made a decision, whether or not it matches the
    /// suggestion.
    UserOverridden,
}

/// One finding, its suggestion, and (if any) the user's decision on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    /// The finding's stable identity.
    pub key: FindingKey,
    /// The finding's full evidence, for display.
    pub finding: Finding,
    /// What the ledger recommends.
    pub suggestion: Suggestion,
    /// Whether this still needs the user's attention.
    pub status: ResolutionStatus,
    /// The action actually in effect: `suggestion.action` unless
    /// `status` is `UserOverridden`.
    pub effective: Action,
    /// The user's decision, if one exists.
    pub decision: Option<Decision>,
    /// Only `Some` when this ledger was built for
    /// [`super::order::OrderSource::Current`]: would the suggested order
    /// resolve this finding?
    pub resolved_by_suggested: Option<bool>,
    /// The merge preview's status, when `effective` is [`Action::Merge`]
    /// (a decision, or an auto-accepted clean-merge suggestion — see
    /// [`redecide_for_clean_merge`]) and a preview has been computed.
    /// Always `None` coming out of [`crate::ledger::build`] — this crate
    /// has no XML to build a preview from — `rim-session` fills it in
    /// afterward from its own merge-preview cache: once from its
    /// decision-driven pass and re-deriving `status` via [`merge_status`]
    /// (see that function's doc comment for why a Merge decision's status
    /// isn't simply `UserOverridden`), and once from its clean-merge
    /// redecision, so the inbox's state pill shows for an auto-suggested
    /// merge exactly like a decided one.
    pub merge: Option<MergeState>,
    /// The structural guard's field, when `merge`
    /// is `Some` and the guard forced it to `NeedsFieldInput` — the
    /// triggering field's own display text (e.g. `"thingClass"`), mirroring
    /// [`Self::merge`]'s own "this crate has no XML, `rim-session` fills it
    /// in" story: always `None` coming out of [`crate::ledger::build`]/
    /// [`crate::ledger::scoped`], populated by `rim-session` alongside
    /// `merge` (from its own `MergePreview::structural_guard_field` — a
    /// type this crate has no XML to build and so never names). Exists so
    /// a decided-merge row's own display (the inbox card's pill, the
    /// merge-mod entry list) can read "confirm the winner" instead of a
    /// field count for a def a pre-existing `Merge` decision has since
    /// become guarded on — the same signal `redecide_for_clean_merge`'s
    /// own `structural_guard_field` parameter already carries for the
    /// *undecided* path.
    pub structural_guard_field: Option<String>,
    /// This key's membership in the patch scope the enclosing
    /// [`super::ledger::Ledger`] was derived for, if any. Always `None`
    /// coming out of [`crate::ledger::build`] (the profile ledger has no
    /// scope); `Some` for every entry [`crate::ledger::scoped`] keeps,
    /// since a dropped ([`ScopeMembership::Outside`]) key never becomes an
    /// entry there in the first place.
    pub scope: Option<ScopeMembership>,
}

/// A merge preview's status: how much of the contested def's field-level
/// diff still needs the user's input before a patch can be emitted for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeState {
    /// Every field has a value (automatic or chosen); a patch of
    /// `op_count` operations would be emitted.
    Complete {
        /// How many operations the resulting merge patch would contain.
        op_count: usize,
    },
    /// At least one field is a genuine conflict with no stored choice.
    NeedsFieldInput {
        /// How many fields still lack a choice.
        unresolved: usize,
        /// How many fields this def has in total.
        total: usize,
    },
    /// The merge can't be carried out at all (e.g. an unsupported xpath on
    /// a contributing patch).
    CannotMerge {
        /// Why, in a form the editor can display verbatim.
        reason: String,
    },
}

/// Maps a merge preview's [`MergeState`] to the [`ResolutionStatus`] its
/// `Merge` decision should show in the ledger.
///
/// A deliberate deviation from every other decided finding, which always
/// shows `UserOverridden` the moment a decision exists: a half-finished
/// merge (`NeedsFieldInput`) or one that can't be carried out at all
/// (`CannotMerge`) must stay in the default `NeedsInput` inbox view rather
/// than disappearing just because a `Merge` decision object exists on
/// file — otherwise choosing "merge" and walking away mid-edit would look,
/// to the inbox, exactly like a finished decision. Lives here (not
/// `rim-session`, the only place that can actually compute a
/// [`MergeState`], since doing so needs the def's XML) as a pure function
/// so the session has no reason to duplicate this mapping.
#[must_use]
pub fn merge_status(state: &MergeState) -> ResolutionStatus {
    match state {
        MergeState::Complete { .. } => ResolutionStatus::UserOverridden,
        MergeState::NeedsFieldInput { .. } | MergeState::CannotMerge { .. } => {
            ResolutionStatus::NeedsInput
        }
    }
}

/// Which shape [`redecide_for_clean_merge`] is redeciding for — the two
/// finding kinds it supports differ in what a clean, non-zero-op
/// `MergeState::Complete` preview may safely promise. A plain enum
/// rather than the caller's full `FindingKey` (already redundant with the
/// `key: &DefKey` parameter next to it, and heavier than this function
/// needs — it only ever asks "which of the two shapes").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeFindingKind {
    /// A field-merged copy of a def no author shipped — entering the
    /// generated merge mod needs an explicit choice.
    DefOverride,
    /// Mirrors RimWorld's own sequential patch composition — safe to
    /// promote outright.
    PatchCollision,
}

/// Re-derives a `DefOverride`/`PatchCollision` finding's suggestion once
/// its merge preview is known: a clean preview makes the suggestion
/// merge-first, and a zero-op clean preview is an `Accept`, not a
/// needs-input with a no-op `Merge`. Pure: `rim-session` calls this from its
/// own ledger-building pass (gated on the `suggestMergeWhenClean` setting)
/// once it has computed `key`'s preview through `PlanMerge`; this function
/// itself does no I/O and knows nothing about *how* `state` was computed.
/// `assumed_mod_setting_defaults` is likewise computed by the caller — it
/// means "the preview carries a `Caveat::ModSettingDefault`", but that type
/// lives in `rim-merge`, one layer above this crate in the workspace's
/// dependency graph (`rim-session -> rim-merge -> rim-resolve`), so this
/// crate takes the already-reduced `bool` rather than importing the type
/// and inverting the graph.
///
/// Only takes effect when `suggestion` already offers a `Merge`
/// alternative for `key` — every def override the confidence table
/// already resolves with a stronger signal (`winner_declares_relation`,
/// `same_author`, a lone non-vanilla owner: all `Accept` at 90+
/// confidence, see `ledger::suggest::def_override`) and every additive
/// patch collision (`Accept` 95) never offer one in the first place, so
/// they pass straight through unchanged — as do texture overrides,
/// likely-duplicate mods, and every other finding kind, none of which
/// `Action::Merge` is ever a candidate action for.
///
/// - [`MergeState::Complete`] with `op_count > 0`: at least one field's
///   automatic result actually differs from what the load-order winner's
///   own def already provides, so the merge genuinely combines both
///   mods' contributions — the suggestion becomes `Merge` at confidence
///   85, with a fixed `Accept` "keep the load-order winner" alternative
///   first, followed by `suggestion`'s own alternatives with `Merge`
///   removed (it's now the action, not an alternative).
/// - [`MergeState::Complete`] with `op_count == 0`: a zero-op preview says
///   "there is nothing to merge", never "the ledger's own direction was
///   wrong" — so when `suggestion.action` is already `Action::Accept`,
///   every field already resolves to exactly what the winner's own def
///   would produce on its own, RimWorld itself would already compose
///   these contributions to the same single value, and this becomes
///   `Accept` at confidence 95 (80 when `assumed_mod_setting_defaults` is
///   set — the replay assumed default mod settings), with `Merge`
///   **removed** from the alternatives (a zero-op `Merge` is not a real
///   alternative) and the `PreferWinner` alternatives kept (they're the
///   only way to force a different order from this row). This does not
///   file the finding as needs-input; [`merge_status`] only ever sees
///   `Complete`, `NeedsFieldInput`, or `CannotMerge`, and this branch's
///   own `Action::Accept` result carries no [`MergeState`] at all by the
///   time it reaches that function. **When `suggestion.action` is not
///   `Accept`** (`ledger::suggest::def_override`'s `ShadowsFramework`
///   branch is the one real case: action `PreferWinner` at confidence 40,
///   `Merge` only ever an *alternative*), the row's whole point is that
///   direction — a zero-op preview only means the no-op `Merge`
///   alternative is gone, it never re-points the suggestion itself, so
///   `action`/`confidence`/`rationale` all pass through unchanged
///   (replacing them unconditionally would silently turn a confidence-40
///   `ShadowsFramework` warning into a confidence-95 `Accept`).
/// - [`MergeState::NeedsFieldInput`] **with `structural_guard_field:
///   None`** (the ordinary case — a genuine per-field conflict): the
///   ledger's own suggestion is left as-is, but `Merge` moves to the
///   front of the alternatives, its rationale naming how many fields
///   still need a choice.
/// - [`MergeState::NeedsFieldInput`] **with `structural_guard_field:
/// Some(field)`** (a structural guard
///   forced this state; `rim-session`'s own `state_from_plan` sets
///   `unresolved == total` in this case specifically so the numbers never
///   contradict the state, which also means they can't be used to tell
///   this case apart from a genuine conflict — `structural_guard_field`
///   is the separate signal this crate needs instead, since it has no
///   XML of its own to re-derive the guard from `key` alone): `Merge` is
///   **removed** from the alternatives
///   outright rather than led with — this state can never become
///   `Complete`, no matter what the user picks per field, so offering it
///   as the recommended action would be recommending a dead end (leading
///   with "Merge, 9 fields need a choice." for a def where every field is
///   already `OneSided`, while the apply dialog can only ever report
///   `Skipped (still needs input)` — advice with no action the user could
///   take). `action`/`confidence` are left alone (the guard forces a
///   confirmation, it does not change who wins), but `rationale` is
///   replaced with one naming
///   the field and the confirmation being asked for, since the ledger's
///   own rationale (whichever direction produced it) never mentions the
///   structural incompatibility at all.
/// - [`MergeState::CannotMerge`]: `suggestion` comes back unchanged — a
///   preview that can't be carried out at all says nothing new about
///   whether the ledger's own suggestion is still right.
///
/// **`MergeState::Complete` with `op_count > 0` also branches on
/// `finding_kind`**: only a [`MergeFindingKind::PatchCollision`]
/// promotes outright to `Action::Merge` at confidence 85, mirroring
/// RimWorld's own sequential composition. A [`MergeFindingKind::DefOverride`]
/// instead leads with `Merge` in the alternatives — the exact
/// `NeedsFieldInput` shape above — since a field-merged def override is a
/// def copy no author ever shipped; the merge mod's own inclusion rule
/// (`rim-session`'s `render_merge_mod.rs`) only ever enters the generated
/// merge mod on an *undecided* `Merge` for a `PatchCollision`, so handing
/// an undecided `DefOverride` an `Action::Merge` suggestion here would
/// have the ledger promise something `RenderMergeMod` silently never
/// carries out — the row would read "auto: merge" with a merge-state pill
/// while `apply` drops it from both `entries` and `skipped`, with nothing
/// left to explain why.
#[must_use]
pub fn redecide_for_clean_merge(
    suggestion: Suggestion,
    key: &DefKey,
    state: &MergeState,
    assumed_mod_setting_defaults: bool,
    structural_guard_field: Option<&str>,
    finding_kind: MergeFindingKind,
) -> Suggestion {
    let offers_merge = suggestion
        .alternatives
        .iter()
        .any(|alt| matches!(&alt.action, Action::Merge { key: merge_key, .. } if merge_key == key));
    if !offers_merge {
        return suggestion;
    }

    fn without_merge(alternatives: Vec<Alternative>) -> Vec<Alternative> {
        alternatives
            .into_iter()
            .filter(|alt| !matches!(alt.action, Action::Merge { .. }))
            .collect()
    }

    fn lead_with_merge(suggestion: Suggestion, key: &DefKey, rationale: Rationale) -> Suggestion {
        let mut alternatives = vec![Alternative {
            action: Action::Merge {
                key: key.clone(),
                choices: BTreeMap::new(),
            },
            rationale,
        }];
        alternatives.extend(without_merge(suggestion.alternatives));
        Suggestion {
            action: suggestion.action,
            confidence: suggestion.confidence,
            rationale: suggestion.rationale,
            alternatives,
        }
    }

    match state {
        MergeState::Complete { op_count: 0 } => {
            // A zero-op preview says "there is
            // nothing to merge", never "the ledger's direction was
            // wrong". `ShadowsFramework`'s direction lives in the
            // *action* (`PreferWinner` at confidence 40), not in the
            // alternatives, so only the no-op `Merge` alternative is
            // stripped for it — the action, confidence, and rationale
            // are untouched.
            if !matches!(suggestion.action, Action::Accept) {
                return Suggestion {
                    alternatives: without_merge(suggestion.alternatives),
                    ..suggestion
                };
            }
            let confidence_percent = if assumed_mod_setting_defaults { 80 } else { 95 };
            Suggestion {
                action: Action::Accept,
                confidence: Confidence::new(confidence_percent)
                    .unwrap_or_else(|_| unreachable!("80 and 95 are within 0..=100")),
                rationale: Rationale::MergeCompleteNothingToMerge,
                alternatives: without_merge(suggestion.alternatives),
            }
        }
        // Only a `PatchCollision` may promote outright
        // — see this function's own doc comment above.
        MergeState::Complete { .. } if finding_kind == MergeFindingKind::DefOverride => {
            lead_with_merge(suggestion, key, Rationale::MergeLeadDefOverrideCombine)
        }
        MergeState::Complete { .. } => {
            let mut alternatives = vec![Alternative {
                action: Action::Accept,
                rationale: Rationale::KeepLoadOrderWinner,
            }];
            alternatives.extend(without_merge(suggestion.alternatives));
            Suggestion {
                action: Action::Merge {
                    key: key.clone(),
                    choices: BTreeMap::new(),
                },
                confidence: Confidence::new(85)
                    .unwrap_or_else(|_| unreachable!("85 is within 0..=100")),
                rationale: Rationale::MergePromotedPatchCollisionKeepsBoth,
                alternatives,
            }
        }
        MergeState::NeedsFieldInput { unresolved, .. } => {
            if let Some(field) = structural_guard_field {
                // This state can never become `Complete`, however the
                // fields are chosen, so leading with (or even offering)
                // `Merge` recommends an action with no way to finish —
                // strip it outright, the same way the zero-op arm above
                // strips its own no-op `Merge`. `action`/`confidence`
                // stay whatever the ledger's own suggestion already
                // said — the guard confirms the winner, it doesn't
                // re-pick one — but `rationale` is replaced: none of the
                // ledger's own rationales (an "unexplained override" or a
                // framework-shadowing warning, say) ever mention *why*
                // this def can't safely field-merge.
                return Suggestion {
                    rationale: Rationale::MergeStructuralGuard {
                        field: field.to_string(),
                    },
                    alternatives: without_merge(suggestion.alternatives),
                    ..suggestion
                };
            }
            let rationale = Rationale::MergeFieldsNeedChoice {
                unresolved: *unresolved,
            };
            lead_with_merge(suggestion, key, rationale)
        }
        MergeState::CannotMerge { .. } => suggestion,
    }
}

/// Re-derives a `DefOverride` finding's suggestion once every active
/// owner's own copy is known to be structurally identical (the copies are
/// compared before a def override is called contested). Pure, mirroring
/// [`redecide_for_clean_merge`]'s own shape: `rim-session` does the actual
/// XML reading and comparison (through the `DefSourceReader` port already
/// wired for the clean-merge redecide) and calls this only once it has
/// confirmed both preconditions this function itself does not re-check —
/// see `rim-session`'s own `Session::redecide_identical_copies_at` for
/// exactly what those are:
///
/// - the finding's suggestion carries no direction of its own (the ledger
///   confidence table's `WinnerDeclaresRelation`/lone-non-vanilla-owner/
///   `ShadowsFramework` branches are excluded — a def override already
///   explained by a stronger signal is left alone even when the copies
///   happen to agree, since this redecision is only about defs the ledger
///   has *no other way* to call settled:
///   `ledger::suggest::DefOverrideDirection::SameAuthor`/
///   `Unknown` only);
/// - every active owner's parsed copy compares equal.
///
/// When both hold, order genuinely cannot change the outcome: the
/// suggestion becomes `Accept` at confidence 99, with no `Merge`/
/// `PreferWinner` alternatives — there is nothing left to choose between.
#[must_use]
pub fn redecide_for_identical_copies(_suggestion: Suggestion) -> Suggestion {
    Suggestion {
        action: Action::Accept,
        confidence: Confidence::new(99).unwrap_or_else(|_| unreachable!("99 is within 0..=100")),
        rationale: Rationale::IdenticalCopiesOrderIrrelevant,
        alternatives: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_every_value_up_to_100() {
        assert_eq!(Confidence::new(0).unwrap().percent(), 0);
        assert_eq!(Confidence::new(100).unwrap().percent(), 100);
    }

    #[test]
    fn rejects_values_over_100() {
        assert!(Confidence::new(101).is_err());
        assert!(Confidence::new(255).is_err());
    }

    #[test]
    fn meets_is_inclusive_of_the_threshold() {
        let eighty = Confidence::new(80).unwrap();
        assert!(Confidence::new(80).unwrap().meets(eighty));
        assert!(Confidence::new(81).unwrap().meets(eighty));
        assert!(!Confidence::new(79).unwrap().meets(eighty));
    }

    #[test]
    fn a_complete_merge_state_reports_user_overridden() {
        assert_eq!(
            merge_status(&MergeState::Complete { op_count: 3 }),
            ResolutionStatus::UserOverridden
        );
    }

    #[test]
    fn a_merge_state_needing_field_input_reports_needs_input() {
        assert_eq!(
            merge_status(&MergeState::NeedsFieldInput {
                unresolved: 1,
                total: 5
            }),
            ResolutionStatus::NeedsInput
        );
    }

    #[test]
    fn a_merge_that_cannot_be_carried_out_reports_needs_input() {
        assert_eq!(
            merge_status(&MergeState::CannotMerge {
                reason: "unsupported xpath".to_string()
            }),
            ResolutionStatus::NeedsInput
        );
    }

    fn def_key() -> DefKey {
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        }
    }

    fn merge_alt(key: &DefKey, rationale: Rationale) -> Alternative {
        Alternative {
            action: Action::Merge {
                key: key.clone(),
                choices: BTreeMap::new(),
            },
            rationale,
        }
    }

    fn prefer_winner_alt(key: &DefKey, winner: &str) -> Alternative {
        Alternative {
            action: Action::PreferWinner {
                key: key.clone(),
                winner: ModId::new(winner),
            },
            rationale: Rationale::ForceDefOverrideWinner,
        }
    }

    /// An "unexplained def override" suggestion (`Accept` 60, per
    /// `ledger::suggest::def_override`'s own fallback) is the shape the
    /// clean-merge redecision most commonly re-derives: `PreferWinner`
    /// alternatives plus a
    /// trailing `Merge` one.
    fn unexplained_def_override_suggestion(key: &DefKey) -> Suggestion {
        Suggestion {
            action: Action::Accept,
            confidence: Confidence::new(60).unwrap_or_else(|_| unreachable!()),
            rationale: Rationale::DefOverrideUnexplained,
            alternatives: vec![
                prefer_winner_alt(key, "a"),
                prefer_winner_alt(key, "b"),
                merge_alt(key, Rationale::MergeUnexplainedDefOverride),
            ],
        }
    }

    /// A clean, non-zero-op preview promotes outright to
    /// `Action::Merge` only for [`MergeFindingKind::PatchCollision`] — see
    /// [`a_complete_def_override_preview_never_promotes_merge_to_the_action`]
    /// for the `DefOverride` sibling.
    #[test]
    fn a_complete_patch_collision_preview_promotes_merge_to_the_suggestion() {
        let key = def_key();
        let suggestion = unexplained_def_override_suggestion(&key);

        let redecided = redecide_for_clean_merge(
            suggestion,
            &key,
            &MergeState::Complete { op_count: 2 },
            false,
            None,
            MergeFindingKind::PatchCollision,
        );

        assert_eq!(
            redecided.action,
            Action::Merge {
                key: key.clone(),
                choices: BTreeMap::new(),
            }
        );
        assert_eq!(redecided.confidence.percent(), 85);
        assert_eq!(
            redecided.alternatives.first(),
            Some(&Alternative {
                action: Action::Accept,
                rationale: Rationale::KeepLoadOrderWinner,
            })
        );
        assert!(
            redecided
                .alternatives
                .iter()
                .all(|alt| !matches!(alt.action, Action::Merge { .. })),
            "Merge must not still be listed as an alternative once it's the suggestion: {:?}",
            redecided.alternatives
        );
        assert_eq!(
            redecided.alternatives.len(),
            3,
            "the two PreferWinner alternatives must survive alongside the new Accept one"
        );
    }

    /// The exact same clean, non-zero-op preview as the test
    /// above, but for `MergeFindingKind::DefOverride` — a field-merged def
    /// copy no author shipped must never become `Action::Merge` outright;
    /// `Merge` only leads the alternatives (the `NeedsFieldInput` shape),
    /// leaving `action`/`confidence` exactly what the ledger already
    /// suggested, so the user must click it explicitly. An undecided
    /// `DefOverride` promoted here would read "auto: merge" with a
    /// merge-state pill in the ledger while `RenderMergeMod` never collects
    /// it into either `entries` or `skipped` — a row every surface
    /// describes as one thing and the engine treats as another.
    #[test]
    fn a_complete_def_override_preview_never_promotes_merge_to_the_action() {
        let key = def_key();
        let suggestion = unexplained_def_override_suggestion(&key);

        let redecided = redecide_for_clean_merge(
            suggestion.clone(),
            &key,
            &MergeState::Complete { op_count: 2 },
            false,
            None,
            MergeFindingKind::DefOverride,
        );

        assert_eq!(
            redecided.action, suggestion.action,
            "a DefOverride's action must stay whatever the ledger already suggested"
        );
        assert_eq!(redecided.confidence, suggestion.confidence);
        assert_eq!(redecided.rationale, suggestion.rationale);
        assert_eq!(
            redecided.alternatives.first(),
            Some(&merge_alt(&key, Rationale::MergeLeadDefOverrideCombine)),
            "Merge must lead the alternatives, never become the action: {:?}",
            redecided.alternatives
        );
        assert_eq!(
            redecided.alternatives.len(),
            3,
            "the two PreferWinner alternatives survive alongside the re-worded, still-led Merge one"
        );
    }

    /// A `Complete` preview
    /// with zero ops means the "merge" is just the load-order winner's own
    /// def — nothing to actually write — so it must never be promoted to
    /// `Action::Merge` (a false "the mods change different fields"
    /// promotion at confidence 85 over a patch with no operations). Nor is
    /// it needs-input with a no-op `Merge` alternative — it's a plain
    /// `Accept` at confidence 95.
    #[test]
    fn a_complete_preview_with_zero_ops_never_promotes_to_merge() {
        let key = def_key();
        let suggestion = unexplained_def_override_suggestion(&key);

        let redecided = redecide_for_clean_merge(
            suggestion.clone(),
            &key,
            &MergeState::Complete { op_count: 0 },
            false,
            None,
            MergeFindingKind::DefOverride,
        );

        assert_eq!(
            redecided.action,
            Action::Accept,
            "a no-op merge must never become the suggestion"
        );
        assert_eq!(redecided.confidence.percent(), 95);
        assert_eq!(redecided.rationale, Rationale::MergeCompleteNothingToMerge);
        assert!(
            redecided
                .alternatives
                .iter()
                .all(|alt| !matches!(alt.action, Action::Merge { .. })),
            "Merge must be gone from the alternatives entirely: {:?}",
            redecided.alternatives
        );
        assert_eq!(
            redecided.alternatives,
            vec![prefer_winner_alt(&key, "a"), prefer_winner_alt(&key, "b")],
            "the PreferWinner alternatives must survive: they're the \
             only way to force a different order from this row"
        );
    }

    /// When the preview assumed default mod settings, the zero-op
    /// `Accept` drops to confidence 80 instead of 95.
    #[test]
    fn a_complete_preview_with_zero_ops_and_assumed_mod_settings_gets_confidence_eighty() {
        let key = def_key();
        let suggestion = unexplained_def_override_suggestion(&key);

        let redecided = redecide_for_clean_merge(
            suggestion,
            &key,
            &MergeState::Complete { op_count: 0 },
            true,
            None,
            MergeFindingKind::DefOverride,
        );

        assert_eq!(redecided.action, Action::Accept);
        assert_eq!(redecided.confidence.percent(), 80);
    }

    /// `ledger::suggest::def_override`'s
    /// `ShadowsFramework` branch carries its direction in the *action*
    /// (`PreferWinner` at confidence 40), not in the alternatives — a
    /// zero-op preview must only drop the no-op `Merge` alternative, never
    /// re-point the suggestion to a plain `Accept`, which would silently
    /// turn a "a leaf mod is quietly shadowing a shared framework's def"
    /// warning into a confidence-95 `Accept`, losing the warning and
    /// leaving no `PreferWinner` anywhere on the row.
    #[test]
    fn a_zero_op_preview_never_overrides_a_prefer_winner_suggestion() {
        let key = def_key();
        let shadows_framework_suggestion = Suggestion {
            action: Action::PreferWinner {
                key: key.clone(),
                winner: ModId::new("framework"),
            },
            confidence: Confidence::new(40).unwrap_or_else(|_| unreachable!()),
            rationale: Rationale::DefOverrideShadowsFramework,
            alternatives: vec![
                Alternative {
                    action: Action::Accept,
                    rationale: Rationale::KeepCurrentWinner,
                },
                merge_alt(&key, Rationale::MergeShadowsFrameworkFieldByField),
            ],
        };

        let redecided = redecide_for_clean_merge(
            shadows_framework_suggestion.clone(),
            &key,
            &MergeState::Complete { op_count: 0 },
            false,
            None,
            MergeFindingKind::DefOverride,
        );

        assert_eq!(
            redecided.action, shadows_framework_suggestion.action,
            "a zero-op preview must never re-point a PreferWinner suggestion to Accept"
        );
        assert_eq!(
            redecided.confidence,
            shadows_framework_suggestion.confidence
        );
        assert_eq!(redecided.rationale, shadows_framework_suggestion.rationale);
        assert_eq!(
            redecided.alternatives,
            vec![Alternative {
                action: Action::Accept,
                rationale: Rationale::KeepCurrentWinner,
            }],
            "only the no-op Merge alternative is dropped; the Accept alternative survives"
        );
    }

    #[test]
    fn needs_field_input_keeps_the_suggestion_but_leads_with_merge() {
        let key = def_key();
        let suggestion = unexplained_def_override_suggestion(&key);

        let redecided = redecide_for_clean_merge(
            suggestion.clone(),
            &key,
            &MergeState::NeedsFieldInput {
                unresolved: 2,
                total: 5,
            },
            false,
            None,
            MergeFindingKind::DefOverride,
        );

        assert_eq!(redecided.action, suggestion.action);
        assert_eq!(redecided.confidence, suggestion.confidence);
        assert_eq!(
            redecided.alternatives.first(),
            Some(&merge_alt(
                &key,
                Rationale::MergeFieldsNeedChoice { unresolved: 2 }
            ))
        );
    }

    /// A `NeedsFieldInput` forced by the structural guard
    /// never leads with (or even offers) `Merge` — it strips it entirely,
    /// the same way a zero-op `Complete` preview strips its own no-op
    /// `Merge` — and replaces the rationale with one naming the guard's
    /// own triggering field, since this state can never become `Complete`
    /// regardless of what the user picks per field.
    #[test]
    fn a_guarded_needs_field_input_strips_merge_and_names_the_field() {
        let key = def_key();
        let suggestion = unexplained_def_override_suggestion(&key);

        let redecided = redecide_for_clean_merge(
            suggestion.clone(),
            &key,
            &MergeState::NeedsFieldInput {
                unresolved: 9,
                total: 9,
            },
            false,
            Some("thingClass"),
            MergeFindingKind::DefOverride,
        );

        assert_eq!(
            redecided.action, suggestion.action,
            "the guard confirms the winner, it doesn't re-pick one"
        );
        assert_eq!(redecided.confidence, suggestion.confidence);
        assert_eq!(
            redecided.rationale,
            Rationale::MergeStructuralGuard {
                field: "thingClass".to_string()
            }
        );
        assert!(
            redecided
                .alternatives
                .iter()
                .all(|alt| !matches!(alt.action, Action::Merge { .. })),
            "Merge must be gone from the alternatives entirely, not just reordered: {:?}",
            redecided.alternatives
        );
        assert_eq!(
            redecided.alternatives,
            vec![prefer_winner_alt(&key, "a"), prefer_winner_alt(&key, "b")],
            "the PreferWinner alternatives must survive"
        );
    }

    /// The guard field is consulted only when `state` is
    /// `NeedsFieldInput` — a `Some` value alongside any other state would
    /// mean the caller mismatched `preview.structural_change` against
    /// `preview.state`, which `state_from_plan` guarantees can't happen
    /// (the guard forces `NeedsFieldInput`, always), but this pins that
    /// this function itself never reads the field outside that one arm.
    #[test]
    fn a_guard_field_alongside_complete_is_never_consulted() {
        let key = def_key();
        let suggestion = unexplained_def_override_suggestion(&key);

        let redecided = redecide_for_clean_merge(
            suggestion.clone(),
            &key,
            &MergeState::Complete { op_count: 0 },
            false,
            Some("thingClass"),
            MergeFindingKind::DefOverride,
        );

        assert_eq!(
            redecided.rationale,
            Rationale::MergeCompleteNothingToMerge,
            "Complete's own zero-op rationale must win — the guard field is meaningless here"
        );
    }

    #[test]
    fn needs_field_input_uses_singular_wording_for_one_field() {
        let key = def_key();
        let suggestion = unexplained_def_override_suggestion(&key);

        let redecided = redecide_for_clean_merge(
            suggestion,
            &key,
            &MergeState::NeedsFieldInput {
                unresolved: 1,
                total: 5,
            },
            false,
            None,
            MergeFindingKind::DefOverride,
        );

        assert_eq!(
            redecided.alternatives.first().map(|alt| &alt.rationale),
            Some(&Rationale::MergeFieldsNeedChoice { unresolved: 1 })
        );
    }

    #[test]
    fn cannot_merge_leaves_the_suggestion_unchanged() {
        let key = def_key();
        let suggestion = unexplained_def_override_suggestion(&key);

        let redecided = redecide_for_clean_merge(
            suggestion.clone(),
            &key,
            &MergeState::CannotMerge {
                reason: "unsupported xpath".to_string(),
            },
            false,
            None,
            MergeFindingKind::DefOverride,
        );

        assert_eq!(redecided, suggestion);
    }

    /// A finding whose suggestion never offered `Merge` at all (e.g. an
    /// additive patch collision, or a def override the confidence table
    /// already resolved with a stronger signal) must never gain one just
    /// because a preview happens to be clean.
    #[test]
    fn a_suggestion_with_no_merge_alternative_is_never_redecided() {
        let key = def_key();
        let suggestion = Suggestion {
            action: Action::Accept,
            confidence: Confidence::new(95).unwrap_or_else(|_| unreachable!()),
            rationale: Rationale::PatchCollisionAdditive,
            alternatives: vec![prefer_winner_alt(&key, "a")],
        };

        let redecided = redecide_for_clean_merge(
            suggestion.clone(),
            &key,
            &MergeState::Complete { op_count: 0 },
            false,
            None,
            MergeFindingKind::DefOverride,
        );

        assert_eq!(redecided, suggestion);
    }

    /// A v1 `decisions.json` record never contains a `Merge` action, so no
    /// real v1 file has a `choices` field to omit — but `#[serde(default)]`
    /// still has to hold for a hand-written or future-written record that
    /// does, since nothing else guarantees `choices` is always present.
    #[test]
    fn merge_action_deserializes_without_a_choices_field() {
        let json = r#"{"action":"merge","key":{"def_type":"ThingDef","def_name":"Wall"}}"#;

        let action: Action = serde_json::from_str(json).expect("choices defaults to empty");

        assert_eq!(
            action,
            Action::Merge {
                key: def_key(),
                choices: BTreeMap::new(),
            }
        );
    }

    #[test]
    fn merge_action_with_choices_round_trips_through_json() {
        let mut choices = BTreeMap::new();
        choices.insert(
            "label".parse().unwrap(),
            MergeChoice::From {
                mod_id: ModId::new("example.bionicsfork"),
            },
        );
        choices.insert(
            "labelNoun".parse().unwrap(),
            MergeChoice::Value {
                text: "a bionic heart".to_string(),
            },
        );
        choices.insert("description".parse().unwrap(), MergeChoice::Drop);
        let action = Action::Merge {
            key: def_key(),
            choices,
        };

        let json = serde_json::to_string(&action).expect("serializable");
        let round_tripped: Action = serde_json::from_str(&json).expect("deserializable");

        assert_eq!(round_tripped, action);
    }

    #[test]
    fn ship_asset_action_round_trips_through_json() {
        let action = Action::ShipAsset {
            texture_path: "Things/Wall.png".to_string(),
            from: ModId::new("example.bionicsfork"),
        };

        let json = serde_json::to_string(&action).expect("serializable");
        let round_tripped: Action = serde_json::from_str(&json).expect("deserializable");

        assert_eq!(round_tripped, action);
        assert!(json.contains(r#""action":"ship_asset""#));
    }

    /// `Action::PromoteRule` (the "Promote" alternative) round-trips
    /// through the same
    /// serde JSON form `decisions.json` persists.
    #[test]
    fn promote_rule_action_round_trips_through_json() {
        let action = Action::PromoteRule {
            rule: PromotedRuleKey::Pair {
                after: ModId::new("example.bionicsfork"),
                before: ModId::new("biotech"),
            },
        };

        let json = serde_json::to_string(&action).expect("serializable");
        let round_tripped: Action = serde_json::from_str(&json).expect("deserializable");

        assert_eq!(round_tripped, action);
        assert!(json.contains(r#""action":"promote_rule""#));
    }

    /// The same, for [`PromotedRuleKey::Placement`] — `PromoteRule`'s
    /// other shape.
    #[test]
    fn promote_rule_action_for_a_placement_round_trips_through_json() {
        let action = Action::PromoteRule {
            rule: PromotedRuleKey::Placement {
                mod_id: ModId::new("example.framework"),
            },
        };

        let json = serde_json::to_string(&action).expect("serializable");
        let round_tripped: Action = serde_json::from_str(&json).expect("deserializable");

        assert_eq!(round_tripped, action);
    }

    /// `Action::DropRule` (the
    /// "drop this pair rule, keeping the placement" alternative)
    /// round-trips through the same serde JSON form `decisions.json`
    /// persists.
    #[test]
    fn drop_rule_action_round_trips_through_json() {
        let action = Action::DropRule {
            after: ModId::new("example.bionicsfork"),
            before: ModId::new("biotech"),
        };

        let json = serde_json::to_string(&action).expect("serializable");
        let round_tripped: Action = serde_json::from_str(&json).expect("deserializable");

        assert_eq!(round_tripped, action);
        assert!(json.contains(r#""action":"drop_rule""#));
    }

    /// "Old-file" compatibility: a `decisions.json` record written before
    /// `Action::PromoteRule`/`Action::DropRule` existed (an ordinary
    /// `Reorder`, here) must still deserialize exactly as before — adding
    /// new tagged variants to this internally-tagged enum must never
    /// disturb parsing of the tags that already existed.
    #[test]
    fn a_pre_promote_rule_and_drop_rule_record_still_deserializes() {
        let json = r#"{"action":"reorder","after":"example.bionicsfork","before":"biotech"}"#;

        let action: Action =
            serde_json::from_str(json).expect("an old Reorder record must still parse");

        assert_eq!(
            action,
            Action::Reorder {
                after: ModId::new("example.bionicsfork"),
                before: ModId::new("biotech"),
            }
        );
    }

    /// Once `rim-session` has confirmed both preconditions, this
    /// always promotes to `Accept` 99 with no alternatives — regardless of
    /// what the input suggestion looked like, since the caller has already
    /// decided that this input qualifies.
    #[test]
    fn identical_copies_always_promote_to_accept_99_with_no_alternatives() {
        let key = def_key();
        let suggestion = unexplained_def_override_suggestion(&key);

        let redecided = redecide_for_identical_copies(suggestion);

        assert_eq!(redecided.action, Action::Accept);
        assert_eq!(redecided.confidence.percent(), 99);
        assert_eq!(
            redecided.rationale,
            Rationale::IdenticalCopiesOrderIrrelevant
        );
        assert!(redecided.alternatives.is_empty());
    }
}
