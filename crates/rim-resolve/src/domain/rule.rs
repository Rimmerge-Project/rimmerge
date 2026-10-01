//! Rules: user- and community-sourced load-order constraints, merged
//! into one [`RuleSet`] before the sorter runs.

use std::collections::BTreeSet;
use std::fmt;

use rim_analyzer::domain::{EdgeKind, ModId, Report};
use serde::{Deserialize, Serialize};

use super::tag::is_valid_slug;

/// Where a [`Rule`] came from, and — via its `Ord` implementation — the
/// precedence [`RuleSet::merged`] merges by: a user's own decision always
/// outranks an imported one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleOrigin {
    /// The user accepted a suggestion or manually reordered two mods.
    UserDecision,
    /// Imported from RimSort's `userRules.json`.
    RimSortUser,
    /// Imported from RimSort's community `communityRules.json`.
    RimSortCommunity,
    /// Imported from RimSort's `steamDB.json` dependency data.
    SteamDb,
}

/// One mod must load after another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRule {
    /// The mod that must load after `before`.
    pub after: ModId,
    /// The mod that must load before `after`.
    pub before: ModId,
    /// Where this rule came from.
    pub origin: RuleOrigin,
    /// A free-text note (e.g. imported from RimSort, or the user's own
    /// reasoning).
    pub comment: Option<String>,
    /// The declared-edge override (the deferred half of the pair-rule
    /// feature): when `true` **and** `origin` is
    /// [`RuleOrigin::UserDecision`], this rule's edge is added at
    /// `crate::sort::Layer::DeclaredOverride` — a layer sitting between
    /// `AnyOf` and `Declared` — instead of the ordinary
    /// `Layer::UserDecision` an unflagged `UserDecision` row uses, so it
    /// wins a cycle against an author's own `loadAfter`/`modDependencies`
    /// edge instead of losing to it. Ignored for every other origin: an
    /// imported (`RimSortUser`/`RimSortCommunity`/`SteamDb`) row can carry
    /// the field (round-tripping it if one somehow does) but it never
    /// changes that row's own layer — only a user's own, explicit,
    /// per-pair opt-in may outrank a declaration, never a one-click
    /// "promote" of a database rule. `#[serde(default)]`: additive to an
    /// existing `rules.json` struct, so no schema-version bump — the same
    /// precedent `suggest_merge_when_clean` set (`crates/rim-io/CLAUDE.md`).
    #[serde(default)]
    pub overrides_declared: bool,
}

/// How a [`PairRule`]'s own claimed order relates to the derived edges
/// already connecting the same two mods in [`Report::edges`], used to
/// break an imported rule set
/// into what the analyzer would derive anyway versus what only the
/// mods' own community knowledge states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairRuleEvidence {
    /// At least one derived edge between the same two mods already
    /// points the rule's own way — the rule states a fact the sorter
    /// would enforce (or, for an `Awareness`-only kind, at least be aware
    /// of) regardless of whether the rule exists. Every edge kind
    /// covering it in that direction (a rule can be covered by more than
    /// one).
    Redundant(BTreeSet<EdgeKind>),
    /// No edge agrees, but at least one points the opposite way — the
    /// rule and the analyzer's own evidence disagree. Every edge kind
    /// opposing it. In practice this only ever happens against an
    /// `Awareness`-strength kind (`MayRequire`, `PatchTargetsDef`, ...):
    /// a `Hard`/`Declared` edge opposing an imported pair rule instead
    /// surfaces as [`super::FindingKey::EdgeDropped`]/`DeclarationQuestioned`
    /// once the rule is fed to the sorter, since those *are* enforced and
    /// something has to give.
    ///
    /// `ParentTemplate` is `Hard`, so it is never the opposing kind here;
    /// see `apps/cli/tests/real_install_pair_rules.rs` for the real-install
    /// check.
    Contradicting(BTreeSet<EdgeKind>),
    /// No derived edge connects the two mods in either direction — the
    /// rule states something no file-level fact reconstructs, the same
    /// category a [`PlacementRule`] already occupies.
    Novel,
}

/// Classifies `rule` against every edge `report` already derives between
/// its two mods. Redundant takes priority over contradicting when a pair
/// somehow carries edges both ways (e.g. two different edge kinds
/// disagreeing with each other, not just with the rule) — a rule that
/// agrees with *any* derived evidence is not what "novel" is meant to
/// flag, and the sorter's own tie-breaking, not this classification,
/// decides which edge wins when edges themselves conflict.
#[must_use]
pub fn classify_pair_rule(rule: &PairRule, report: &Report) -> PairRuleEvidence {
    let mut same_direction = BTreeSet::new();
    let mut opposite_direction = BTreeSet::new();
    for edge_report in &report.edges {
        let edge = &edge_report.edge;
        if edge.after == rule.after && edge.before == rule.before {
            same_direction.insert(edge.kind);
        } else if edge.after == rule.before && edge.before == rule.after {
            opposite_direction.insert(edge.kind);
        }
    }
    if !same_direction.is_empty() {
        PairRuleEvidence::Redundant(same_direction)
    } else if !opposite_direction.is_empty() {
        PairRuleEvidence::Contradicting(opposite_direction)
    } else {
        PairRuleEvidence::Novel
    }
}

/// Where a [`PlacementRule`] pins a mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// The top tier of the load order.
    Top,
    /// The bottom tier of the load order.
    Bottom,
}

/// One mod is pinned to the top or bottom tier of the load order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementRule {
    /// The pinned mod.
    pub mod_id: ModId,
    /// Which tier it's pinned to.
    pub placement: Placement,
    /// Where this rule came from.
    pub origin: RuleOrigin,
    /// A free-text note.
    pub comment: Option<String>,
}

/// Two mods declared incompatible with each other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncompatibleRule {
    /// One of the incompatible mods.
    pub a: ModId,
    /// The other.
    pub b: ModId,
    /// Where this rule came from.
    pub origin: RuleOrigin,
}

/// A validated, non-empty slug identifying a [`ClusterRule`]: lowercase
/// `[a-z0-9_-]+`, e.g. `"framework"`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ClusterRuleId(String);

/// [`ClusterRuleId::new`] rejects text that isn't non-empty lowercase
/// `[a-z0-9_-]+`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid cluster rule id {0:?}: must be non-empty lowercase [a-z0-9_-]+")]
pub struct ClusterRuleIdError(String);

impl ClusterRuleId {
    /// Validates and constructs a cluster rule id.
    ///
    /// # Errors
    ///
    /// Returns [`ClusterRuleIdError`] when `raw` is empty or contains
    /// anything outside lowercase ascii letters, digits, `_`, and `-`.
    pub fn new(raw: impl Into<String>) -> Result<Self, ClusterRuleIdError> {
        let raw = raw.into();
        if is_valid_slug(&raw) {
            Ok(Self(raw))
        } else {
            Err(ClusterRuleIdError(raw))
        }
    }

    /// The id's normalized text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ClusterRuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ClusterRuleId {
    type Error = ClusterRuleIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ClusterRuleId> for String {
    fn from(value: ClusterRuleId) -> Self {
        value.0
    }
}

/// A single load-order rule, in whichever of its three shapes.
///
/// There is no cluster (tag-based contiguity) rule: contiguity would
/// replace a cluster's own member order with alphabetical-by-id and drag
/// an anchor to the block's median, and an anchor-only form is just pair
/// rules generated from a tag, which a user can write by hand.
/// [`ClusterRuleId`] exists only as the
/// payload type for [`super::resolution::Action::ExcludeFromCluster`],
/// kept uninhabited-in-practice rather than removed outright so an
/// already-persisted `decisions.json`/`rules.json` record naming one
/// doesn't fail to deserialize (see `crates/rim-resolve/CLAUDE.md`'s
/// file-format rule for `Action`'s wire format).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Rule {
    /// See [`PairRule`].
    Pair(PairRule),
    /// See [`PlacementRule`].
    Placement(PlacementRule),
    /// See [`IncompatibleRule`].
    Incompatible(IncompatibleRule),
}

impl Rule {
    /// This rule's origin: stored per-rule for every shape.
    #[must_use]
    pub fn origin(&self) -> RuleOrigin {
        match self {
            Self::Pair(rule) => rule.origin,
            Self::Placement(rule) => rule.origin,
            Self::Incompatible(rule) => rule.origin,
        }
    }
}

/// All rules in effect, in the stable order [`RuleSet::merged`] produces:
/// origin precedence first, then insertion order.
///
/// Pair/placement lookups should compare [`ModId::base`] so `_steam`
/// duplicates never miss — callers of [`RuleSet::pairs`]/
/// [`RuleSet::placements`] are responsible for that comparison; the set
/// itself stores rules exactly as given.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSet {
    rules: Vec<Rule>,
}

impl RuleSet {
    /// Builds a rule set from an already-ordered list of rules.
    #[must_use]
    pub fn new(rules: Vec<Rule>) -> Self {
        Self { rules }
    }

    /// Every rule, in this set's current order.
    pub fn iter(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter()
    }

    /// The pair rules in this set, in order.
    pub fn pairs(&self) -> impl Iterator<Item = &PairRule> {
        self.rules.iter().filter_map(|rule| match rule {
            Rule::Pair(pair) => Some(pair),
            _ => None,
        })
    }

    /// The placement rules in this set, in order.
    pub fn placements(&self) -> impl Iterator<Item = &PlacementRule> {
        self.rules.iter().filter_map(|rule| match rule {
            Rule::Placement(placement) => Some(placement),
            _ => None,
        })
    }

    /// The incompatibility rules in this set, in order.
    pub fn incompatibles(&self) -> impl Iterator<Item = &IncompatibleRule> {
        self.rules.iter().filter_map(|rule| match rule {
            Rule::Incompatible(incompatible) => Some(incompatible),
            _ => None,
        })
    }

    /// Whether this set has no rules.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The number of rules in this set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Merges several rule sets into one, in a stable order: every
    /// [`RuleOrigin::UserDecision`] rule first, then `RimSortUser`, then
    /// `RimSortCommunity`, then `SteamDb` — ties within an origin keep
    /// the order the rules were given in (each input set's own order,
    /// sets concatenated in the order given).
    #[must_use]
    pub fn merged(sets: impl IntoIterator<Item = RuleSet>) -> RuleSet {
        let mut rules: Vec<Rule> = sets.into_iter().flat_map(|set| set.rules).collect();
        rules.sort_by_key(Rule::origin);
        RuleSet { rules }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(after: &str, before: &str, origin: RuleOrigin) -> Rule {
        Rule::Pair(PairRule {
            after: ModId::new(after),
            before: ModId::new(before),
            origin,
            comment: None,
            overrides_declared: false,
        })
    }

    fn pair_rule(after: &str, before: &str, origin: RuleOrigin) -> PairRule {
        PairRule {
            after: ModId::new(after),
            before: ModId::new(before),
            origin,
            comment: None,
            overrides_declared: false,
        }
    }

    #[test]
    fn a_pair_rule_matching_a_derived_edge_is_redundant() {
        let report = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .mod_("b")
            .declared_edge("a", "b") // a after b: same direction as the rule below.
            .build();
        let rule = pair_rule("a", "b", RuleOrigin::SteamDb);

        assert_eq!(
            classify_pair_rule(&rule, &report),
            PairRuleEvidence::Redundant(BTreeSet::from([EdgeKind::LoadAfter]))
        );
    }

    #[test]
    fn a_pair_rule_covered_by_more_than_one_edge_kind_names_every_kind() {
        let report = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .mod_("b")
            .declared_edge("a", "b")
            .hard_edge("a", "b")
            .build();
        let rule = pair_rule("a", "b", RuleOrigin::RimSortCommunity);

        assert_eq!(
            classify_pair_rule(&rule, &report),
            PairRuleEvidence::Redundant(BTreeSet::from([
                EdgeKind::AssemblyRef,
                EdgeKind::LoadAfter
            ]))
        );
    }

    #[test]
    fn a_pair_rule_opposing_a_derived_edge_with_nothing_agreeing_is_contradicting() {
        let report = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .mod_("b")
            .awareness_edge("b", "a") // b after a: the opposite direction.
            .build();
        let rule = pair_rule("a", "b", RuleOrigin::SteamDb); // a after b.

        assert_eq!(
            classify_pair_rule(&rule, &report),
            PairRuleEvidence::Contradicting(BTreeSet::from([EdgeKind::MayRequire]))
        );
    }

    #[test]
    fn a_pair_rule_with_no_connecting_edge_is_novel() {
        let report = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .mod_("b")
            .build();
        let rule = pair_rule("a", "b", RuleOrigin::RimSortCommunity);

        assert_eq!(classify_pair_rule(&rule, &report), PairRuleEvidence::Novel);
    }

    #[test]
    fn cluster_rule_id_accepts_a_valid_slug() {
        assert_eq!(
            ClusterRuleId::new("framework").unwrap().as_str(),
            "framework"
        );
    }

    #[test]
    fn cluster_rule_id_rejects_uppercase_and_spaces() {
        assert!(ClusterRuleId::new("FrameWork").is_err());
        assert!(ClusterRuleId::new("lovers lab").is_err());
        assert!(ClusterRuleId::new("").is_err());
    }

    #[test]
    fn merged_orders_rules_by_origin_precedence() {
        let community = RuleSet::new(vec![pair("a", "b", RuleOrigin::RimSortCommunity)]);
        let user_decision = RuleSet::new(vec![pair("c", "d", RuleOrigin::UserDecision)]);
        let steam_db = RuleSet::new(vec![pair("e", "f", RuleOrigin::SteamDb)]);

        let merged = RuleSet::merged([community, user_decision, steam_db]);

        let origins: Vec<RuleOrigin> = merged.iter().map(Rule::origin).collect();
        assert_eq!(
            origins,
            vec![
                RuleOrigin::UserDecision,
                RuleOrigin::RimSortCommunity,
                RuleOrigin::SteamDb,
            ]
        );
    }

    #[test]
    fn merged_keeps_insertion_order_within_the_same_origin() {
        let first = RuleSet::new(vec![pair("a", "b", RuleOrigin::RimSortUser)]);
        let second = RuleSet::new(vec![
            pair("c", "d", RuleOrigin::RimSortUser),
            pair("e", "f", RuleOrigin::RimSortUser),
        ]);

        let merged = RuleSet::merged([first, second]);

        let afters: Vec<ModId> = merged.pairs().map(|rule| rule.after.clone()).collect();
        assert_eq!(
            afters,
            vec![ModId::new("a"), ModId::new("c"), ModId::new("e")],
            "a stable sort must preserve the concatenation order for equal keys"
        );
    }

    #[test]
    fn typed_accessors_filter_to_their_own_variant() {
        let set = RuleSet::new(vec![
            pair("a", "b", RuleOrigin::UserDecision),
            Rule::Incompatible(IncompatibleRule {
                a: ModId::new("x"),
                b: ModId::new("y"),
                origin: RuleOrigin::UserDecision,
            }),
        ]);

        assert_eq!(set.pairs().count(), 1);
        assert_eq!(set.incompatibles().count(), 1);
        assert_eq!(set.placements().count(), 0);
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());
    }
}
