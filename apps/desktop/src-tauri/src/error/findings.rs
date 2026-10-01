//! Mapping decision, finding-key, tag, rule, confidence, def-ref, conflict-view, and def-inspection errors.

use super::{CommandError, CommandErrorCode};

impl From<rim_session::use_cases::DecideError> for CommandError {
    fn from(error: rim_session::use_cases::DecideError) -> Self {
        use rim_session::use_cases::DecideError as E;
        match error {
            E::Invalid(e) => e.into(),
            E::Store(e) => e.into(),
            // `decide_inner` always builds `Decide` via `with_rule_store`
            // (`AppState::adapters` always has a real `rule_store`), so
            // this can't actually be reached through the command — kept
            // exhaustive rather than panicking should that composition
            // ever change.
            E::PromoteNeedsRuleStore => {
                CommandError::internal("promoting a rule needs a rule store, but none was given")
            }
        }
    }
}

impl From<rim_resolve::domain::ResolveError> for CommandError {
    fn from(error: rim_resolve::domain::ResolveError) -> Self {
        // `ResolveError` is currently uninhabited (every `Action`
        // validates), so this side is never actually reached — the
        // exhaustive empty match is what makes that a compile-time fact
        // rather than an assumption.
        match error {}
    }
}

impl From<rim_resolve::domain::FindingKeyParseError> for CommandError {
    fn from(error: rim_resolve::domain::FindingKeyParseError) -> Self {
        Self::invalid_input(format!("invalid finding key: {error}"))
    }
}

impl From<rim_resolve::domain::TagError> for CommandError {
    fn from(error: rim_resolve::domain::TagError) -> Self {
        Self::invalid_input(error.to_string())
    }
}

impl From<rim_resolve::domain::ClusterRuleIdError> for CommandError {
    fn from(error: rim_resolve::domain::ClusterRuleIdError) -> Self {
        Self::invalid_input(error.to_string())
    }
}

impl From<rim_resolve::domain::ConfidenceError> for CommandError {
    fn from(error: rim_resolve::domain::ConfidenceError) -> Self {
        Self::invalid_input(error.to_string())
    }
}

impl From<rim_resolve::domain::DefRefParseError> for CommandError {
    fn from(error: rim_resolve::domain::DefRefParseError) -> Self {
        Self::invalid_input(error.to_string())
    }
}

impl From<rim_session::DefConflictViewError> for CommandError {
    fn from(error: rim_session::DefConflictViewError) -> Self {
        use rim_session::DefConflictViewError as E;
        let message = error.to_string();
        match error {
            // `key.def_ref()` returning `None` is already filtered out by
            // `get_def_conflict_view_inner` before this is ever reached
            // (the same check this variant itself makes internally) —
            // kept exhaustive rather than assuming the command's own
            // early check can never change.
            E::UnsupportedFinding(_) => Self::invalid_input(message),
            // The command always runs `InspectDef`/`PlanMerge` against
            // this exact key under the selected order before calling
            // `Session::def_conflict_view`, so these two are structurally
            // unreachable through it — `internal` names that rather than
            // inventing a dedicated code for a path that can't be hit
            // (mirrors `RenderMergeModError::Emit`'s own rationale).
            E::NotInspected(_) | E::NotPlanned(_) => Self::internal(message),
        }
    }
}

impl From<rim_session::use_cases::InspectDefError> for CommandError {
    fn from(error: rim_session::use_cases::InspectDefError) -> Self {
        use rim_session::use_cases::InspectDefError as E;
        let message = error.to_string();
        match error {
            E::NotFound(def_ref) => Self::def_not_found(&def_ref.to_string()),
            // The XML failed to parse for the same "something changed since
            // the scan" reason `PlanMergeError::Xml` maps to
            // `MergeSourceFailed` for.
            E::Source(_) | E::Xml(_) => Self::new(CommandErrorCode::MergeSourceFailed, message),
        }
    }
}
