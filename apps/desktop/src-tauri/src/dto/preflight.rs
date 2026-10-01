//! DTOs for `get_apply_preflight`: the hard problems in the order `apply`
//! would write, mirrored from [`rim_resolve::preflight`].
//!
//! Display-only — never received back from the frontend, so `From` is
//! one-directional. [`HardProblemDto`] is `kind`-tagged with a named
//! payload struct per variant (the `WelcomeDto` pattern): a struct
//! *variant*'s own fields would need their own `rename_all`.

use rim_resolve::preflight::{Availability, HardProblem, MissingModOutcome, PreflightItem};
use rim_session::use_cases::ApplyPreflight;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{EdgeKindDto, OrderSourceDto};

/// Mirrors [`Availability`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum AvailabilityDto {
    /// See [`Availability::InstalledInactive`].
    InstalledInactive,
    /// See [`Availability::NotInstalled`].
    NotInstalled,
}

impl From<Availability> for AvailabilityDto {
    fn from(value: Availability) -> Self {
        match value {
            Availability::InstalledInactive => Self::InstalledInactive,
            Availability::NotInstalled => Self::NotInstalled,
        }
    }
}

/// Mirrors [`MissingModOutcome`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum MissingModOutcomeDto {
    /// See [`MissingModOutcome::RemovedFromActiveList`].
    RemovedFromActiveList,
    /// See [`MissingModOutcome::KeptInActiveList`].
    KeptInActiveList,
}

impl From<MissingModOutcome> for MissingModOutcomeDto {
    fn from(value: MissingModOutcome) -> Self {
        match value {
            MissingModOutcome::RemovedFromActiveList => Self::RemovedFromActiveList,
            MissingModOutcome::KeptInActiveList => Self::KeptInActiveList,
        }
    }
}

/// [`HardProblem::MissingDependency`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MissingDependencyDto {
    /// The mod that declares the requirement.
    pub mod_id: String,
    /// The required mod.
    pub dependency: String,
    /// The author's name for the dependency, when `About.xml` gave one.
    pub display_name: Option<String>,
    /// Whether the dependency is installed.
    pub availability: AvailabilityDto,
}

/// [`HardProblem::IncompatiblePair`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct IncompatiblePairDto {
    /// The smaller id of the pair.
    pub a: String,
    /// The larger id of the pair.
    pub b: String,
}

/// [`HardProblem::MissingMod`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct MissingModDto {
    /// The mod that is listed but not on disk.
    pub mod_id: String,
    /// What the written order does with it.
    pub outcome: MissingModOutcomeDto,
}

/// [`HardProblem::LoadRequirementViolated`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct LoadRequirementViolatedDto {
    /// The edge's dependent side (must load later).
    pub after: String,
    /// The edge's dependency side (must load first).
    pub before: String,
    /// The kind of edge.
    pub edge_kind: EdgeKindDto,
}

/// [`HardProblem::AnyOfUnsatisfied`]'s payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AnyOfUnsatisfiedDto {
    /// The mod that needs one of the candidates first.
    pub after: String,
    /// The candidates, sorted.
    pub candidates: Vec<String>,
}

/// Mirrors [`HardProblem`], `kind`-tagged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum HardProblemDto {
    /// See [`HardProblem::MissingDependency`].
    MissingDependency(MissingDependencyDto),
    /// See [`HardProblem::IncompatiblePair`].
    IncompatiblePair(IncompatiblePairDto),
    /// See [`HardProblem::MissingMod`].
    MissingMod(MissingModDto),
    /// See [`HardProblem::LoadRequirementViolated`].
    LoadRequirementViolated(LoadRequirementViolatedDto),
    /// See [`HardProblem::AnyOfUnsatisfied`].
    AnyOfUnsatisfied(AnyOfUnsatisfiedDto),
}

impl From<HardProblem> for HardProblemDto {
    fn from(value: HardProblem) -> Self {
        match value {
            HardProblem::MissingDependency {
                mod_id,
                dependency,
                display_name,
                availability,
            } => Self::MissingDependency(MissingDependencyDto {
                mod_id: mod_id.to_string(),
                dependency: dependency.to_string(),
                display_name,
                availability: availability.into(),
            }),
            HardProblem::IncompatiblePair { a, b } => Self::IncompatiblePair(IncompatiblePairDto {
                a: a.to_string(),
                b: b.to_string(),
            }),
            HardProblem::MissingMod { mod_id, outcome } => Self::MissingMod(MissingModDto {
                mod_id: mod_id.to_string(),
                outcome: outcome.into(),
            }),
            HardProblem::LoadRequirementViolated {
                after,
                before,
                kind,
            } => Self::LoadRequirementViolated(LoadRequirementViolatedDto {
                after: after.to_string(),
                before: before.to_string(),
                edge_kind: kind.into(),
            }),
            HardProblem::AnyOfUnsatisfied { after, candidates } => {
                Self::AnyOfUnsatisfied(AnyOfUnsatisfiedDto {
                    after: after.to_string(),
                    candidates: candidates.iter().map(ToString::to_string).collect(),
                })
            }
        }
    }
}

/// One hard problem and whether the user already decided it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PreflightItemDto {
    /// True when the problem's own finding carries a user decision.
    pub acknowledged: bool,
    /// What is wrong.
    pub problem: HardProblemDto,
}

impl From<PreflightItem> for PreflightItemDto {
    fn from(value: PreflightItem) -> Self {
        Self {
            acknowledged: value.acknowledged,
            problem: value.problem.into(),
        }
    }
}

/// The hard problems in one order — `get_apply_preflight`'s result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ApplyPreflightDto {
    /// The order the problems were found in.
    pub source: OrderSourceDto,
    /// The problems, sorted by kind and then by id.
    pub items: Vec<PreflightItemDto>,
    /// Whether some problem is still unanswered. Computed in Rust
    /// ([`ApplyPreflight::requires_confirmation`]), never re-derived by
    /// the frontend.
    pub requires_confirmation: bool,
}

impl From<ApplyPreflight> for ApplyPreflightDto {
    fn from(value: ApplyPreflight) -> Self {
        let requires_confirmation = value.requires_confirmation();
        Self {
            source: value.source.into(),
            items: value.items.into_iter().map(Into::into).collect(),
            requires_confirmation,
        }
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::{EdgeKind, ModId};
    use rim_resolve::domain::OrderSource;

    use super::*;

    fn item(problem: HardProblem, acknowledged: bool) -> PreflightItem {
        PreflightItem {
            problem,
            acknowledged,
        }
    }

    #[test]
    fn a_missing_dependency_serializes_kind_tagged_with_camel_case_fields() {
        let dto = HardProblemDto::from(HardProblem::MissingDependency {
            mod_id: ModId::new("app"),
            dependency: ModId::new("lib"),
            display_name: Some("Lib".to_string()),
            availability: Availability::InstalledInactive,
        });

        let json = serde_json::to_value(&dto).expect("serializes");

        assert_eq!(
            json,
            serde_json::json!({
                "kind": "missingDependency",
                "modId": "app",
                "dependency": "lib",
                "displayName": "Lib",
                "availability": "installedInactive",
            })
        );
    }

    #[test]
    fn every_problem_variant_maps_to_its_own_kind() {
        let problems = [
            HardProblem::IncompatiblePair {
                a: ModId::new("a"),
                b: ModId::new("b"),
            },
            HardProblem::MissingMod {
                mod_id: ModId::new("m"),
                outcome: MissingModOutcome::KeptInActiveList,
            },
            HardProblem::LoadRequirementViolated {
                after: ModId::new("x"),
                before: ModId::new("y"),
                kind: EdgeKind::ForceLoadAfter,
            },
            HardProblem::AnyOfUnsatisfied {
                after: ModId::new("x"),
                candidates: [ModId::new("c2"), ModId::new("c1")].into_iter().collect(),
            },
        ];

        let kinds: Vec<String> = problems
            .into_iter()
            .map(|problem| {
                let json = serde_json::to_value(HardProblemDto::from(problem)).expect("serializes");
                json["kind"].as_str().expect("kind is a string").to_string()
            })
            .collect();

        assert_eq!(
            kinds,
            [
                "incompatiblePair",
                "missingMod",
                "loadRequirementViolated",
                "anyOfUnsatisfied"
            ]
        );
    }

    #[test]
    fn any_of_candidates_keep_their_sorted_order() {
        let dto = HardProblemDto::from(HardProblem::AnyOfUnsatisfied {
            after: ModId::new("x"),
            candidates: [ModId::new("c2"), ModId::new("c1")].into_iter().collect(),
        });

        let HardProblemDto::AnyOfUnsatisfied(payload) = dto else {
            panic!("wrong variant");
        };

        assert_eq!(payload.candidates, ["c1", "c2"]);
    }

    #[test]
    fn requires_confirmation_comes_from_the_use_case_result() {
        let decided = ApplyPreflight {
            source: OrderSource::Suggested,
            items: vec![item(
                HardProblem::MissingMod {
                    mod_id: ModId::new("m"),
                    outcome: MissingModOutcome::RemovedFromActiveList,
                },
                true,
            )],
        };
        let mut open = decided.clone();
        open.items[0].acknowledged = false;

        assert!(!ApplyPreflightDto::from(decided).requires_confirmation);
        assert!(ApplyPreflightDto::from(open).requires_confirmation);
    }
}
