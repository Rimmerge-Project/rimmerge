//! DTOs for `list_order`/`explain_placement`: one row per mod in a load
//! order, and the full why-panel explanation for one mod's placement.
//!
//! Every edge-shaped type here (`EdgeDto`, `DroppedEdgeDto`,
//! `AdvisoryEdgeDto`) is display-only — never received back from the
//! frontend, so `From` is one-directional even though
//! [`EdgeProvenanceDto`]/[`TierReasonDto`] mirror
//! [`rim_resolve::sort::EdgeProvenance`]/[`rim_resolve::sort::TierReason`]
//! as structured tagged unions (never a pre-rendered string) for the
//! frontend to build a localized sentence from.

use rim_resolve::sort::{
    AdvisoryEdge, DroppedEdge, EdgeProvenance, Layer, OrderingEdge, PlacementExplanation,
    TierReason,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{EdgeKindDto, EdgeStrengthDto, RuleOriginDto, SourceDto, TierDto};

/// Mirrors [`Layer`] as a display label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum LayerDto {
    /// See [`Layer::Hard`].
    Hard,
    /// See [`Layer::AnyOf`].
    AnyOf,
    /// See [`Layer::DeclaredOverride`].
    DeclaredOverride,
    /// See [`Layer::Declared`].
    Declared,
    /// See [`Layer::UserDecision`].
    UserDecision,
    /// See [`Layer::RimSortUser`].
    RimSortUser,
    /// See [`Layer::RimSortCommunity`].
    RimSortCommunity,
    /// See [`Layer::SteamDb`].
    SteamDb,
    /// See [`Layer::Inferred`].
    Inferred,
    /// See [`Layer::Soft`].
    Soft,
    /// See [`Layer::Awareness`].
    Awareness,
}

impl From<Layer> for LayerDto {
    fn from(value: Layer) -> Self {
        match value {
            Layer::Hard => Self::Hard,
            Layer::AnyOf => Self::AnyOf,
            Layer::DeclaredOverride => Self::DeclaredOverride,
            Layer::Declared => Self::Declared,
            Layer::UserDecision => Self::UserDecision,
            Layer::RimSortUser => Self::RimSortUser,
            Layer::RimSortCommunity => Self::RimSortCommunity,
            Layer::SteamDb => Self::SteamDb,
            Layer::Inferred => Self::Inferred,
            Layer::Soft => Self::Soft,
            Layer::Awareness => Self::Awareness,
        }
    }
}

/// Mirrors [`EdgeProvenance`] as a structured tag, for the frontend to
/// render a localized sentence from — the sibling of [`EdgeDto::kind`]:
/// `kind` stays the engine edge kind (`None` off this enum's own
/// `Engine` case too, since that shape carries no data of its own —
/// [`EdgeDto::detail`] is where an `Engine` edge's real, English
/// analyzer text lives, kept there and never translated, per this
/// workspace's "shown as evidence" rule).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum EdgeProvenanceDto {
    /// See [`EdgeProvenance::Engine`]. Carries no fields of its own —
    /// [`EdgeDto::kind`]/[`EdgeDto::detail`] already carry the engine
    /// kind and the analyzer's own detail text for this case.
    Engine,
    /// See [`EdgeProvenance::AnyOf`].
    #[serde(rename_all = "camelCase")]
    AnyOf {
        /// The shared assembly name.
        assembly: String,
    },
    /// See [`EdgeProvenance::Rule`].
    #[serde(rename_all = "camelCase")]
    Rule {
        /// Where the rule came from.
        origin: RuleOriginDto,
        /// The rule's own free-text note, if any — user-authored, never
        /// translated.
        comment: Option<String>,
    },
    /// See [`EdgeProvenance::Tier`].
    #[serde(rename_all = "camelCase")]
    Tier {
        /// The tier this boundary belongs to.
        tier: TierDto,
    },
}

impl From<&EdgeProvenance> for EdgeProvenanceDto {
    fn from(value: &EdgeProvenance) -> Self {
        match value {
            EdgeProvenance::Engine { .. } => Self::Engine,
            EdgeProvenance::AnyOf { assembly } => Self::AnyOf {
                assembly: assembly.clone(),
            },
            EdgeProvenance::Rule { origin, comment } => Self::Rule {
                origin: (*origin).into(),
                comment: comment.clone(),
            },
            EdgeProvenance::Tier { tier } => Self::Tier {
                tier: (*tier).into(),
            },
        }
    }
}

/// One accepted load-order constraint between two mods, flattened for
/// display. Mirrors [`OrderingEdge`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct EdgeDto {
    /// The dependent side: must load after `before`.
    pub after: String,
    /// The dependency side: must load before `after`.
    pub before: String,
    /// Which layer this edge was added in.
    pub layer: LayerDto,
    /// The engine edge kind, when this edge came directly from the
    /// analyzer (`None` for rule/cluster/tier/any-of edges).
    pub kind: Option<EdgeKindDto>,
    /// Where this edge came from, structured — the frontend renders a
    /// localized sentence from it.
    pub provenance: EdgeProvenanceDto,
    /// The analyzer's own description of the edge, verbatim — `Some`
    /// only when [`Self::provenance`] is [`EdgeProvenanceDto::Engine`].
    /// Kept English and shown as an evidence line, never translated:
    /// this is [`rim_analyzer::domain::Edge::detail`]'s own text, which
    /// can embed file names and xpaths no translation could safely
    /// rewrite.
    pub detail: Option<String>,
}

impl From<&OrderingEdge> for EdgeDto {
    fn from(edge: &OrderingEdge) -> Self {
        let (kind, detail) = match &edge.provenance {
            EdgeProvenance::Engine { kind, detail } => (Some((*kind).into()), Some(detail.clone())),
            EdgeProvenance::AnyOf { .. }
            | EdgeProvenance::Rule { .. }
            | EdgeProvenance::Tier { .. } => (None, None),
        };
        Self {
            after: edge.after.as_str().to_string(),
            before: edge.before.as_str().to_string(),
            layer: edge.layer.into(),
            kind,
            provenance: (&edge.provenance).into(),
            detail,
        }
    }
}

/// An edge the sorter dropped to keep the graph acyclic. Mirrors
/// [`DroppedEdge`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DroppedEdgeDto {
    /// The edge that had to be dropped.
    pub edge: EdgeDto,
    /// The cycle it would otherwise have closed, as mod ids.
    pub witness_cycle: Vec<String>,
    /// The edge that overruled this one, when the witness cycle was a
    /// direct two-mod contradiction. `None` for a longer cycle, where no single edge can be
    /// blamed for the contradiction.
    pub winner: Option<EdgeDto>,
}

impl From<&DroppedEdge> for DroppedEdgeDto {
    fn from(dropped: &DroppedEdge) -> Self {
        Self {
            edge: (&dropped.edge).into(),
            witness_cycle: dropped
                .witness_cycle
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
            winner: dropped.winner.as_ref().map(Into::into),
        }
    }
}

/// A `Soft`/`Awareness` engine edge excluded from the graph by
/// [`rim_resolve::sort::EnforcedLayers`]. Mirrors [`AdvisoryEdge`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AdvisoryEdgeDto {
    /// The excluded edge.
    pub edge: EdgeDto,
    /// Whether the emitted order happens to satisfy it anyway.
    pub satisfied: bool,
    /// `Soft` or `Awareness`.
    pub strength: EdgeStrengthDto,
}

impl From<&AdvisoryEdge> for AdvisoryEdgeDto {
    fn from(advisory: &AdvisoryEdge) -> Self {
        Self {
            edge: (&advisory.edge).into(),
            satisfied: advisory.satisfied,
            strength: advisory.strength.into(),
        }
    }
}

/// The tie-break inputs behind one mod's exact position. Mirrors
/// [`rim_resolve::sort::PlacementTieBreak`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct TieBreakDto {
    /// This mod's position in the current order, if any.
    pub current_position: Option<usize>,
    /// The key actually used to schedule this mod.
    pub effective_key: usize,
    /// The dependent edge that pulled this mod's key below its own base,
    /// if any.
    pub pulled_forward_by: Option<EdgeDto>,
    /// How many other mods were emitted between `became_ready_after` and
    /// this mod.
    pub mods_preferred_ahead: usize,
}

/// Mirrors [`TierReason`], for the frontend to render a localized
/// sentence from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum TierReasonDto {
    /// See [`TierReason::Source`].
    #[serde(rename_all = "camelCase")]
    Source {
        /// `Source::Core`/`Source::Dlc`.
        source: SourceDto,
    },
    /// See [`TierReason::Placement`].
    #[serde(rename_all = "camelCase")]
    Placement {
        /// Where the placement rule came from.
        origin: RuleOriginDto,
    },
    /// See [`TierReason::PromotedBy`].
    #[serde(rename_all = "camelCase")]
    PromotedBy {
        /// The stronger edge that pulled this mod out of its nominal
        /// tier.
        edge: EdgeDto,
    },
    /// See [`TierReason::Body`].
    Body,
}

impl From<&TierReason> for TierReasonDto {
    fn from(value: &TierReason) -> Self {
        match value {
            TierReason::Source(source) => Self::Source {
                source: (*source).into(),
            },
            TierReason::Placement(origin) => Self::Placement {
                origin: (*origin).into(),
            },
            TierReason::PromotedBy(edge) => Self::PromotedBy { edge: edge.into() },
            TierReason::Body => Self::Body,
        }
    }
}

/// Why one mod ended up exactly where it did. Mirrors
/// [`PlacementExplanation`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PlacementExplanationDto {
    /// The explained mod.
    pub mod_id: String,
    /// Its zero-based position in the emitted order.
    pub position: usize,
    /// Its position in the current order, if any.
    pub previous_position: Option<usize>,
    /// The tier it was assigned to.
    pub tier: TierDto,
    /// Why it's in that tier, structured.
    pub tier_reason: TierReasonDto,
    /// The edge that actually made this mod ready.
    pub became_ready_after: Option<EdgeDto>,
    /// Every accepted incoming edge, `Hard` first.
    pub lower_bounds: Vec<EdgeDto>,
    /// Every accepted outgoing edge, `Hard` first.
    pub upper_bounds: Vec<EdgeDto>,
    /// Every dropped edge touching this mod.
    pub dropped: Vec<DroppedEdgeDto>,
    /// Every advisory (not-enforced) edge touching this mod.
    pub advisory: Vec<AdvisoryEdgeDto>,
    /// The tie-break inputs behind its exact position.
    pub tie_break: TieBreakDto,
}

impl From<&PlacementExplanation> for PlacementExplanationDto {
    fn from(explanation: &PlacementExplanation) -> Self {
        Self {
            mod_id: explanation.mod_id.as_str().to_string(),
            position: explanation.position,
            previous_position: explanation.previous_position,
            tier: explanation.tier.into(),
            tier_reason: (&explanation.tier_reason).into(),
            became_ready_after: explanation.became_ready_after.as_ref().map(Into::into),
            lower_bounds: explanation.lower_bounds.iter().map(Into::into).collect(),
            upper_bounds: explanation.upper_bounds.iter().map(Into::into).collect(),
            dropped: explanation.dropped.iter().map(Into::into).collect(),
            advisory: explanation.advisory.iter().map(Into::into).collect(),
            tie_break: TieBreakDto {
                current_position: explanation.tie_break.current_position,
                effective_key: explanation.tie_break.effective_key,
                pulled_forward_by: explanation
                    .tie_break
                    .pulled_forward_by
                    .as_ref()
                    .map(Into::into),
                mods_preferred_ahead: explanation.tie_break.mods_preferred_ahead,
            },
        }
    }
}

/// One row of a load order listing. Mirrors one mod's projection over
/// [`rim_analyzer::domain::Mod`] plus its [`PlacementExplanation`] for the
/// selected order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct OrderRowDto {
    /// The mod's id.
    pub mod_id: String,
    /// Its display name.
    pub name: String,
    /// Its zero-based position in this order.
    pub position: usize,
    /// Its position in the current order — `None` when this row is
    /// itself the current order (nothing to compare it to) or when the
    /// mod isn't present there at all, even when listing a different
    /// order and the position happens to be unchanged.
    pub previous_position: Option<usize>,
    /// The tier it was assigned to.
    pub tier: TierDto,
    /// Every tag it currently carries.
    pub tags: Vec<String>,
    /// Distinct active mods with a Hard-strength edge pointing at it.
    pub hard_dependents: usize,
    /// How many live findings naming this mod still need input.
    pub needs_input_count: usize,
}
