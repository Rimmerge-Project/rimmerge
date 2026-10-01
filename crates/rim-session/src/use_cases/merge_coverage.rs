//! [`MergeCoverage`]: both coverage tables `rimmerge merge coverage` renders.
//!
//! **Patch collisions**: for each contested patch collision key, plans a
//! preview through [`super::plan_merge::PlanMerge`] and tallies the result.
//! The tally lives in this crate rather than in `apps/cli`, which is a
//! composition root, since this crate already owns the planner-loop shape of
//! every other coverage-shaped use case (`CLAUDE.md`'s layering rule).
//!
//! **Def overrides**: enumerates every `Conflict::DefOverride` from the
//! report, plans each through [`super::plan_merge::PlanMerge`], and tallies
//! by owner count, by whether [`rim_merge::diff::structural_change`] (the
//! structural guard's trigger predicate) would fire, by preview state, and by
//! how many currently promote to `Action::Merge` at confidence 85.
//! `PlanMerge` applies the structural guard itself, so `preview` (the
//! `PreviewStateTally`) reflects the guard's *real* effect: a def the guard
//! fires for is `needs_field_input`, never `complete_zero_ops`/
//! `complete_with_ops`, regardless of how clean its per-field diff would
//! otherwise be. `guard_fires`/`guard_does_not_fire`/`guard_reread_failed`
//! below are an independent measurement: `record_guard_outcome` re-reads the
//! owners' raw/resolved data itself rather than trusting
//! `preview.structural_change`. `total` always splits cleanly into exactly
//! `planning_failures` plus every finding that got a real preview
//! (`preview.record`ed and guard-evaluated) — a planning failure is never
//! folded into `preview.cannot_merge`, since a computed
//! `MergeState::CannotMerge` is guard-evaluated same as any other preview,
//! while a planning failure has no preview to evaluate the guard against at
//! all.

use std::collections::BTreeMap;

use rim_analyzer::domain::{Conflict, LoadOrder, PatchCollisionSeverity};
use rim_merge::diff::{StructuralField, structural_change};
use rim_resolve::domain::{
    Action, Confidence, DefKey, FindingKey, MergeFindingKind, MergeState, Suggestion,
};

use super::plan_merge::{self, PlanMerge};
use crate::Session;
use crate::merge_workspace::MergePreview;
use crate::ports::DefSourceReader;

/// One merge preview's [`MergeState`], bucketed. Only ever recorded for a
/// finding that actually got a preview — a planning failure (the
/// finding's own source couldn't be read at all) is tracked separately,
/// on [`DefOverrideCoverage::planning_failures`], never folded in here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreviewStateTally {
    /// [`MergeState::Complete`] with `op_count: 0` — a preview whose
    /// merge would write nothing.
    pub complete_zero_ops: usize,
    /// [`MergeState::Complete`] with `op_count > 0`.
    pub complete_with_ops: usize,
    /// [`MergeState::NeedsFieldInput`].
    pub needs_field_input: usize,
    /// [`MergeState::CannotMerge`] — a preview that *computed*, but
    /// concluded the merge can't be carried out (e.g. an unsatisfiable
    /// scope). Distinct from a planning failure: this preview still has a
    /// real `ThreeWayDiff`, so the structural guard is still evaluated
    /// against it.
    pub cannot_merge: usize,
    /// `cannot_merge`'s own reasons, counted.
    pub cannot_merge_reasons: BTreeMap<String, usize>,
}

impl PreviewStateTally {
    fn record(&mut self, state: &MergeState) {
        match state {
            MergeState::Complete { op_count: 0 } => self.complete_zero_ops += 1,
            MergeState::Complete { .. } => self.complete_with_ops += 1,
            MergeState::NeedsFieldInput { .. } => self.needs_field_input += 1,
            MergeState::CannotMerge { reason } => {
                self.cannot_merge += 1;
                *self.cannot_merge_reasons.entry(reason.clone()).or_insert(0) += 1;
            }
        }
    }
}

/// What [`redecide_for_clean_merge`](rim_resolve::domain::redecide_for_clean_merge)
/// actually did to one finding's real ledger suggestion — the clean-merge
/// redecision's effect made measurable: how many findings actually leave
/// the needs-input inbox once redecided, as opposed to `ledger --report`'s
/// bare-JSON numbers, which never run this redecision at all.
///
/// **Two orthogonal facts, deliberately not one enum**: *what the
/// redecision did* (`Redecision`) and *whether the result clears the inbox
/// threshold* (`InboxOutcome`). A single `match (action, confidence)`
/// catch-all is wrong at both ends: a `WinnerDeclaresRelation`/`SameAuthor`
/// row (`Accept` 90-95, no `Merge` alternative at all) passes straight
/// through `redecide_for_clean_merge`'s own `offers_merge` gate untouched,
/// but a bare `(Action::Accept, 95)` match would file it as `Accept95` —
/// the zero-op clean-merge arm — even though the redecision never touched it
/// and its own preview might not even be `Complete`; simultaneously every
/// genuinely unmatched shape (a `PreferWinner`-actioned `ShadowsFramework`
/// row the redecision only stripped the no-op `Merge` alternative from, or
/// any other pass-through) would fall into a bare `NeedsInput` catch-all
/// even when its real confidence already clears the threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuggestionOutcome {
    /// What the redecision did to the suggestion.
    pub redecision: Redecision,
    /// Whether the *result* clears the profile's own threshold — mirrors
    /// `ledger::build`'s own `suggestion.confidence.meets(threshold) =>
    /// Auto` gate exactly (the same `>=` comparison, no stored-decision
    /// check — see [`DefOverrideCoverage::promotes_to_merge_85`]'s own
    /// "ceiling, not a live count" caveat, which applies here too), so
    /// this field and a live ledger's own `ResolutionStatus` can never
    /// silently drift apart for a decision-less profile.
    pub inbox: InboxOutcome,
}

/// What [`rim_resolve::domain::redecide_for_clean_merge`] did to a
/// finding's suggestion — see [`SuggestionOutcome`]'s own doc comment for
/// why this is split from `InboxOutcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Redecision {
    /// The structural guard fired for this finding
    /// ([`crate::merge_workspace::MergePreview::structural_guard_field`]
    /// is `Some`, forcing [`MergeState::NeedsFieldInput`] regardless of
    /// the per-field diff — `DefOverride` only, always absent for a
    /// `PatchCollision`). Checked directly off the preview, *before*
    /// `redecide_for_clean_merge` is even called: a finding whose
    /// original suggestion never offers `Merge` in the first place (a
    /// `DefOverride` classified `SameAuthor`/`LoneNonVanillaOwner` —
    /// `redecide_for_clean_merge`'s own `offers_merge` gate) would
    /// otherwise bypass the guard branch entirely and come back
    /// `Redecision::Unchanged` even when its preview also happens to
    /// carry a structural change — it is still counted `Guarded` here,
    /// since the guard forced this finding's `MergeState` regardless of what the
    /// suggestion layer does with it (exactly what
    /// [`PreviewStateTally::needs_field_input`] already reflects for the
    /// same finding). A vanishingly rare combination in practice (a
    /// same-author copy that also diverges structurally), disclosed
    /// rather than silently mis-bucketed.
    Guarded,
    /// `redecide_for_clean_merge` returned the suggestion byte-for-byte
    /// unchanged — either its own `offers_merge` gate never fired
    /// (`WinnerDeclaresRelation`/`SameAuthor`/`LoneNonVanillaOwner`/
    /// `ShadowsFramework`'s own zero-op case, an additive patch collision,
    /// ...), or the state was [`MergeState::CannotMerge`]. This bucket is
    /// the redecision's own "nothing to attribute" answer — it says
    /// nothing about whether the *unchanged* suggestion is `Auto` or
    /// `NeedsInput`; read [`SuggestionOutcome::inbox`] for that.
    Unchanged,
    /// The zero-op clean-merge arm, confidence 95 — no
    /// `Caveat::ModSettingDefault` was assumed.
    Accept95,
    /// The zero-op clean-merge arm, confidence 80 — the replay assumed
    /// default mod settings. **Structurally always `0` on the def-override
    /// side**: `plan_def_override` never
    /// emits `Caveat::ModSettingDefault` — only a patch-collision replay
    /// (`patch_eval::replay`) ever does. On the patch-collision side, the
    /// predicate is **def-wide, not field-wide**: `plan_patch_collision`
    /// seeds `plan.caveats` from a replay over *every* active contribution
    /// to the whole def, not scoped to this one collision's own sub-path,
    /// so one framework mod wrapping unrelated patches in a mod-setting
    /// conditional taints `accept_80` for every collision on that def,
    /// even where the caveated op never touches the contested field.
    /// Combined with confidence 80 being unobservable at the default
    /// threshold (`Confidence::meets` is `>=`, and 80 is the default), this
    /// downgrade currently buys nothing while implying a field-level
    /// precision the replay doesn't have.
    Accept80,
    /// The clean-merge promotion (a `Complete` preview with
    /// a nonzero op count), confidence 85 — see
    /// [`DefOverrideCoverage::promotes_to_merge_85`]'s own doc comment for
    /// the real preconditions this ceiling doesn't check.
    /// **`PatchCollision` only**: a `DefOverride`'s own non-zero-op
    /// `Complete` arm leads with `Merge` in the alternatives instead
    /// (`Redecision::OtherRewrite`, action/confidence untouched) and never
    /// becomes this action — see
    /// `rim_resolve::domain::redecide_for_clean_merge`'s own doc comment
    /// for why: the merge mod only carries an undecided `Merge` for a
    /// `PatchCollision`, so an undecided `DefOverride` promoted here
    /// would have the ledger promise something `RenderMergeMod` silently
    /// never carries out.
    Merge85,
    /// `redecide_for_clean_merge` rewrote the suggestion, but not into
    /// `Accept` 95/80 or `Merge` 85 — this is exactly the structural guard's
    /// `NeedsFieldInput`-with-a-guard-field rationale rewrite (action and
    /// confidence untouched, only `rationale`/`alternatives` change), and
    /// the `NeedsFieldInput`-without-a-guard "lead with Merge in the
    /// alternatives" rewrite (also action/confidence untouched). Kept
    /// distinct from `Unchanged` because the suggestion genuinely differs
    /// even though neither is a named "outcome" bucket of its own.
    OtherRewrite,
}

/// Whether a finding's *final*, post-redecision confidence clears the
/// profile's own threshold — see [`SuggestionOutcome::inbox`]'s own doc
/// comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InboxOutcome {
    /// `confidence.meets(threshold)` — would show `Auto` in a
    /// decision-less ledger.
    Auto,
    /// Below threshold — would show `NeedsInput`.
    NeedsInput,
}

/// Tallies [`SuggestionOutcome`] across a set of findings, on both its
/// independent axes (`Redecision` and `InboxOutcome` — one finding
/// increments exactly one field in each group, so `accept_95 + accept_80 +
/// guarded + merge_85 + unchanged + other_rewrite` and `auto +
/// needs_input` both sum to the same total). Like
/// [`DefOverrideCoverage::promotes_to_merge_85`], **`merge_85` (and, by
/// extension, `auto`) is a ceiling, not a live inbox count**:
/// `redecide_for_clean_merge` only requires the finding to already offer
/// `Merge` as an alternative, never checking the two real preconditions a
/// live promotion also needs beyond the confidence threshold — no stored
/// decision yet, and `Settings::suggest_merge_when_clean` on — so a
/// profile with existing decisions or the setting off moves fewer findings
/// out of the inbox in practice than `auto` suggests. **Disclosed, not
/// fixed**: this tally only ever runs `redecide_for_clean_merge` — the
/// live inbox (`Session::findings`/`resolution`) also runs
/// `Session::redecide_identical_copies_at` first, which takes a
/// `SameAuthor`/`Unknown` `DefOverride` with byte-identical owner copies
/// to `Accept` 99 before this pass ever sees it. The `inbox` axis still
/// gets the right *aggregate* answer regardless (99 already clears the
/// default threshold of 80, same as this tally's own `Unchanged` rows at
/// their own real confidence), but "post-redecision" here is only ever
/// true of `redecide_for_clean_merge`'s own pass, not the full pipeline a
/// live session actually runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SuggestionOutcomeTally {
    /// `Redecision::Accept95`.
    pub accept_95: usize,
    /// `Redecision::Accept80`.
    pub accept_80: usize,
    /// `Redecision::Guarded`.
    pub guarded: usize,
    /// `Redecision::Merge85`.
    pub merge_85: usize,
    /// `Redecision::Unchanged`.
    pub unchanged: usize,
    /// `Redecision::OtherRewrite`.
    pub other_rewrite: usize,
    /// `InboxOutcome::Auto`.
    pub auto: usize,
    /// `InboxOutcome::NeedsInput`.
    pub needs_input: usize,
}

impl SuggestionOutcomeTally {
    fn record(&mut self, outcome: SuggestionOutcome) {
        match outcome.redecision {
            Redecision::Accept95 => self.accept_95 += 1,
            Redecision::Accept80 => self.accept_80 += 1,
            Redecision::Guarded => self.guarded += 1,
            Redecision::Merge85 => self.merge_85 += 1,
            Redecision::Unchanged => self.unchanged += 1,
            Redecision::OtherRewrite => self.other_rewrite += 1,
        }
        match outcome.inbox {
            InboxOutcome::Auto => self.auto += 1,
            InboxOutcome::NeedsInput => self.needs_input += 1,
        }
    }
}

/// Patch-collision coverage: how many contested patch collisions the
/// supported xpath grammar can (and can't) replay, and — for the
/// supported ones — whether the replay is order-independent (`agreeing`)
/// or a genuine conflict.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PatchCollisionCoverage {
    /// Every `Conflict::PatchCollision` at
    /// [`PatchCollisionSeverity::Contested`].
    pub contested_collisions: usize,
    /// Of those, how many planned to a real preview (`Complete` or
    /// `NeedsFieldInput` — never `CannotMerge`).
    pub supported: usize,
    /// The complement of `supported` — a computed `CannotMerge` preview
    /// or a planning failure alike (unchanged fold, preserved verbatim
    /// from before this table had a def-override sibling).
    pub unsupported: usize,
    /// Of `supported`, how many are order-independent
    /// ([`MergeState::Complete`]).
    pub agreeing: usize,
    /// Of `supported`, how many are a genuine conflict
    /// ([`MergeState::NeedsFieldInput`]).
    pub conflict: usize,
    /// `unsupported`'s own reasons, counted.
    pub unsupported_reasons: BTreeMap<String, usize>,
    /// The post-redecision suggestion outcome for every
    /// contested collision that reached a real preview (`supported` plus
    /// a computed `CannotMerge` — everything but a planning failure, which
    /// has no [`MergeState`] to redecide against). The structural guard never applies to a
    /// `PatchCollision`, so [`SuggestionOutcomeTally::guarded`] is always
    /// `0` here.
    pub suggestion_outcomes: SuggestionOutcomeTally,
}

/// Def-override coverage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DefOverrideCoverage {
    /// Every `Conflict::DefOverride` in the report. Always exactly
    /// `planning_failures` plus every finding that reached
    /// [`Self::preview`]/the guard tally below.
    pub total: usize,
    /// Histogram of owner counts (`Conflict::DefOverride.owners.len()`).
    pub by_owner_count: BTreeMap<usize, usize>,
    /// How many are mod-versus-mod (`overrides_vanilla == false`) rather
    /// than a mod overriding Core/a DLC.
    pub mod_versus_mod: usize,
    /// A finding whose own source couldn't be read at all — no preview
    /// was ever built, so neither [`Self::preview`] nor the guard tally
    /// below has an entry for it. Kept apart from
    /// [`PreviewStateTally::cannot_merge`] on purpose: that field means a
    /// preview *computed* to `CannotMerge` (still guard-evaluated); this
    /// one means no preview exists to evaluate anything against.
    pub planning_failures: usize,
    /// `planning_failures`'s own reasons, counted.
    pub planning_failure_reasons: BTreeMap<String, usize>,
    /// How many the structural guard would fire for, broken down by
    /// which field triggered it first. Measurement only — see
    /// [`rim_merge::diff::structural_change`]'s own doc comment; nothing
    /// here reclassifies anything.
    pub guard_fires: BTreeMap<StructuralField, usize>,
    /// How many the guard was evaluated for and genuinely does not fire —
    /// **not** the complement of `guard_fires`'s own total: a finding
    /// whose owner data couldn't be re-read for the check
    /// ([`Self::guard_reread_failed`]) is neither a fire nor a
    /// does-not-fire, and a `planning_failures` finding was never
    /// evaluated at all. `guard_fires`'s own sum, plus this, plus
    /// `guard_reread_failed`, plus `planning_failures`, equals `total`.
    pub guard_does_not_fire: usize,
    /// A finding whose preview computed successfully, but re-reading its
    /// owners' raw/resolved data for the guard check itself failed (a
    /// stale scan between the two reads, or — vanishingly unlikely in
    /// practice — [`rim_merge::diff::structural_change`]'s own
    /// `BaseNotAnOwner`). Kept apart from `guard_does_not_fire` so this
    /// measurement never silently counts "couldn't check" as "checked and
    /// clean".
    pub guard_reread_failed: usize,
    /// This def override's merge preview, bucketed — one entry per
    /// finding that isn't a `planning_failures` one.
    pub preview: PreviewStateTally,
    /// How many currently promote to `Action::Merge` at confidence 85 —
    /// the finding's real ledger suggestion, run through
    /// [`rim_resolve::domain::redecide_for_clean_merge`] against this
    /// preview's own state.
    ///
    /// **Not** inferred from `Complete { op_count > 0 }` alone:
    /// `redecide_for_clean_merge` only promotes when the ledger's own
    /// suggestion already offers `Merge` as an alternative (its own
    /// `offers_merge` gate) — a def override the confidence table
    /// already resolves with a stronger signal never offers `Merge` at
    /// all, however clean its preview is.
    ///
    /// **This count is a ceiling, not a live count of what the inbox
    /// would actually auto-apply.** A real promotion also needs the
    /// finding to carry no stored decision yet
    /// (`Session::redecide_clean_merge_at`'s own `entry.decision.is_none()`
    /// gate), `Settings::suggest_merge_when_clean` to be on, and 85 to
    /// meet `Settings::threshold` — none of which this tally checks. A
    /// profile with existing decisions on some of these findings, or a
    /// raised threshold, promotes fewer than this number in practice.
    ///
    /// **Structurally always `0`**: `redecide_for_clean_merge`'s
    /// `Complete { op_count > 0 }` arm never promotes a `DefOverride` to
    /// `Action::Merge` — only a `PatchCollision` (which has no field of this
    /// name; see [`PatchCollisionCoverage::suggestion_outcomes`]'s own
    /// `merge_85` instead) can. Kept, not removed, so a caller reading it
    /// directly (`apps/cli`'s own JSON/table output) keeps seeing a real,
    /// meaningful number — zero — rather than the field vanishing out
    /// from under it.
    pub promotes_to_merge_85: usize,
    /// The post-redecision suggestion outcome for every def
    /// override that reached a real preview (everything but
    /// `planning_failures`). `suggestion_outcomes.merge_85` is the same
    /// count as [`Self::promotes_to_merge_85`] above (both are computed
    /// from the identical redecision — see `classify_suggestion_outcome`);
    /// kept as two fields rather than one, since existing callers already
    /// read `promotes_to_merge_85` directly.
    pub suggestion_outcomes: SuggestionOutcomeTally,
}

/// The whole coverage result: both tables, computed together against
/// [`Session::selected`]'s order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeCoverageReport {
    /// The patch-collision table.
    pub patch_collisions: PatchCollisionCoverage,
    /// The def-override table.
    pub def_overrides: DefOverrideCoverage,
}

/// One contested `Conflict::PatchCollision`, pre-parsed into the pieces
/// [`MergeCoverage::patch_collision_coverage`] needs — the finding key
/// itself and the `DefKey` it's built from, mirroring
/// [`DefOverrideFinding`]'s own shape, so no caller destructures the
/// `DefKey` back out of `key` behind an `unreachable!` (the pattern
/// `DefOverrideFinding`'s own doc comment says this crate avoids).
struct PatchCollisionFinding {
    key: FindingKey,
    def_key: DefKey,
}

/// Every contested `Conflict::PatchCollision`, as a [`PatchCollisionFinding`].
fn contested_patch_collision_findings(session: &Session) -> Vec<PatchCollisionFinding> {
    session
        .report()
        .conflicts
        .iter()
        .filter_map(|conflict| match conflict {
            Conflict::PatchCollision(collision)
                if collision.severity == PatchCollisionSeverity::Contested =>
            {
                let def_key = DefKey {
                    def_type: collision.def_type.clone(),
                    def_name: collision.def_name.clone(),
                };
                Some(PatchCollisionFinding {
                    key: FindingKey::PatchCollision {
                        key: def_key.clone(),
                        selector: collision.selector,
                        sub_path: collision.sub_path.clone(),
                        mods: collision
                            .mods
                            .iter()
                            .map(|entry| entry.mod_id.clone())
                            .collect(),
                    },
                    def_key,
                })
            }
            _ => None,
        })
        .collect()
}

/// One `Conflict::DefOverride`, pre-parsed into the pieces
/// [`MergeCoverage::def_override_coverage`] needs: the finding key
/// itself, the `DefKey` it's built from (avoiding an `unreachable!`
/// destructure back out of `key` at every call site), and the two facts
/// [`DefOverrideCoverage::by_owner_count`]/`mod_versus_mod` tally.
struct DefOverrideFinding {
    key: FindingKey,
    def_key: DefKey,
    owner_count: usize,
    overrides_vanilla: bool,
}

/// Every `Conflict::DefOverride` in the report, as a [`DefOverrideFinding`].
fn def_override_findings(session: &Session) -> Vec<DefOverrideFinding> {
    session
        .report()
        .conflicts
        .iter()
        .filter_map(|conflict| match conflict {
            Conflict::DefOverride(over) => {
                let def_key = DefKey {
                    def_type: over.def_type.clone(),
                    def_name: over.def_name.clone(),
                };
                Some(DefOverrideFinding {
                    key: FindingKey::DefOverride {
                        key: def_key.clone(),
                        owners: over.owners.iter().cloned().collect(),
                    },
                    def_key,
                    owner_count: over.owners.len(),
                    overrides_vanilla: over.overrides_vanilla,
                })
            }
            _ => None,
        })
        .collect()
}

/// Runs `suggestion` — a finding's real ledger suggestion — through
/// [`rim_resolve::domain::redecide_for_clean_merge`] against `preview`'s
/// state and classifies the result into a [`SuggestionOutcome`].
/// `Redecision::Guarded` is checked
/// directly off `preview`, *before* `redecide_for_clean_merge` is even
/// called (see that variant's own doc comment for why); every other
/// bucket is attributed from the **delta** between `suggestion` and the
/// redecided result, never from the redecided result's shape alone —
/// matching on `(action, confidence)` with no equality check first would
/// mis-bucket every pass-through suggestion whose own, unrelated shape
/// happens to coincide with one of the redecision's own output shapes.
///
/// The redecision never turns a zero-op preview into `Merge` — a zero-op
/// preview redecides to `Accept` at 95/80 — but `redecide_for_clean_merge`
/// still requires the `assumed_mod_setting_defaults` argument, taken from
/// [`MergePreview::assumed_mod_setting_defaults`] (shared with
/// `Session::redecide_clean_merge_at` rather than each site re-deriving it
/// from `rim_merge::plan::Caveat::ModSettingDefault` independently).
fn classify_suggestion_outcome(
    suggestion: Suggestion,
    def_key: &DefKey,
    preview: &MergePreview,
    finding_kind: MergeFindingKind,
    threshold: Confidence,
) -> SuggestionOutcome {
    let guard_field = preview.structural_guard_field();
    let redecided = rim_resolve::domain::redecide_for_clean_merge(
        suggestion.clone(),
        def_key,
        &preview.state,
        preview.assumed_mod_setting_defaults(),
        guard_field.as_deref(),
        finding_kind,
    );
    let redecision = if guard_field.is_some() {
        Redecision::Guarded
    } else if redecided == suggestion {
        Redecision::Unchanged
    } else {
        match (&redecided.action, redecided.confidence.percent()) {
            (Action::Accept, 95) => Redecision::Accept95,
            (Action::Accept, 80) => Redecision::Accept80,
            (Action::Merge { .. }, 85) => Redecision::Merge85,
            _ => Redecision::OtherRewrite,
        }
    };
    let inbox = if redecided.confidence.meets(threshold) {
        InboxOutcome::Auto
    } else {
        InboxOutcome::NeedsInput
    };
    SuggestionOutcome { redecision, inbox }
}

/// Builds both coverage tables.
pub struct MergeCoverage<Reader> {
    planner: PlanMerge<Reader>,
}

impl<Reader: DefSourceReader> MergeCoverage<Reader> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(reader: Reader) -> Self {
        Self {
            planner: PlanMerge::new(reader),
        }
    }

    /// Computes both tables against [`Session::selected`]'s order.
    #[must_use]
    pub fn execute(&self, session: &mut Session) -> MergeCoverageReport {
        // Read once: neither table's own loop mutates `Settings`, so
        // re-reading per finding would only ever re-derive the identical
        // value (the same "computed once, not per finding" reasoning
        // `def_override_coverage`'s own `order` already documents below).
        let threshold = session.settings().threshold;
        MergeCoverageReport {
            patch_collisions: self.patch_collision_coverage(session, threshold),
            def_overrides: self.def_override_coverage(session, threshold),
        }
    }

    fn patch_collision_coverage(
        &self,
        session: &mut Session,
        threshold: Confidence,
    ) -> PatchCollisionCoverage {
        let findings = contested_patch_collision_findings(session);
        let source = session.selected();
        // Same technique `def_override_coverage` uses below, for the same
        // reason: `session.ledger` mutably borrows `session`, which the
        // loop also needs for `PlanMerge::execute`.
        let suggestions: BTreeMap<FindingKey, Suggestion> = session
            .ledger(source)
            .entries
            .iter()
            .filter(|entry| matches!(entry.key, FindingKey::PatchCollision { .. }))
            .map(|entry| (entry.key.clone(), entry.suggestion.clone()))
            .collect();

        let mut coverage = PatchCollisionCoverage {
            contested_collisions: findings.len(),
            ..PatchCollisionCoverage::default()
        };
        for finding in &findings {
            match self.planner.execute(session, &finding.key) {
                Ok(preview) => {
                    match &preview.state {
                        MergeState::CannotMerge { reason } => {
                            coverage.unsupported += 1;
                            *coverage
                                .unsupported_reasons
                                .entry(reason.clone())
                                .or_insert(0) += 1;
                        }
                        // `preview.state` already tells `Complete` (every
                        // field auto-resolved — order-independent, needs no
                        // decision) from `NeedsFieldInput` (a genuine,
                        // unresolved conflict) exactly, including for a
                        // keyed-map collision's own per-key expansion.
                        MergeState::Complete { .. } => {
                            coverage.supported += 1;
                            coverage.agreeing += 1;
                        }
                        MergeState::NeedsFieldInput { .. } => {
                            coverage.supported += 1;
                            coverage.conflict += 1;
                        }
                    }
                    // Every finding that reached a real preview
                    // (`supported` and a computed `CannotMerge` alike) also
                    // gets its post-redecision suggestion outcome tallied —
                    // only a planning failure (the `Err` arm below) has no
                    // `MergeState` to redecide against at all.
                    if let Some(suggestion) = suggestions.get(&finding.key) {
                        let outcome = classify_suggestion_outcome(
                            suggestion.clone(),
                            &finding.def_key,
                            preview,
                            MergeFindingKind::PatchCollision,
                            threshold,
                        );
                        coverage.suggestion_outcomes.record(outcome);
                    }
                }
                Err(error) => {
                    coverage.unsupported += 1;
                    *coverage
                        .unsupported_reasons
                        .entry(format!("planning failed: {error}"))
                        .or_insert(0) += 1;
                }
            }
        }
        coverage
    }

    fn def_override_coverage(
        &self,
        session: &mut Session,
        threshold: Confidence,
    ) -> DefOverrideCoverage {
        let findings = def_override_findings(session);
        let source = session.selected();
        // Computed once, not per finding: neither the selected order nor
        // which mods are active can change while this loop runs (nothing
        // it does mutates `Session`), so re-reading either per finding
        // would only ever re-derive the identical value.
        let order = session.orders().get(source).clone();
        // Read once, before the loop: `session.ledger` mutably borrows
        // `session`, which the loop below also needs (for `PlanMerge::execute`/
        // `build_owner_versions`) — collecting the handful of fields this
        // tally needs into an owned map up front avoids re-borrowing
        // `session` from inside the loop just to look one finding back up,
        // and turns what would otherwise be a linear scan of every ledger
        // entry per def override into one map build plus a lookup per
        // finding.
        let suggestions: BTreeMap<FindingKey, Suggestion> = session
            .ledger(source)
            .entries
            .iter()
            .filter(|entry| matches!(entry.key, FindingKey::DefOverride { .. }))
            .map(|entry| (entry.key.clone(), entry.suggestion.clone()))
            .collect();

        let mut coverage = DefOverrideCoverage {
            total: findings.len(),
            ..DefOverrideCoverage::default()
        };

        for finding in &findings {
            *coverage
                .by_owner_count
                .entry(finding.owner_count)
                .or_insert(0) += 1;
            if !finding.overrides_vanilla {
                coverage.mod_versus_mod += 1;
            }

            let preview = match self.planner.execute(session, &finding.key) {
                Ok(preview) => preview.clone(),
                Err(error) => {
                    coverage.planning_failures += 1;
                    *coverage
                        .planning_failure_reasons
                        .entry(error.to_string())
                        .or_insert(0) += 1;
                    continue;
                }
            };
            coverage.preview.record(&preview.state);

            self.record_guard_outcome(session, &order, &finding.def_key, &preview, &mut coverage);

            if let Some(suggestion) = suggestions.get(&finding.key) {
                let outcome = classify_suggestion_outcome(
                    suggestion.clone(),
                    &finding.def_key,
                    &preview,
                    MergeFindingKind::DefOverride,
                    threshold,
                );
                coverage.suggestion_outcomes.record(outcome);
                if outcome.redecision == Redecision::Merge85 {
                    coverage.promotes_to_merge_85 += 1;
                }
            }
        }

        coverage
    }

    /// Re-reads `preview.owners`' own raw/resolved data (never carried by
    /// [`super::merge_workspace::MergePreview`] itself — only its
    /// [`rim_merge::diff::ThreeWayDiff`] is) and runs
    /// [`structural_change`] over it. A re-read failure (the scan is
    /// stale relative to disk, the same class of failure `PlanMerge`
    /// itself could equally have hit a moment earlier) — or, in
    /// principle, [`structural_change`]'s own `BaseNotAnOwner` — counts
    /// toward [`DefOverrideCoverage::guard_reread_failed`], never folded
    /// into "does not fire": this measurement must never present "could
    /// not check" as "checked and clean".
    fn record_guard_outcome(
        &self,
        session: &Session,
        order: &LoadOrder,
        def_key: &DefKey,
        preview: &MergePreview,
        coverage: &mut DefOverrideCoverage,
    ) {
        let owner_versions = plan_merge::build_owner_versions(
            self.planner.reader(),
            session,
            order,
            &def_key.def_type,
            &def_key.def_name,
            &preview.owners,
        );
        match owner_versions {
            Ok(owner_versions) => match structural_change(&preview.diff, &owner_versions) {
                Ok(Some(change)) => {
                    *coverage.guard_fires.entry(change.field).or_insert(0) += 1;
                }
                Ok(None) => coverage.guard_does_not_fire += 1,
                Err(_) => coverage.guard_reread_failed += 1,
            },
            Err(_) => coverage.guard_reread_failed += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        bionic_heart_fixture, bionic_heart_fixture_with_conflict, session_with_sources,
        session_with_sources_and_mods, two_clean_overrides_fixture,
    };

    #[test]
    fn patch_collision_table_is_empty_with_no_contested_collisions() {
        let fixture = bionic_heart_fixture();
        let mut session = session_with_sources(fixture.sources, fixture.report);
        let use_case = MergeCoverage::new(fixture.reader);

        let report = use_case.execute(&mut session);

        assert_eq!(report.patch_collisions.contested_collisions, 0);
    }

    /// [`bionic_heart_fixture`]'s two-owner, no-stored-choices shape would
    /// otherwise preview `MergeState::Complete { op_count: 0 }` — the zero-op
    /// bucket — but it is also a real structural-guard `ParentName` trigger:
    /// BIONICS's own owner declares `ParentName="addedPartExampleSynth"` where
    /// Core's declares `ParentName="AddedBodyPartBase"` — two different
    /// chains that happen to resolve to the same effective fields two hops
    /// up. The predicate fires anyway, with no template-equivalence check —
    /// exactly the scenario this fixture is built around, exercised through
    /// the coverage tally rather than only `rim-merge`'s own unit tests.
    #[test]
    fn a_two_owner_guarded_override_is_tallied_as_needs_field_input_not_zero_op() {
        let fixture = bionic_heart_fixture_with_conflict();
        let mut session = session_with_sources(fixture.sources, fixture.report);
        let use_case = MergeCoverage::new(fixture.reader);

        let report = use_case.execute(&mut session);

        let overrides = &report.def_overrides;
        assert_eq!(overrides.total, 1);
        assert_eq!(overrides.by_owner_count.get(&2), Some(&1));
        assert_eq!(overrides.mod_versus_mod, 1);
        assert_eq!(overrides.planning_failures, 0);
        // `PlanMerge` applies the structural guard itself, so this def's
        // preview — every field would otherwise auto-resolve to a no-op, but
        // BIONICS's own `ParentName` differs from Core's — is counted in
        // `needs_field_input`, not `complete_zero_ops`. The guard tally itself
        // is computed independently, by re-reading the owners' raw/resolved
        // data (see `record_guard_outcome`'s own doc comment): it never
        // depends on `PlanMerge` itself applying the guard.
        assert_eq!(overrides.preview.complete_zero_ops, 0, "{overrides:#?}");
        assert_eq!(overrides.preview.needs_field_input, 1, "{overrides:#?}");
        assert_eq!(overrides.guard_does_not_fire, 0, "{overrides:#?}");
        assert_eq!(overrides.guard_reread_failed, 0, "{overrides:#?}");
        assert_eq!(
            overrides.guard_fires.get(&StructuralField::ParentName),
            Some(&1),
            "{overrides:#?}"
        );
        // Never promoted, for two independent reasons: the zero-op-preview
        // rule never applies to a `NeedsFieldInput` preview, and
        // `redecide_for_clean_merge` only ever promotes from
        // `MergeState::Complete` — this preview is neither.
        assert_eq!(overrides.promotes_to_merge_85, 0);
        // The guard fires, so the post-redecision outcome is
        // `Guarded` — checked directly off the preview, ahead of whatever
        // the redecided suggestion's own shape ends up being.
        assert_eq!(overrides.suggestion_outcomes.guarded, 1, "{overrides:#?}");
    }

    /// The `Accept95` bucket: [`bionic_heart_fixture_flat_with_conflict`]'s
    /// two-owner, zero-op preview (no `ParentName` difference, unlike the
    /// guarded fixture above) redecides to `Accept` at confidence 95 — no
    /// `Caveat::ModSettingDefault` was assumed, so `accept_80` stays `0`.
    #[test]
    fn a_zero_op_override_lands_on_accept_95() {
        let fixture = crate::test_support::bionic_heart_fixture_flat_with_conflict();
        let mut session = session_with_sources(fixture.sources, fixture.report);
        let use_case = MergeCoverage::new(fixture.reader);

        let report = use_case.execute(&mut session);

        let overrides = &report.def_overrides;
        assert_eq!(overrides.preview.complete_zero_ops, 1, "{overrides:#?}");
        assert_eq!(overrides.suggestion_outcomes.accept_95, 1, "{overrides:#?}");
        assert_eq!(overrides.suggestion_outcomes.accept_80, 0, "{overrides:#?}");
        assert_eq!(
            overrides.suggestion_outcomes.auto, 1,
            "95 clears the default threshold of 80: {overrides:#?}"
        );
    }

    /// [`two_clean_overrides_fixture`] carries two independent,
    /// genuinely-clean three-owner overrides (`Complete { op_count: 1 }`)
    /// plus a third, three-owner *conflicting* one (`NeedsFieldInput`) —
    /// exercising every `PreviewStateTally` bucket `two_clean_overrides_fixture`
    /// itself can produce.
    ///
    /// **`promotes_to_merge_85` is `0`.**
    /// `redecide_for_clean_merge`'s `Complete { op_count > 0 }` arm never
    /// promotes a `DefOverride` to `Action::Merge` outright — it only leads
    /// with `Merge` in the alternatives, the same `NeedsFieldInput` shape the
    /// conflicting row already gets — so all three rows here land on
    /// `Redecision::OtherRewrite`, none on `Merge85`.
    #[test]
    fn a_mixed_def_override_fixture_no_longer_promotes_any_row_to_merge_85() {
        let fixture = two_clean_overrides_fixture();
        let mut session =
            session_with_sources_and_mods(fixture.sources, fixture.report, &fixture.active_mods);
        let use_case = MergeCoverage::new(fixture.reader);

        let report = use_case.execute(&mut session);

        let overrides = &report.def_overrides;
        assert_eq!(overrides.total, 3);
        assert_eq!(overrides.by_owner_count.get(&3), Some(&3));
        assert_eq!(overrides.planning_failures, 0);
        assert_eq!(overrides.preview.complete_with_ops, 2, "{overrides:#?}");
        assert_eq!(overrides.preview.needs_field_input, 1, "{overrides:#?}");
        assert_eq!(overrides.preview.complete_zero_ops, 0);
        assert_eq!(overrides.preview.cannot_merge, 0);
        assert_eq!(
            overrides.promotes_to_merge_85, 0,
            "a DefOverride is never promoted outright any more: {overrides:#?}"
        );
        assert_eq!(overrides.suggestion_outcomes.merge_85, 0, "{overrides:#?}");
        // All three rows (the two clean overrides and the conflicting one
        // alike) get a real rewrite (`lead_with_merge` reorders their
        // alternatives) that isn't one of the three named outcomes, so
        // all three are `OtherRewrite`.
        assert_eq!(
            overrides.suggestion_outcomes.other_rewrite, 3,
            "{overrides:#?}"
        );
    }

    /// A finding whose planning genuinely fails (a stale/missing source)
    /// counts toward `total` and `planning_failures` — never folded into
    /// `preview.cannot_merge` (there is no preview at all to have
    /// computed that), and never guard-evaluated at all: this finding
    /// must not appear in `guard_fires`, `guard_does_not_fire`, or
    /// `guard_reread_failed`.
    #[test]
    fn a_planning_failure_is_counted_separately_from_a_computed_cannot_merge() {
        let fixture = bionic_heart_fixture_with_conflict();
        // No sources at all for this report's own def override — every
        // lookup PlanMerge needs will miss.
        let mut session = session_with_sources(Default::default(), fixture.report);
        let use_case = MergeCoverage::new(fixture.reader);

        let report = use_case.execute(&mut session);

        let overrides = &report.def_overrides;
        assert_eq!(overrides.total, 1);
        assert_eq!(overrides.planning_failures, 1);
        assert!(
            !overrides.planning_failure_reasons.is_empty(),
            "{:?}",
            overrides.planning_failure_reasons
        );
        assert_eq!(
            overrides.preview.cannot_merge, 0,
            "a planning failure has no preview and must never be counted here"
        );
        assert_eq!(overrides.guard_does_not_fire, 0);
        assert_eq!(overrides.guard_reread_failed, 0);
        assert_eq!(overrides.guard_fires.values().sum::<usize>(), 0);
        assert_eq!(overrides.promotes_to_merge_85, 0);
        assert_eq!(
            overrides.suggestion_outcomes,
            SuggestionOutcomeTally::default(),
            "a planning failure has no preview to redecide against and must never be counted here: {overrides:#?}"
        );
    }

    /// The patch-collision side: [`whole_def_patch_collision_fixture`]'s
    /// single-contributor, whole-def `Add` collision previews `Complete
    /// { op_count: 0 }` (the one contributor's own candidate already
    /// equals the real replay's outcome) — the zero-op arm redecides
    /// it to `Accept`, never leaving it needs-input.
    #[test]
    fn a_clean_patch_collision_lands_on_an_accept_bucket() {
        let fixture = crate::test_support::whole_def_patch_collision_fixture();
        let mut session =
            session_with_sources_and_mods(fixture.sources, fixture.report, &["core.mod", "a.mod"]);
        let use_case = MergeCoverage::new(fixture.reader);

        let report = use_case.execute(&mut session);

        let patch = &report.patch_collisions;
        assert_eq!(patch.contested_collisions, 1, "{patch:#?}");
        assert_eq!(patch.agreeing, 1, "{patch:#?}");
        assert_eq!(patch.conflict, 0, "{patch:#?}");
        // The structural guard never applies to a `PatchCollision`, so `guarded` stays `0`
        // regardless of which `Accept` confidence this lands on.
        assert_eq!(patch.suggestion_outcomes.guarded, 0, "{patch:#?}");
        assert_eq!(patch.suggestion_outcomes.needs_input, 0, "{patch:#?}");
        assert_eq!(
            patch.suggestion_outcomes.accept_95 + patch.suggestion_outcomes.accept_80,
            1,
            "an agreeing collision must redecide to Accept at 95 or 80, never stay needs-input: {patch:#?}"
        );
    }
}
