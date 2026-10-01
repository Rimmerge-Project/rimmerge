//! Domain re-exports: the public shape of `rim-resolve`'s pure model.
//!
//! No filesystem, no XML, no Tauri — everything here is plain data and
//! the functions that validate or combine it.

mod assignment;
mod decision;
mod def_ref;
mod finding;
mod ledger;
mod merge;
mod order;
mod patch;
mod rationale;
mod resolution;
mod rule;
mod tag;

pub use assignment::{
    AssignmentId, AssignmentIdParseError, AssignmentProject, AssignmentRow, AssignmentRowError,
    AssignmentSchema, Cardinality, Coverage, CoverageRow, ENUM_MAX_DISTINCT, ExistingInstance,
    ExistingMatch, FieldAlreadyExistsError, FieldOccurrence, FieldRole, FieldSpec, INSIDE_MIN,
    InstanceValues, KnownDefs, MIN_RESOLVED_DISTINCT, PrecedenceRule, RowIntent, RowKey, RowValue,
    SHAPE_MIN, ScalarKind, Section, SectionError, StoredAssignmentProject, TYPE_COVERAGE_MIN,
    TargetRef, TargetShape, UnknownFieldError, Winner, coverage, majority_owner,
    reconstruct_candidate_type, top_level_field,
};
// Measurement-only diagnostics — see `assignment`'s own re-export of this
// same type for why it's gated identically.
#[cfg(any(test, feature = "test-support"))]
pub use assignment::ReferenceFieldDiagnostics;
pub use decision::{Decision, DecisionSet, ResolveError, SorterOverrides, validate_action};
pub use def_ref::{DefRef, DefRefParseError};
pub use finding::{
    DefKey, EdgeWinner, Finding, FindingKey, FindingKeyParseError, PatchFailureCause, ReorderKind,
    normalize_log_text,
};
pub use ledger::{Ledger, LedgerStats};
pub use merge::{
    FieldPath, FieldPathParseError, GeneratedModIdentity, GeneratedMods, ItemId, MergeChoice,
    PathSegment,
};
pub use order::{BothOrders, OrderSource};
pub use patch::{
    PatchDecisionError, PatchId, PatchIdParseError, PatchIdentityError, PatchModIdentity,
    PatchProject, PatchScope, PatchScopeError, ScopeChange, ScopeMembership, StoredPatchProject,
    UnpatchableAction,
};
pub use rationale::Rationale;
pub use resolution::{
    Action, Alternative, Confidence, ConfidenceError, MergeFindingKind, MergeState,
    PromotedRuleKey, Resolution, ResolutionStatus, Suggestion, merge_status,
    redecide_for_clean_merge, redecide_for_identical_copies,
};
pub use rule::{
    ClusterRuleId, ClusterRuleIdError, IncompatibleRule, PairRule, PairRuleEvidence, Placement,
    PlacementRule, Rule, RuleOrigin, RuleSet, classify_pair_rule,
};
pub use tag::{
    ManualTag, Tag, TagAssignment, TagError, TagEvidence, TagMode, TagProvenance, TagRule,
    TagSignal, Tagging,
};
