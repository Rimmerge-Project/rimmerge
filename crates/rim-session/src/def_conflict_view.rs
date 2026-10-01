//! [`Session::def_conflict_view`](crate::Session::def_conflict_view): one field-by-field
//! projection for a def-shaped finding (`DefOverride`/`PatchCollision`/`DuplicateTemplateName`)
//! — who changed what, who wins and why, and what couldn't be evaluated.
//!
//! A pure join over two already-cached results — [`InspectDef`]'s
//! [`DefInspection`](crate::use_cases::DefInspection) (touchers, the effective def and its
//! provenance, completeness) and [`PlanMerge`]'s [`MergePreview`](crate::MergePreview)
//! (per-mod candidates,
//! the plan, the stored choices already folded in) — never a fresh replay
//! and never new IO: a caller must have already inspected and (for
//! `DefOverride`/`PatchCollision`) planned this key under the selected
//! order, or this returns an error naming which is missing.
//!
//! [`InspectDef`]: crate::use_cases::InspectDef
//! [`PlanMerge`]: crate::use_cases::PlanMerge

use rim_analyzer::domain::ModId;
use rim_merge::diff::Value;
use rim_merge::effective::Completeness;
use rim_merge::inherit::InheritError;
use rim_merge::tree::FieldPath;
use rim_resolve::domain::{DefRef, FindingKey, MergeChoice};

mod grouping;
mod rendering;
mod rows;

pub(crate) use rendering::{build, build_for_slot, build_with_plan_failure};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "def_conflict_view/def_conflict_view_tests.rs"]
mod tests;

/// Which of the three def-shaped finding kinds this view was built for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefConflictKind {
    /// [`FindingKey::DefOverride`].
    DefOverride,
    /// [`FindingKey::PatchCollision`], with its own `sub_path` parsed to a
    /// [`FieldPath`] when the grammar `rim_analyzer::extract::xpath_expr`
    /// supports allows it (`None` when it doesn't — the same case that
    /// already makes the cached [`MergePreview`](crate::MergePreview) `CannotMerge`).
    PatchCollision {
        /// The contested field, if the sub_path parsed.
        sub_path: Option<FieldPath>,
    },
    /// [`FindingKey::DuplicateTemplateName`] — touchers and problems only;
    /// there is no diff engine for two templates' own field sets (no
    /// `MergePreview` is ever built for this kind — see
    /// `crate::use_cases::PlanMerge::execute`), so [`DefConflictView::fields`]
    /// is always empty for this kind.
    DuplicateTemplateName,
}

/// Which part a [`Toucher`] plays. A mod that both owns and patches the
/// same def gets two rows, one per role — see [`Toucher`]'s own doc
/// comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToucherRole {
    /// Registers/owns the def or template.
    Owner,
    /// Patches it (foreign or self).
    Patcher,
}

/// One mod's own row in the touchers table. A mod that
/// both owns *and* patches the def appears twice, once per role — a
/// single row can't carry two different `op_count`s (an owner's is
/// always 0; a patcher's is its own top-level operation count), and role
/// is one of the fields a toucher carries, not something a row can hold
/// two of at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toucher {
    /// The mod.
    pub mod_id: ModId,
    /// Its position in the selected order.
    pub position: usize,
    /// Whether this is a Rimmerge-generated mod — shown, never hidden
    /// (it's what the game actually runs).
    pub is_generated: bool,
    /// Owner or patcher.
    pub role: ToucherRole,
    /// Top-level operations targeting the def — always 0 for
    /// [`ToucherRole::Owner`].
    pub op_count: usize,
}

/// How one [`FieldRow`] should be grouped and displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldRowKind {
    /// Two or more touchers set this path to different values.
    Conflict,
    /// Exactly one toucher sets this path (or several agree on one
    /// value) — the merged result keeps it with no contest.
    CleanMerge,
    /// One `li` item under a list path, contributed by one toucher; other
    /// entries under the same list (from other touchers) are their own
    /// rows and all end up in the final list together.
    ListEntry,
    /// One key of a [`rim_merge::tree::ContainerKind::KeyedMap`]
    /// container, contributed by one toucher (or several agreeing on
    /// one value — see [`FieldRow::agreed_by`]) — [`Self::ListEntry`]'s
    /// twin for a tag-keyed container instead of an `li` list
    ///
    /// Other keys under the same map are their own rows and all end up
    /// unioned in the final map together.
    MapEntry,
    /// No toucher sets this path — collapsed by default, count shown by
    /// the caller.
    Unchanged,
}

/// Who wins a [`FieldRow`] and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preference {
    /// The last toucher in the selected order wins — no decision or
    /// choice overrides it.
    LoadOrder {
        /// The winning mod.
        winner: ModId,
    },
    /// An [`Action::PreferWinner`](rim_resolve::domain::Action::PreferWinner) decision on this finding names the
    /// winner explicitly.
    Decision {
        /// The decided winner.
        winner: ModId,
    },
    /// A stored [`MergeChoice`] for this exact path overrides both of the
    /// above.
    MergeChoice {
        /// The stored choice.
        choice: MergeChoice,
    },
    /// Not contested — no preference applies.
    None,
}

/// One row of [`DefConflictView::fields`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRow {
    /// The field's address — a list entry is its own row, keyed by the
    /// item's own [`rim_merge::tree::ItemId`].
    pub path: FieldPath,
    /// How to group/display this row.
    pub kind: FieldRowKind,
    /// Every toucher that sets this path to a value differing from the
    /// base (empty for [`FieldRowKind::Unchanged`]), in the selected
    /// order.
    pub values: Vec<(ModId, Value)>,
    /// Other touchers that independently agree with this row's own
    /// value: either that they added the *exact same* `li` item under a
    /// colliding identity (the "list case" dedup), for a
    /// [`FieldRowKind::ListEntry`] row produced from
    /// [`EffectiveDef::provenance`](rim_merge::effective::EffectiveDef::provenance) directly (see
    /// `fold_duplicate_list_entries`), or that they agree on a keyed
    /// map's own key value, for a [`FieldRowKind::MapEntry`] row backed
    /// by a [`DiffClass::Agreeing`](rim_merge::diff::DiffClass::Agreeing)
    /// [`FieldDiff`](rim_merge::diff::FieldDiff) — load order,
    /// excluding whichever mod already appears in [`Self::values`].
    /// Empty for every other row: an ordinary (non-map) [`FieldDiff`](rim_merge::diff::FieldDiff)-
    /// backed row already lists every agreeing toucher in
    /// [`Self::values`] itself, since two owners setting one *named*
    /// field to the same value never collides at the path level the way
    /// a colliding `li` identity (or a shared map key) does.
    pub agreed_by: Vec<ModId>,
    /// Who/what the game runs today, from the effective def's own
    /// provenance — `None` when the field isn't in the (possibly
    /// partial, see [`DefConflictView::effective_completeness`]) resolved
    /// tree at all.
    pub in_game: Option<(ModId, Value)>,
    /// What a complete merge would produce, when one can be computed
    /// (the cached preview is [`MergeState::Complete`](rim_resolve::domain::MergeState::Complete)).
    /// `None` otherwise, even for
    /// a field that itself has an unambiguous auto result: an incomplete
    /// preview means *some* field is still unresolved, so the merged
    /// tree as a whole was never actually built.
    pub after_merge: Option<(ModId, Value)>,
    /// Who wins in the effective def today, and why — [`Preference::None`]
    /// for anything but a [`FieldRowKind::Conflict`] row.
    pub preference: Preference,
}

/// Something [`DefConflictView`] couldn't evaluate past a certain point —
/// named so the panel can say which op and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A contribution's own operation stopped the fold
    /// ([`Stopper::Replay`](rim_merge::effective::Stopper::Replay)) — every field before it in the fold's own
    /// order is still populated; this names exactly which op and why.
    UnsupportedOp {
        /// The mod that shipped the operation.
        mod_id: ModId,
        /// The operation's index across every active patcher's top-level
        /// operations, in load order (the same index
        /// [`rim_merge::effective::Provenance::Patch`] uses).
        op_index: usize,
        /// The operation's `Class`, when it could be recovered from the
        /// patcher's own op summaries — `"<unknown>"` when
        /// `representative_stopper_op` found nothing. Left as a sentinel
        /// `String` rather than `Option<String>`:
        /// `apps/desktop/src-tauri/src/dto/def_conflict.rs` mirrors this
        /// field as a plain `String` and its own tests assert against it
        /// directly.
        class: String,
        /// The operation's `<xpath>`, when available.
        xpath: Option<String>,
        /// Why the replay stopped.
        reason: String,
    },
    /// A `ParentName` chain named a template with no active owner.
    MissingTemplate {
        /// The def type the chain was walking.
        def_type: String,
        /// The missing template's `Name`.
        name: String,
    },
    /// A `ParentName` chain revisited a name it had already visited.
    Cycle {
        /// The repeated name, appended to the chain that led back to it.
        chain: Vec<String>,
    },
    /// A `DefOverride`'s own [`crate::use_cases::PlanMerge::execute_in`]
    /// hard-errored with something other than a structured
    /// [`InheritError`] this view can already name precisely
    /// (`MissingTemplate`/`Cycle` above). `InspectDef`'s own `Problem`s are derived only from
    /// the *winning* owner's own template chain (`DefInspection.effective`'s
    /// fold never looks at a losing owner's own chain at all), so a
    /// losing owner's own broken chain (most commonly
    /// [`crate::use_cases::PlanMergeError::MissingSource`], a plain
    /// string reason — the source index has no record of some
    /// owner/template a participant's own plan needed) would otherwise
    /// vanish silently: the view would come back with an empty `fields`
    /// entry for that owner's own contribution, no `problems` entry
    /// naming why, and a `Complete` pill claiming nothing is wrong. See
    /// [`problem_from_plan_failure`].
    PlanFailed {
        /// The swallowed [`crate::use_cases::PlanMergeError`]'s own
        /// `Display` text, verbatim.
        reason: String,
    },
}

/// Converts a `DefOverride`'s own swallowed
/// [`crate::use_cases::PlanMergeError`] (see [`Problem::PlanFailed`]'s own
/// doc comment for why the caller — `apps/desktop`'s `get_def_conflict_view`
/// command — must never simply drop it) into the [`Problem`] a view should
/// carry instead of hiding it: the two [`InheritError`] shapes this crate
/// already has a precise, structured [`Problem`] for reuse that one
/// (identical to how `problems_from_completeness` reports the *winning*
/// owner's own gap of the same shape); anything else — most commonly
/// [`crate::use_cases::PlanMergeError::MissingSource`], which is already
/// just a `String` reason with no owner/template to name structurally —
/// becomes [`Problem::PlanFailed`].
#[must_use]
pub fn problem_from_plan_failure(error: &crate::use_cases::PlanMergeError) -> Problem {
    use crate::use_cases::PlanMergeError;
    match error {
        PlanMergeError::Inherit(InheritError::MissingParent { def_type, name }) => {
            Problem::MissingTemplate {
                def_type: def_type.clone(),
                name: name.clone(),
            }
        }
        PlanMergeError::Inherit(InheritError::Cycle { chain }) => Problem::Cycle {
            chain: chain.clone(),
        },
        other => Problem::PlanFailed {
            reason: other.to_string(),
        },
    }
}

/// One `EdgeKind::PatchSelectsInjectedNode` relation naming this def —
/// rather than a fresh, standalone inbox finding for every such edge, the
/// relation surfaces at the point the user is already looking at this
/// def. One toucher's patch op
/// selects an XML node path (`subject`) that another toucher's own patch
/// injects, with no active mod writing that path inline and no single
/// distinct injector to name confidently (see
/// [`rim_analyzer::domain::EdgeKind::PatchSelectsInjectedNode`]'s own
/// doc comment for the full "why this is only advisory" story) — evidence
/// the two touchers interact, never an ordering requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectedNodeRelation {
    /// The toucher whose patch op selects [`Self::subject`].
    pub selector: ModId,
    /// The toucher whose own patch injects [`Self::subject`].
    pub injector: ModId,
    /// The selected node path, verbatim from `Edge.subject` — may name a
    /// sub-path beneath this def, not always the def's own root.
    pub subject: String,
}

/// One def-shaped finding's field-by-field conflict view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefConflictView {
    /// The def or template this view is about.
    pub def_ref: DefRef,
    /// Which finding kind produced it.
    pub kind: DefConflictKind,
    /// Every owner and patcher, selected order.
    pub touchers: Vec<Toucher>,
    /// Every field any toucher sets, deterministically ordered — conflicts
    /// first, then by path.
    pub fields: Vec<FieldRow>,
    /// Everything that stopped evaluation partway through.
    pub problems: Vec<Problem>,
    /// Whether the effective def's own fold reached every stage.
    pub effective_completeness: Completeness,
    /// Every `PatchSelectsInjectedNode` edge naming this def, `(selector,
    /// injector, subject)`-ordered for a deterministic, byte-identical
    /// view. Read straight off [`Session::report`](crate::Session::report)'s already-computed
    /// edges — no new IO, no replay.
    pub injected_node_relations: Vec<InjectedNodeRelation>,
}

/// Filters and pages [`DefConflictView::fields`] for a caller (the desktop
/// `get_def_conflict_view` command) that wants only
/// what changed and a bounded page — the same `offset`/`limit`-clamped
/// shape [`crate::merge_workspace::MergeFieldFilter`]/
/// [`crate::effective_fields::EffectiveFieldFilter`] already apply to
/// their own field lists. Lives here (not the DTO layer) per
/// `apps/desktop/CLAUDE.md`'s "filtering, paging, and aggregation live in
/// rim-session" — the same rule [`crate::effective_fields::page`] follows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldRowFilter {
    /// Hide [`FieldRowKind::Unchanged`] rows.
    pub only_changed: bool,
    /// How many matching rows to skip before collecting the page.
    pub offset: usize,
    /// How many rows to return, capped at [`crate::MAX_PAGE_SIZE`].
    pub limit: usize,
}

/// One page of [`page`]'s field-row list: how many rows matched the
/// filter (before paging), and the page of rows itself, in
/// [`DefConflictView::fields`]'s own order (conflicts first, then by
/// path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRowPage<'a> {
    /// Rows matching the filter, before paging.
    pub total: usize,
    /// The requested page of rows.
    pub items: Vec<&'a FieldRow>,
}

/// Filters and pages `view.fields` — pure computation over an
/// already-built [`DefConflictView`], the one place a caller pages this
/// list instead of recomputing the same filter/skip/take by hand.
#[must_use]
pub fn page<'a>(view: &'a DefConflictView, filter: &FieldRowFilter) -> FieldRowPage<'a> {
    let matching: Vec<&FieldRow> = view
        .fields
        .iter()
        .filter(|row| !filter.only_changed || row.kind != FieldRowKind::Unchanged)
        .collect();
    let total = matching.len();
    let limit = filter.limit.min(crate::MAX_PAGE_SIZE);
    let items = matching
        .into_iter()
        .skip(filter.offset)
        .take(limit)
        .collect();
    FieldRowPage { total, items }
}

/// Everything that can go wrong building a [`DefConflictView`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DefConflictViewError {
    /// `key` doesn't name a def or template
    /// ([`FindingKey::def_ref`] returns `None`) — nothing to show a
    /// field-by-field view of.
    #[error("{0:?} is not a def-shaped finding")]
    UnsupportedFinding(FindingKey),
    /// No cached [`DefInspection`](crate::use_cases::DefInspection) exists for this def under the selected
    /// order yet — call [`crate::use_cases::InspectDef::execute`] first.
    #[error("{0} has not been inspected under the selected order yet")]
    NotInspected(DefRef),
    /// No cached [`MergePreview`](crate::MergePreview) exists for this `PatchCollision` finding
    /// under the selected order yet — call
    /// [`crate::use_cases::PlanMerge::execute`] first. Never returned for
    /// [`DefConflictKind::DuplicateTemplateName`] (no preview to begin
    /// with) or [`DefConflictKind::DefOverride`] (a missing preview there
    /// degrades gracefully instead —
    /// see `fields_from_provenance_only`'s own doc comment for why a
    /// `DefOverride`'s own `PlanMerge::execute` can genuinely cache
    /// nothing at all, unlike a `PatchCollision`'s).
    #[error("{0:?} has not been planned under the selected order yet")]
    NotPlanned(FindingKey),
}
