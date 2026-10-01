//! Suggestions and decisions: actions, alternatives, promoted rule keys, merge choices, resolutions, decide requests/results.

use std::collections::BTreeMap;

use rim_resolve::domain::{
    Action, Alternative, FieldPath, MergeChoice, PromotedRuleKey, Resolution, Suggestion, TagSignal,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::evidence::FindingDto;
use super::key::LedgerStatsDto;
use super::rationale::RationaleDto;
use crate::dto::common::{DefKeyDto, EdgeKindDto, ResolutionStatusDto};
use crate::dto::merge::MergeStateDto;
use crate::dto::patch::{ScopeMembershipDto, scope_membership_dto};
use crate::error::CommandError;

/// A resolving move on a finding. Mirrors [`Action`]; tagged on `kind`
/// (this DTO's own convention, distinct from the domain enum's `action`
/// tag) rather than reusing the domain's serde shape directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ActionDto {
    /// See [`Action::Accept`].
    Accept,
    /// See [`Action::Ignore`].
    Ignore,
    /// See [`Action::Reorder`].
    Reorder {
        /// The mod that must load after `before`.
        after: String,
        /// The mod that must load before `after`.
        before: String,
    },
    /// See [`Action::PreferWinner`].
    PreferWinner {
        /// The contested def.
        key: DefKeyDto,
        /// The owner whose def should win.
        winner: String,
    },
    /// See [`Action::ChooseCandidate`].
    ChooseCandidate {
        /// The mod requiring one of the candidates.
        after: String,
        /// The chosen candidate.
        chosen: String,
    },
    /// See [`Action::DropEdge`].
    #[serde(rename_all = "camelCase")]
    DropEdge {
        /// The edge's dependent side.
        after: String,
        /// The edge's dependency side.
        before: String,
        /// The kind of edge to drop.
        edge_kind: EdgeKindDto,
    },
    /// See [`Action::KeepEdge`].
    #[serde(rename_all = "camelCase")]
    KeepEdge {
        /// The edge's dependent side.
        after: String,
        /// The edge's dependency side.
        before: String,
        /// The kind of edge to keep.
        edge_kind: EdgeKindDto,
    },
    /// See [`Action::AddTag`].
    #[serde(rename_all = "camelCase")]
    AddTag {
        /// The mod to tag.
        mod_id: String,
        /// The tag to add.
        tag: String,
    },
    /// See [`Action::RemoveTag`].
    #[serde(rename_all = "camelCase")]
    RemoveTag {
        /// The mod to untag.
        mod_id: String,
        /// The tag to remove.
        tag: String,
    },
    /// See [`Action::ExcludeFromCluster`].
    #[serde(rename_all = "camelCase")]
    ExcludeFromCluster {
        /// The cluster rule.
        rule: String,
        /// The member to exclude.
        mod_id: String,
    },
    /// See [`Action::RemoveMod`].
    #[serde(rename_all = "camelCase")]
    RemoveMod {
        /// The mod to remove.
        mod_id: String,
    },
    /// See [`Action::Merge`].
    Merge {
        /// The contested def.
        key: DefKeyDto,
        /// Per-field choices, keyed by the field's canonical path text
        /// (see [`rim_resolve::domain::FieldPath`]'s `Display`).
        choices: BTreeMap<String, MergeChoiceDto>,
    },
    /// See [`Action::ShipAsset`].
    #[serde(rename_all = "camelCase")]
    ShipAsset {
        /// The shared, normalized texture path.
        texture_path: String,
        /// The owner whose file to copy in.
        from: String,
    },
    /// See [`Action::PromoteRule`].
    PromoteRule {
        /// Which rule to promote.
        rule: PromotedRuleKeyDto,
    },
    /// See [`Action::DropRule`].
    #[serde(rename_all = "camelCase")]
    DropRule {
        /// The dropped rule's dependent side.
        after: String,
        /// The dropped rule's dependency side.
        before: String,
    },
}

/// Which rule [`ActionDto::PromoteRule`] promotes. Mirrors
/// [`PromotedRuleKey`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum PromotedRuleKeyDto {
    /// See [`PromotedRuleKey::Pair`].
    #[serde(rename_all = "camelCase")]
    Pair {
        /// The dependent side.
        after: String,
        /// The dependency side.
        before: String,
    },
    /// See [`PromotedRuleKey::Placement`].
    #[serde(rename_all = "camelCase")]
    Placement {
        /// The pinned mod.
        mod_id: String,
    },
}

impl From<PromotedRuleKey> for PromotedRuleKeyDto {
    fn from(value: PromotedRuleKey) -> Self {
        match value {
            PromotedRuleKey::Pair { after, before } => Self::Pair {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
            },
            PromotedRuleKey::Placement { mod_id } => Self::Placement {
                mod_id: mod_id.as_str().to_string(),
            },
        }
    }
}

impl From<PromotedRuleKeyDto> for PromotedRuleKey {
    fn from(value: PromotedRuleKeyDto) -> Self {
        use rim_analyzer::domain::ModId;

        match value {
            PromotedRuleKeyDto::Pair { after, before } => Self::Pair {
                after: ModId::new(after),
                before: ModId::new(before),
            },
            PromotedRuleKeyDto::Placement { mod_id } => Self::Placement {
                mod_id: ModId::new(mod_id),
            },
        }
    }
}

/// A per-field merge choice. Mirrors [`MergeChoice`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "choice", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum MergeChoiceDto {
    /// See [`MergeChoice::From`].
    #[serde(rename_all = "camelCase")]
    From {
        /// The owner whose value to take.
        mod_id: String,
    },
    /// See [`MergeChoice::Value`].
    Value {
        /// The literal value.
        text: String,
    },
    /// See [`MergeChoice::Drop`].
    Drop,
}

impl From<MergeChoice> for MergeChoiceDto {
    fn from(value: MergeChoice) -> Self {
        match value {
            MergeChoice::From { mod_id } => Self::From {
                mod_id: mod_id.as_str().to_string(),
            },
            MergeChoice::Value { text } => Self::Value { text },
            MergeChoice::Drop => Self::Drop,
        }
    }
}

impl From<MergeChoiceDto> for MergeChoice {
    fn from(value: MergeChoiceDto) -> Self {
        use rim_analyzer::domain::ModId;

        match value {
            MergeChoiceDto::From { mod_id } => Self::From {
                mod_id: ModId::new(mod_id),
            },
            MergeChoiceDto::Value { text } => Self::Value { text },
            MergeChoiceDto::Drop => Self::Drop,
        }
    }
}

impl From<Action> for ActionDto {
    fn from(value: Action) -> Self {
        match value {
            Action::Accept => Self::Accept,
            Action::Ignore => Self::Ignore,
            Action::Reorder { after, before } => Self::Reorder {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
            },
            Action::PreferWinner { key, winner } => Self::PreferWinner {
                key: key.into(),
                winner: winner.as_str().to_string(),
            },
            Action::ChooseCandidate { after, chosen } => Self::ChooseCandidate {
                after: after.as_str().to_string(),
                chosen: chosen.as_str().to_string(),
            },
            Action::DropEdge {
                after,
                before,
                kind,
            } => Self::DropEdge {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
                edge_kind: kind.into(),
            },
            Action::KeepEdge {
                after,
                before,
                kind,
            } => Self::KeepEdge {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
                edge_kind: kind.into(),
            },
            Action::AddTag { mod_id, tag } => Self::AddTag {
                mod_id: mod_id.as_str().to_string(),
                tag: tag.as_str().to_string(),
            },
            Action::RemoveTag { mod_id, tag } => Self::RemoveTag {
                mod_id: mod_id.as_str().to_string(),
                tag: tag.as_str().to_string(),
            },
            Action::ExcludeFromCluster { rule, mod_id } => Self::ExcludeFromCluster {
                rule: rule.to_string(),
                mod_id: mod_id.as_str().to_string(),
            },
            Action::RemoveMod { mod_id } => Self::RemoveMod {
                mod_id: mod_id.as_str().to_string(),
            },
            Action::Merge { key, choices } => Self::Merge {
                key: key.into(),
                choices: choices
                    .into_iter()
                    .map(|(path, choice)| (path.to_string(), choice.into()))
                    .collect(),
            },
            Action::ShipAsset { texture_path, from } => Self::ShipAsset {
                texture_path,
                from: from.as_str().to_string(),
            },
            Action::PromoteRule { rule } => Self::PromoteRule { rule: rule.into() },
            Action::DropRule { after, before } => Self::DropRule {
                after: after.as_str().to_string(),
                before: before.as_str().to_string(),
            },
        }
    }
}

impl TryFrom<ActionDto> for Action {
    type Error = CommandError;

    fn try_from(value: ActionDto) -> Result<Self, Self::Error> {
        use rim_analyzer::domain::ModId;
        use rim_resolve::domain::{ClusterRuleId, Tag};

        Ok(match value {
            ActionDto::Accept => Self::Accept,
            ActionDto::Ignore => Self::Ignore,
            ActionDto::Reorder { after, before } => Self::Reorder {
                after: ModId::new(after),
                before: ModId::new(before),
            },
            ActionDto::PreferWinner { key, winner } => Self::PreferWinner {
                key: key.into(),
                winner: ModId::new(winner),
            },
            ActionDto::ChooseCandidate { after, chosen } => Self::ChooseCandidate {
                after: ModId::new(after),
                chosen: ModId::new(chosen),
            },
            ActionDto::DropEdge {
                after,
                before,
                edge_kind,
            } => Self::DropEdge {
                after: ModId::new(after),
                before: ModId::new(before),
                kind: edge_kind.into(),
            },
            ActionDto::KeepEdge {
                after,
                before,
                edge_kind,
            } => Self::KeepEdge {
                after: ModId::new(after),
                before: ModId::new(before),
                kind: edge_kind.into(),
            },
            ActionDto::AddTag { mod_id, tag } => Self::AddTag {
                mod_id: ModId::new(mod_id),
                tag: Tag::new(tag)?,
            },
            ActionDto::RemoveTag { mod_id, tag } => Self::RemoveTag {
                mod_id: ModId::new(mod_id),
                tag: Tag::new(tag)?,
            },
            ActionDto::ExcludeFromCluster { rule, mod_id } => Self::ExcludeFromCluster {
                rule: ClusterRuleId::new(rule)?,
                mod_id: ModId::new(mod_id),
            },
            ActionDto::RemoveMod { mod_id } => Self::RemoveMod {
                mod_id: ModId::new(mod_id),
            },
            ActionDto::Merge { key, choices } => {
                let mut parsed: BTreeMap<FieldPath, MergeChoice> = BTreeMap::new();
                for (path_text, choice) in choices {
                    let path: FieldPath = path_text.parse().map_err(
                        |error: rim_resolve::domain::FieldPathParseError| {
                            CommandError::invalid_input(error.to_string())
                        },
                    )?;
                    parsed.insert(path, choice.into());
                }
                Self::Merge {
                    key: key.into(),
                    choices: parsed,
                }
            }
            ActionDto::ShipAsset { texture_path, from } => Self::ShipAsset {
                texture_path,
                from: ModId::new(from),
            },
            ActionDto::PromoteRule { rule } => Self::PromoteRule { rule: rule.into() },
            ActionDto::DropRule { after, before } => Self::DropRule {
                after: ModId::new(after),
                before: ModId::new(before),
            },
        })
    }
}

/// An action the user didn't take, offered alongside the suggestion.
/// Mirrors [`Alternative`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AlternativeDto {
    /// The alternative action.
    pub action: ActionDto,
    /// Why it's offered, in English — kept for the pair-rule `--comment`
    /// prefill; the UI renders [`Self::rationale_code`] instead.
    pub rationale: String,
    /// Why it's offered, structured for a localized rendering.
    pub rationale_code: RationaleDto,
}

impl From<&Alternative> for AlternativeDto {
    fn from(value: &Alternative) -> Self {
        Self {
            action: value.action.clone().into(),
            rationale: value.rationale.to_string(),
            rationale_code: (&value.rationale).into(),
        }
    }
}

/// What the ledger recommends for one finding. Mirrors [`Suggestion`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SuggestionDto {
    /// The recommended action.
    pub action: ActionDto,
    /// How confident the recommendation is, 0..=100.
    pub confidence: u8,
    /// Why this action was recommended, in English — kept for the
    /// pair-rule `--comment` prefill; the UI renders
    /// [`Self::rationale_code`] instead.
    pub rationale: String,
    /// Why this action was recommended, structured for a localized
    /// rendering.
    pub rationale_code: RationaleDto,
    /// Other actions the user could take instead.
    pub alternatives: Vec<AlternativeDto>,
}

impl From<&Suggestion> for SuggestionDto {
    fn from(value: &Suggestion) -> Self {
        Self {
            action: value.action.clone().into(),
            confidence: value.confidence.percent(),
            rationale: value.rationale.to_string(),
            rationale_code: (&value.rationale).into(),
            alternatives: value.alternatives.iter().map(Into::into).collect(),
        }
    }
}

/// Mirrors [`TagSignal`], structured — shared with
/// [`super::tag::TagProvenanceDto`] so both places a tag's matched
/// signals cross IPC describe them the same way, and rendered by the
/// frontend rather than pre-formatted here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum TagSignalDto {
    /// See [`TagSignal::UrlContains`].
    #[serde(rename_all = "camelCase")]
    UrlContains {
        /// The substring the `About.xml` `url` must contain.
        needle: String,
    },
    /// See [`TagSignal::AssemblyRefTo`].
    #[serde(rename_all = "camelCase")]
    AssemblyRefTo {
        /// The mod the `AssemblyRef` edge points to.
        mod_id: String,
    },
    /// See [`TagSignal::DependsOn`].
    #[serde(rename_all = "camelCase")]
    DependsOn {
        /// The mod named in `modDependencies`/`loadAfter`.
        mod_id: String,
    },
}

impl From<&TagSignal> for TagSignalDto {
    fn from(value: &TagSignal) -> Self {
        match value {
            TagSignal::UrlContains(needle) => Self::UrlContains {
                needle: needle.clone(),
            },
            TagSignal::AssemblyRefTo(mod_id) => Self::AssemblyRefTo {
                mod_id: mod_id.as_str().to_string(),
            },
            TagSignal::DependsOn(mod_id) => Self::DependsOn {
                mod_id: mod_id.as_str().to_string(),
            },
        }
    }
}

/// One finding summarized for a list row: enough to render the inbox
/// list without the full evidence payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ResolutionSummaryDto {
    /// The finding's canonical key text.
    pub key: String,
    /// The full evidence behind it — the frontend derives its own title
    /// from this via `findingTitle()`, per this crate's "backend sends
    /// codes, frontend renders" rule.
    pub finding: FindingDto,
    /// Whether this still needs the user's attention.
    pub status: ResolutionStatusDto,
    /// The suggestion's confidence, 0..=100.
    pub confidence: u8,
    /// The action actually in effect.
    pub effective: ActionDto,
    /// Whether the user has decided this finding.
    pub has_decision: bool,
    /// The merge preview's status, when the decision is a `Merge` and a
    /// preview has been computed; `null` otherwise. See
    /// [`rim_resolve::domain::Resolution::merge`].
    pub merge_state: Option<MergeStateDto>,
    /// The structural guard: the
    /// field that forced `merge_state` to `needsFieldInput`, when a
    /// pre-existing `Merge` decision's own def has since become guarded.
    /// `null` whenever `merge_state` is `null` or the guard never fired.
    /// See [`rim_resolve::domain::Resolution::structural_guard_field`].
    pub structural_guard_field: Option<String>,
    /// Whether this finding is fully or only partly inside a compat
    /// patch's scope; `null` in the profile inbox. See
    /// [`rim_resolve::domain::Resolution::scope`].
    pub scope: Option<ScopeMembershipDto>,
    /// The def/template this finding names, when it's one of the three
    /// kinds `FindingKey::def_ref()` resolves — links the suggestion
    /// panel and change summary to the def inspector
    /// `null` for every other
    /// kind.
    pub def_ref: Option<String>,
}

/// One finding's full detail: evidence, suggestion, alternatives, and any
/// decision on file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ResolutionDetailDto {
    /// The finding's canonical key text.
    pub key: String,
    /// The full evidence behind it.
    pub finding: FindingDto,
    /// What the ledger recommends.
    pub suggestion: SuggestionDto,
    /// Whether this still needs the user's attention.
    pub status: ResolutionStatusDto,
    /// The action actually in effect.
    pub effective: ActionDto,
    /// The user's note, if a decision exists and carries one.
    pub note: Option<String>,
    /// Whether the user has decided this finding.
    pub has_decision: bool,
    /// Only present when this ledger was built for `Current`: would the
    /// suggested order already resolve it?
    pub resolved_by_suggested: Option<bool>,
    /// The merge preview's status, when the decision is a `Merge` and a
    /// preview has been computed; `null` otherwise. See
    /// [`rim_resolve::domain::Resolution::merge`].
    pub merge_state: Option<MergeStateDto>,
    /// See [`ResolutionSummaryDto::structural_guard_field`].
    pub structural_guard_field: Option<String>,
    /// See [`ResolutionSummaryDto::scope`].
    pub scope: Option<ScopeMembershipDto>,
    /// See [`ResolutionSummaryDto::def_ref`].
    pub def_ref: Option<String>,
}

impl From<&Resolution> for ResolutionSummaryDto {
    fn from(resolution: &Resolution) -> Self {
        Self {
            key: resolution.key.to_string(),
            finding: (&resolution.finding).into(),
            status: resolution.status.into(),
            confidence: resolution.suggestion.confidence.percent(),
            effective: resolution.effective.clone().into(),
            has_decision: resolution.decision.is_some(),
            merge_state: resolution.merge.as_ref().map(Into::into),
            structural_guard_field: resolution.structural_guard_field.clone(),
            scope: resolution.scope.as_ref().and_then(scope_membership_dto),
            def_ref: resolution.key.def_ref().map(|def_ref| def_ref.to_string()),
        }
    }
}

impl From<&Resolution> for ResolutionDetailDto {
    fn from(resolution: &Resolution) -> Self {
        Self {
            key: resolution.key.to_string(),
            finding: (&resolution.finding).into(),
            suggestion: (&resolution.suggestion).into(),
            status: resolution.status.into(),
            effective: resolution.effective.clone().into(),
            note: resolution.decision.as_ref().and_then(|d| d.note.clone()),
            has_decision: resolution.decision.is_some(),
            resolved_by_suggested: resolution.resolved_by_suggested,
            merge_state: resolution.merge.as_ref().map(Into::into),
            structural_guard_field: resolution.structural_guard_field.clone(),
            scope: resolution.scope.as_ref().and_then(scope_membership_dto),
            def_ref: resolution.key.def_ref().map(|def_ref| def_ref.to_string()),
        }
    }
}

/// Request shape for `decide`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DecideRequestDto {
    /// The finding's canonical key text.
    pub key: String,
    /// The chosen action.
    pub action: ActionDto,
    /// An optional free-text note.
    pub note: Option<String>,
}

/// One decision whose key no longer matches a live finding in either
/// order's ledger — an owner set changed, or the underlying condition is
/// simply gone. Still on file (never silently dropped); `revert_decision`
/// with this `key` is how the rules page's orphan list prunes one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct OrphanedDecisionDto {
    /// The decision's canonical key text.
    pub key: String,
    /// The action it recorded.
    pub action: ActionDto,
    /// The user's note, if any.
    pub note: Option<String>,
}

impl From<&rim_resolve::domain::Decision> for OrphanedDecisionDto {
    fn from(decision: &rim_resolve::domain::Decision) -> Self {
        Self {
            key: decision.key.to_string(),
            action: decision.action.clone().into(),
            note: decision.note.clone(),
        }
    }
}

/// What `decide`/`revert_decision` did. Mirrors
/// [`rim_session::use_cases::Decide::execute`]'s outcome plus the
/// resulting ledger stats and moved-mod count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DecideResultDto {
    /// The selected order's ledger stats after the change.
    pub stats: LedgerStatsDto,
    /// Whether the change caused a resort.
    pub resorted: bool,
    /// How many mods changed position in the suggested order (0 when
    /// `resorted` is `false`).
    pub moved_mods: usize,
}
