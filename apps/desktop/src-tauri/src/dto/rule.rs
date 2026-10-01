//! DTOs for the rules page: pair/placement/incompatible rules, and
//! RimSort import.

use rim_resolve::domain::{IncompatibleRule, PairRule, PlacementRule, Rule};
use rim_session::ports::{ImportedRules, RimSortPaths, StoredRules};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{PlacementDto, RuleOriginDto};
use crate::error::CommandError;

/// Mirrors [`PairRule`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PairRuleDto {
    /// The mod that must load after `before`.
    pub after: String,
    /// The mod that must load before `after`.
    pub before: String,
    /// Where this rule came from.
    pub origin: RuleOriginDto,
    /// A free-text note.
    pub comment: Option<String>,
    /// The imported origin this row was promoted from (a promoted user rule),
    /// when this is a `userDecision` row copied from an
    /// import at the same `(after, before)` key — `None` for an imported
    /// row itself, or a hand-written user rule with no imported
    /// counterpart. Computed by [`fill_pair_promotion_fields`] over the
    /// *full* rule set, before any per-origin filter — a row built straight
    /// from a [`PairRule`] (this
    /// `From` impl) always starts `None`.
    #[serde(default)]
    pub promoted_from: Option<RuleOriginDto>,
    /// Whether an imported row at this key already has a `userDecision`
    /// copy — same full-rule-set computation as `promoted_from`. Always
    /// `false` for a `userDecision` row itself, and for a row built
    /// straight from a [`PairRule`] before that computation runs.
    #[serde(default)]
    pub already_promoted: bool,
    /// See [`PairRule::overrides_declared`] (the declared-edge override)
    /// — surfaced on the rules table regardless
    /// of origin, but only ever meaningful (and settable from
    /// `AddPairRuleDialog.vue`) for a `userDecision` row.
    #[serde(default)]
    pub overrides_declared: bool,
}

impl From<&PairRule> for PairRuleDto {
    fn from(rule: &PairRule) -> Self {
        Self {
            after: rule.after.as_str().to_string(),
            before: rule.before.as_str().to_string(),
            origin: rule.origin.into(),
            comment: rule.comment.clone(),
            promoted_from: None,
            already_promoted: false,
            overrides_declared: rule.overrides_declared,
        }
    }
}

/// Fills every row's `promoted_from`/`already_promoted` by
/// cross-referencing the *other* rows in `pairs` at the same `(after,
/// before)` key. Must run over the full, unfiltered rule set — a promoted
/// row's own imported original (or vice versa) can otherwise already be
/// missing by the time a per-origin filter (`list_rules`'s own `origin`
/// parameter) has run, so this cross-reference never runs client-side over
/// whatever subset of rows a filtered view happens to have on hand.
fn fill_pair_promotion_fields(pairs: &mut [PairRuleDto]) {
    let snapshot: Vec<(String, String, RuleOriginDto)> = pairs
        .iter()
        .map(|pair| (pair.after.clone(), pair.before.clone(), pair.origin))
        .collect();
    for pair in pairs.iter_mut() {
        if pair.origin == RuleOriginDto::UserDecision {
            pair.promoted_from = snapshot
                .iter()
                .find(|(after, before, origin)| {
                    *after == pair.after
                        && *before == pair.before
                        && *origin != RuleOriginDto::UserDecision
                })
                .map(|(_, _, origin)| *origin);
        } else {
            pair.already_promoted = snapshot.iter().any(|(after, before, origin)| {
                *after == pair.after
                    && *before == pair.before
                    && *origin == RuleOriginDto::UserDecision
            });
        }
    }
}

/// Mirrors [`PlacementRule`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PlacementRuleDto {
    /// The pinned mod.
    pub mod_id: String,
    /// Which tier it's pinned to.
    pub placement: PlacementDto,
    /// Where this rule came from.
    pub origin: RuleOriginDto,
    /// A free-text note.
    pub comment: Option<String>,
    /// See [`PairRuleDto::promoted_from`].
    #[serde(default)]
    pub promoted_from: Option<RuleOriginDto>,
    /// See [`PairRuleDto::already_promoted`].
    #[serde(default)]
    pub already_promoted: bool,
}

impl From<&PlacementRule> for PlacementRuleDto {
    fn from(rule: &PlacementRule) -> Self {
        Self {
            mod_id: rule.mod_id.as_str().to_string(),
            placement: rule.placement.into(),
            origin: rule.origin.into(),
            comment: rule.comment.clone(),
            promoted_from: None,
            already_promoted: false,
        }
    }
}

/// See [`fill_pair_promotion_fields`], for placement rules (keyed by
/// `mod_id` instead of `(after, before)`).
fn fill_placement_promotion_fields(placements: &mut [PlacementRuleDto]) {
    let snapshot: Vec<(String, RuleOriginDto)> = placements
        .iter()
        .map(|placement| (placement.mod_id.clone(), placement.origin))
        .collect();
    for placement in placements.iter_mut() {
        if placement.origin == RuleOriginDto::UserDecision {
            placement.promoted_from = snapshot
                .iter()
                .find(|(mod_id, origin)| {
                    *mod_id == placement.mod_id && *origin != RuleOriginDto::UserDecision
                })
                .map(|(_, origin)| *origin);
        } else {
            placement.already_promoted = snapshot.iter().any(|(mod_id, origin)| {
                *mod_id == placement.mod_id && *origin == RuleOriginDto::UserDecision
            });
        }
    }
}

/// Mirrors [`IncompatibleRule`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct IncompatibleRuleDto {
    /// One of the incompatible mods.
    pub a: String,
    /// The other.
    pub b: String,
    /// Where this rule came from.
    pub origin: RuleOriginDto,
}

impl From<&IncompatibleRule> for IncompatibleRuleDto {
    fn from(rule: &IncompatibleRule) -> Self {
        Self {
            a: rule.a.as_str().to_string(),
            b: rule.b.as_str().to_string(),
            origin: rule.origin.into(),
        }
    }
}

/// One rule, in whichever of its three shapes. Mirrors [`Rule`] and
/// [`rim_session::RuleKey`] (the `kind` tag doubles as the discriminant
/// `delete_rule` needs to build a [`rim_session::RuleKey`] back out of a
/// DTO the frontend only has the fields of, never the key itself).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RuleDto {
    /// See [`Rule::Pair`].
    Pair(PairRuleDto),
    /// See [`Rule::Placement`].
    Placement(PlacementRuleDto),
    /// See [`Rule::Incompatible`].
    Incompatible(IncompatibleRuleDto),
}

impl From<&Rule> for RuleDto {
    fn from(rule: &Rule) -> Self {
        match rule {
            Rule::Pair(r) => Self::Pair(r.into()),
            Rule::Placement(r) => Self::Placement(r.into()),
            Rule::Incompatible(r) => Self::Incompatible(r.into()),
        }
    }
}

impl TryFrom<RuleDto> for Rule {
    type Error = CommandError;

    fn try_from(value: RuleDto) -> Result<Self, Self::Error> {
        use rim_analyzer::domain::ModId;

        Ok(match value {
            RuleDto::Pair(r) => Self::Pair(PairRule {
                after: ModId::new(r.after),
                before: ModId::new(r.before),
                origin: r.origin.into(),
                comment: r.comment,
                overrides_declared: r.overrides_declared,
            }),
            RuleDto::Placement(r) => Self::Placement(PlacementRule {
                mod_id: ModId::new(r.mod_id),
                placement: r.placement.into(),
                origin: r.origin.into(),
                comment: r.comment,
            }),
            RuleDto::Incompatible(r) => Self::Incompatible(IncompatibleRule {
                a: ModId::new(r.a),
                b: ModId::new(r.b),
                origin: r.origin.into(),
            }),
        })
    }
}

/// Every rule and setting currently in effect. Mirrors [`StoredRules`]'s
/// three rule lists (tag rules/manual tags/settings have their own
/// commands).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RuleSetDto {
    /// Pair rules.
    pub pairs: Vec<PairRuleDto>,
    /// Placement rules.
    pub placements: Vec<PlacementRuleDto>,
    /// Incompatibility rules.
    pub incompatibles: Vec<IncompatibleRuleDto>,
    /// Non-fatal warnings noticed while loading `rules.json` — currently
    /// only ever the dropped-cluster-rules migration warning. Always
    /// empty for a session that never had one; never re-derived per call
    /// (see [`crate::commands::rules::rule_set_dto`]), since
    /// [`rim_session::Session::rule_load_warnings`] is set once, at load.
    pub warnings: Vec<String>,
}

impl From<&StoredRules> for RuleSetDto {
    fn from(rules: &StoredRules) -> Self {
        // `promoted_from`/`already_promoted` are computed here, over the
        // full unfiltered rule set — `list_rules`'s own per-origin filter
        // (`commands::rules::list_rules_inner`) retains a subset of the
        // `RuleSetDto` this produces, so a filtered view still shows the
        // right values for a row whose promoted/imported counterpart the
        // filter itself excluded.
        let mut pairs: Vec<PairRuleDto> = rules.pairs.iter().map(Into::into).collect();
        fill_pair_promotion_fields(&mut pairs);
        let mut placements: Vec<PlacementRuleDto> =
            rules.placements.iter().map(Into::into).collect();
        fill_placement_promotion_fields(&mut placements);
        Self {
            pairs,
            placements,
            incompatibles: rules.incompatibles.iter().map(Into::into).collect(),
            warnings: Vec::new(),
        }
    }
}

/// Request shape for `list_rules`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RuleFilterDto {
    /// Only rules from this origin.
    pub origin: Option<RuleOriginDto>,
}

/// Where RimSort's three database files live. Mirrors [`RimSortPaths`] —
/// `null` on a field means that source has no path for this import (no
/// RimSort install, or a blank field in the import dialog), a supported
/// state rather than an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct RimSortPathsDto {
    /// `userRules.json`, when this import includes it.
    pub user_rules: Option<String>,
    /// `communityRules.json`, when this import includes it.
    pub community_rules: Option<String>,
    /// `steamDB.json`, when this import includes it.
    pub steam_db: Option<String>,
}

impl From<RimSortPathsDto> for RimSortPaths {
    fn from(value: RimSortPathsDto) -> Self {
        Self {
            user_rules: value.user_rules.map(Into::into),
            community_rules: value.community_rules.map(Into::into),
            steam_db: value.steam_db.map(Into::into),
        }
    }
}

/// Identifies one rule for `delete_rule`, mirroring
/// [`rim_session::RuleKey`] — a `ruleId` string can't address a `Pair`/
/// `Incompatible` rule (neither has a single id field of its own; their
/// identity is the pair itself), so this carries exactly the fields each
/// shape's [`rim_session::RuleKey`] variant needs, never a full
/// [`RuleDto`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RuleKeyDto {
    /// See [`rim_session::RuleKey::Pair`].
    Pair {
        /// The dependent side.
        after: String,
        /// The dependency side.
        before: String,
    },
    /// See [`rim_session::RuleKey::Placement`].
    #[serde(rename_all = "camelCase")]
    Placement {
        /// The pinned mod.
        mod_id: String,
    },
    /// See [`rim_session::RuleKey::Incompatible`].
    Incompatible {
        /// One of the two mods.
        a: String,
        /// The other.
        b: String,
    },
}

/// Request shape for `delete_rule`. Carries the row's own [`RuleOriginDto`]
/// alongside its [`RuleKeyDto`] — passed through to
/// [`rim_session::use_cases::DeleteRule::execute`]'s own `origin` parameter
/// so deleting one row
/// never also removes a different-origin row sharing the same
/// [`rim_session::RuleKey`] (a promoted rule and its imported original,
/// most commonly). `None` deletes every row at the key regardless of
/// origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DeleteRuleRequestDto {
    /// The rule to delete.
    pub key: RuleKeyDto,
    /// The row's own origin, as currently displayed.
    pub origin: Option<RuleOriginDto>,
}

impl TryFrom<RuleKeyDto> for rim_session::RuleKey {
    type Error = CommandError;

    fn try_from(value: RuleKeyDto) -> Result<Self, Self::Error> {
        use rim_analyzer::domain::ModId;

        Ok(match value {
            RuleKeyDto::Pair { after, before } => Self::Pair {
                after: ModId::new(after),
                before: ModId::new(before),
            },
            RuleKeyDto::Placement { mod_id } => Self::Placement {
                mod_id: ModId::new(mod_id),
            },
            RuleKeyDto::Incompatible { a, b } => Self::Incompatible {
                a: ModId::new(a),
                b: ModId::new(b),
            },
        })
    }
}

/// What one RimSort import did. Mirrors [`ImportedRules`] — `skipped`
/// splits `skippedInactiveRules`/`skippedInactiveSteam` rather than the
/// plan's single `skippedInactive`/`warnings` fields: `ImportedRules`
/// itself reports no warnings and tracks the two skip counts separately
/// (see its own doc comment for why), so this DTO mirrors what the
/// use case actually returns instead of inventing a field it doesn't
/// have.
///
/// **`null` vs. `0` on the three count fields**
/// `null` means that
/// source was not part of this import (mirroring `ImportedRules`'s own
/// `None`) — a missing `userRules.json` must never render as an
/// imported zero. `0` means the source *was* imported and genuinely
/// found nothing to add. The `From` impl below preserves this
/// distinction rather than collapsing it with `unwrap_or(0)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ImportReportDto {
    /// Rules imported from `userRules.json`, `null` when it wasn't part
    /// of this import.
    pub user_rules: Option<usize>,
    /// Rules imported from `communityRules.json`, `null` when it wasn't
    /// part of this import.
    pub community_rules: Option<usize>,
    /// Pair rules derived from `steamDB.json`, `null` when it wasn't
    /// part of this import.
    pub steam_dependencies: Option<usize>,
    /// Relations skipped in `userRules.json`/`communityRules.json`.
    pub skipped_inactive_rules: usize,
    /// `steamDB.json` entries skipped.
    pub skipped_inactive_steam: usize,
}

impl From<&ImportedRules> for ImportReportDto {
    fn from(value: &ImportedRules) -> Self {
        Self {
            user_rules: value.user_rules.as_ref().map(Vec::len),
            community_rules: value.community_rules.as_ref().map(Vec::len),
            steam_dependencies: value.steam_dependencies.as_ref().map(Vec::len),
            skipped_inactive_rules: value.skipped_inactive_rules,
            skipped_inactive_steam: value.skipped_inactive_steam,
        }
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;

    use super::*;

    #[test]
    fn rule_dto_round_trips_a_pair_rule() {
        let rule = Rule::Pair(PairRule {
            after: ModId::new("a.mod"),
            before: ModId::new("b.mod"),
            origin: rim_resolve::domain::RuleOrigin::UserDecision,
            comment: Some("because".to_string()),
            overrides_declared: true,
        });
        let dto: RuleDto = (&rule).into();
        let back: Rule = dto.try_into().expect("valid rule dto");
        assert_eq!(rule, back);
    }

    #[test]
    fn rule_key_dto_maps_to_the_matching_rule_key_variant() {
        let key: rim_session::RuleKey = RuleKeyDto::Placement {
            mod_id: "a.mod".to_string(),
        }
        .try_into()
        .expect("valid key");
        assert_eq!(
            key,
            rim_session::RuleKey::Placement {
                mod_id: ModId::new("a.mod"),
            }
        );
    }

    #[test]
    fn import_report_dto_reports_counts_not_the_rules_themselves() {
        let imported = ImportedRules {
            user_rules: Some(vec![Rule::Pair(PairRule {
                after: ModId::new("a"),
                before: ModId::new("b"),
                origin: rim_resolve::domain::RuleOrigin::RimSortUser,
                comment: None,
                overrides_declared: false,
            })]),
            community_rules: Some(Vec::new()),
            steam_dependencies: Some(Vec::new()),
            skipped_inactive_rules: 3,
            skipped_inactive_steam: 5,
            provenance: std::collections::BTreeMap::new(),
        };
        let dto: ImportReportDto = (&imported).into();
        assert_eq!(dto.user_rules, Some(1));
        assert_eq!(dto.community_rules, Some(0));
        assert_eq!(dto.skipped_inactive_rules, 3);
        assert_eq!(dto.skipped_inactive_steam, 5);
    }

    /// A source that wasn't
    /// part of this import must render `null`, never `0` — a missing
    /// `userRules.json` must never read as an imported zero.
    #[test]
    fn import_report_dto_distinguishes_not_imported_from_an_imported_zero() {
        let imported = ImportedRules {
            user_rules: None,
            community_rules: Some(Vec::new()),
            steam_dependencies: None,
            skipped_inactive_rules: 0,
            skipped_inactive_steam: 0,
            provenance: std::collections::BTreeMap::new(),
        };
        let dto: ImportReportDto = (&imported).into();
        assert_eq!(dto.user_rules, None, "not imported must be null, not 0");
        assert_eq!(
            dto.community_rules,
            Some(0),
            "imported and empty must be Some(0), not null"
        );
        assert_eq!(dto.steam_dependencies, None);
    }

    fn stored_pair(after: &str, before: &str, origin: rim_resolve::domain::RuleOrigin) -> PairRule {
        PairRule {
            after: ModId::new(after),
            before: ModId::new(before),
            origin,
            comment: None,
            overrides_declared: false,
        }
    }

    fn stored_placement(mod_id: &str, origin: rim_resolve::domain::RuleOrigin) -> PlacementRule {
        PlacementRule {
            mod_id: ModId::new(mod_id),
            placement: rim_resolve::domain::Placement::Bottom,
            origin,
            comment: None,
        }
    }

    /// `promoted_from`/`already_promoted`
    /// must be computed over the *full* rule set, before `list_rules`'s own
    /// per-origin filter runs — a filtered view built straight from a
    /// pre-filtered `Vec<PairRuleDto>` could never see a promoted row's own
    /// imported original (or vice versa) once the filter had already
    /// dropped it. `RuleSetDto::from(&StoredRules)` is exactly the point
    /// that still sees everything, so this test exercises it directly
    /// rather than the two filler functions in isolation.
    #[test]
    fn rule_set_dto_computes_promotion_fields_over_the_full_pair_list() {
        use rim_resolve::domain::RuleOrigin;

        let stored = StoredRules {
            pairs: vec![
                stored_pair("a", "b", RuleOrigin::RimSortCommunity),
                stored_pair("a", "b", RuleOrigin::UserDecision),
                stored_pair("c", "d", RuleOrigin::RimSortUser),
            ],
            ..StoredRules::default()
        };

        let dto: RuleSetDto = (&stored).into();

        let imported = dto
            .pairs
            .iter()
            .find(|p| p.origin == RuleOriginDto::RimSortCommunity)
            .expect("imported row present");
        assert!(
            imported.already_promoted,
            "a userDecision copy exists at the same key"
        );
        assert_eq!(imported.promoted_from, None);

        let promoted = dto
            .pairs
            .iter()
            .find(|p| p.origin == RuleOriginDto::UserDecision)
            .expect("promoted row present");
        assert_eq!(
            promoted.promoted_from,
            Some(RuleOriginDto::RimSortCommunity)
        );
        assert!(!promoted.already_promoted);

        let untouched = dto
            .pairs
            .iter()
            .find(|p| p.origin == RuleOriginDto::RimSortUser)
            .expect("untouched row present");
        assert!(!untouched.already_promoted);
        assert_eq!(untouched.promoted_from, None);
    }

    /// A hand-written user rule that was never promoted from an import
    /// gets neither field set, even though its origin is `userDecision`.
    #[test]
    fn rule_set_dto_leaves_a_hand_written_user_pair_unpromoted() {
        use rim_resolve::domain::RuleOrigin;

        let stored = StoredRules {
            pairs: vec![stored_pair("a", "b", RuleOrigin::UserDecision)],
            ..StoredRules::default()
        };

        let dto: RuleSetDto = (&stored).into();

        assert_eq!(dto.pairs[0].promoted_from, None);
        assert!(!dto.pairs[0].already_promoted);
    }

    /// See [`rule_set_dto_computes_promotion_fields_over_the_full_pair_list`],
    /// for placement rules.
    #[test]
    fn rule_set_dto_computes_promotion_fields_over_the_full_placement_list() {
        use rim_resolve::domain::RuleOrigin;

        let stored = StoredRules {
            placements: vec![
                stored_placement("a", RuleOrigin::RimSortUser),
                stored_placement("a", RuleOrigin::UserDecision),
            ],
            ..StoredRules::default()
        };

        let dto: RuleSetDto = (&stored).into();

        let imported = dto
            .placements
            .iter()
            .find(|p| p.origin == RuleOriginDto::RimSortUser)
            .expect("imported row present");
        assert!(imported.already_promoted);

        let promoted = dto
            .placements
            .iter()
            .find(|p| p.origin == RuleOriginDto::UserDecision)
            .expect("promoted row present");
        assert_eq!(promoted.promoted_from, Some(RuleOriginDto::RimSortUser));
    }
}
