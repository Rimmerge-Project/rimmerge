//! The merge context a preview is planned under, and the errors planning can raise.

use std::collections::BTreeMap;

use rim_merge::inherit::InheritError;
use rim_merge::tree::FieldPath;
use rim_resolve::domain::{FindingKey, MergeChoice, PatchScope};

use crate::merge_workspace::PreviewSlot;
use crate::ports::DefSourceError;
use crate::use_cases::def_sources::DefSourceLookupError;

/// Everything a preview depends on besides the finding itself: which slot
/// it's computed for and cached under, that slot's own stored per-field
/// choices for this one finding, and — for a patch's own context — the
/// scope [`PlanMerge::plan_def_override`](crate::use_cases::plan_merge::PlanMerge::plan_def_override)/[`PlanMerge::plan_patch_collision`](crate::use_cases::plan_merge::PlanMerge::plan_patch_collision)
/// restrict diff participants/contributions to
/// `scope: None` (the profile's
/// own context, built by [`PlanMerge::execute`](crate::use_cases::plan_merge::PlanMerge::execute)/[`Session::merge_context`](crate::Session::merge_context))
/// keeps every owner, exactly as before this rule existed.
///
/// Owned (`choices` cloned — a handful of entries at most) rather than
/// borrowed, so building one never holds a borrow of [`Session`](crate::Session) across
/// the mutable calls [`PlanMerge::execute_in`](crate::use_cases::plan_merge::PlanMerge::execute_in) makes with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeContext {
    /// Which preview-cache bucket this context plans and caches into.
    pub slot: PreviewSlot,
    /// This finding's stored per-field choices under `slot`'s own decision
    /// set (the profile's, or a patch project's).
    pub choices: BTreeMap<FieldPath, MergeChoice>,
    /// The patch scope owning this context, if any — restricts diff
    /// participants/contributions to the patch's scope. `None` for the
    /// profile's own context.
    pub scope: Option<PatchScope>,
}

/// Everything that can go wrong building a merge preview.
///
/// `rim_merge::inherit::InheritError` (a missing `ParentName` template, or
/// an inheritance cycle) is kept as its own variant rather than folded
/// into a generic "source" error, since it's a resolver-level data
/// problem, not an I/O/staleness one — reachable because a mod's template
/// chain can always be broken, and it needs *some* reported outcome.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanMergeError {
    /// `key` isn't a def-override or patch-collision finding — nothing
    /// else can be merged.
    #[error("{0:?} is not a mergeable finding")]
    UnsupportedFinding(FindingKey),
    /// The finding's def/template/patch text couldn't be read back — the
    /// scan is stale relative to what's on disk now, or the file itself
    /// couldn't be read.
    #[error(transparent)]
    Source(#[from] DefSourceError),
    /// The def's `ParentName` chain couldn't be resolved.
    #[error("resolving inheritance: {0}")]
    Inherit(#[from] InheritError),
    /// The read-back XML text failed to parse. Kept as a plain `String`
    /// (rather than `#[from] rim_merge::error::MergeError`) so this type
    /// can still derive `PartialEq`/`Eq`/`Clone` like every other use-case
    /// error in this crate — `rim_merge::error::MergeError` wraps
    /// `roxmltree::Error`, which implements neither.
    #[error("parsing merge XML: {0}")]
    Xml(String),
    /// The source index has no record of an owner/template the finding
    /// itself names — a stale or inconsistent scan.
    #[error("{0}")]
    MissingSource(String),
}

/// [`DefSourceLookupError`]'s three variants map onto three of
/// [`PlanMergeError`]'s own — a manual `From` (rather than `#[from]` on a
/// wrapping variant) so every existing `match`/`matches!` on
/// `PlanMergeError::Source(DefSourceError::Stale { .. })` and friends
/// keeps working unchanged: `def_sources`'s own functions raise the
/// shared, narrower error; this crosses back into the wider one at the
/// point they're called.
impl From<DefSourceLookupError> for PlanMergeError {
    fn from(error: DefSourceLookupError) -> Self {
        match error {
            DefSourceLookupError::Source(source) => Self::Source(source),
            DefSourceLookupError::Xml(message) => Self::Xml(message),
            DefSourceLookupError::MissingSource(message) => Self::MissingSource(message),
        }
    }
}
