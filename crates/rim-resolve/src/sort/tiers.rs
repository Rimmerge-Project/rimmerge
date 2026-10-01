//! [`Tier`] and tier assignment: which of the five fixed load-order bands
//! (`Core -> Dlc -> Top -> Body -> Bottom`) each mod starts in, before any
//! edge has a chance to promote it out.
//!
//! There is no `Framework` tier: `Mod::is_framework_candidate` never
//! assigns one (the field stays on `Mod` — the UI surfaces it). A mod that
//! merely *looks* framework-shaped (many hard dependents) is not
//! necessarily one the user wants pulled ahead of everything else, and
//! doing so unconditionally mis-promotes real content mods purely because
//! they happen to have many declared dependents. A framework mod's early
//! position comes only
//! from what it actually earns: real `Hard`/`Declared` edges pointing at
//! it, or the propagated emission key (see [`super::emit`]) pulling it
//! ahead of its dependents.

use std::collections::BTreeMap;

use rim_analyzer::domain::{Mod, ModId, Source};

use super::Layer;
use super::explain::TierReason;
use super::graph::{GraphEdge, GraphEdgeKind, Indices, SortGraph};
use crate::domain::{Placement, RuleOrigin, RuleSet};

/// One of the five fixed bands the load order is divided into, in the
/// physical order they're emitted: every `Core` mod loads before every
/// `Dlc` mod, before every `Top` mod, and so on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// Vanilla RimWorld (`Source::Core`).
    Core,
    /// An official DLC (`Source::Dlc`).
    Dlc,
    /// Pinned to the top via a [`super::super::domain::PlacementRule`].
    Top,
    /// Everything else.
    Body,
    /// Pinned to the bottom via a
    /// [`super::super::domain::PlacementRule`].
    Bottom,
}

impl Tier {
    /// Every tier, in physical load-order sequence.
    pub const ALL: [Tier; 5] = [Tier::Core, Tier::Dlc, Tier::Top, Tier::Body, Tier::Bottom];
}

/// Assigns every mod's starting tier: `Source::Core` -> `Core`;
/// `Source::Dlc` -> `Dlc`;
/// `PlacementRule::Top` -> `Top`; `Bottom` -> `Bottom`; else `Body`.
///
/// A mod named by more than one placement rule uses the first match in
/// `rules`' own order (already precedence-sorted by
/// [`RuleSet::merged`] — a user's own decision beats an imported one).
#[must_use]
pub fn assign(
    mods_by_id: &BTreeMap<ModId, &Mod>,
    rules: &RuleSet,
) -> BTreeMap<ModId, (Tier, TierReason)> {
    let mut placements: BTreeMap<ModId, (Placement, crate::domain::RuleOrigin)> = BTreeMap::new();
    for rule in rules.placements() {
        let base = rule.mod_id.base();
        placements
            .entry(base)
            .or_insert((rule.placement, rule.origin));
    }

    mods_by_id
        .iter()
        .map(|(id, mod_entry)| (id.clone(), assign_one(mod_entry, &placements)))
        .collect()
}

fn assign_one(
    mod_entry: &Mod,
    placements: &BTreeMap<ModId, (Placement, crate::domain::RuleOrigin)>,
) -> (Tier, TierReason) {
    if mod_entry.source == Source::Core {
        return (Tier::Core, TierReason::Source(Source::Core));
    }
    if mod_entry.source == Source::Dlc {
        return (Tier::Dlc, TierReason::Source(Source::Dlc));
    }
    if let Some((placement, origin)) = placements.get(&mod_entry.id.base()) {
        let tier = match placement {
            Placement::Top => Tier::Top,
            Placement::Bottom => Tier::Bottom,
        };
        return (tier, TierReason::Placement(*origin));
    }
    (Tier::Body, TierReason::Body)
}

fn layer_of_origin(origin: RuleOrigin) -> Layer {
    match origin {
        RuleOrigin::UserDecision => Layer::UserDecision,
        RuleOrigin::RimSortUser => Layer::RimSortUser,
        RuleOrigin::RimSortCommunity => Layer::RimSortCommunity,
        RuleOrigin::SteamDb => Layer::SteamDb,
    }
}

/// This tier's membership-edge layer: how tightly a mod is bound to it,
/// and therefore how strong an opposing edge must be to promote the mod
/// out (see [`super::EdgeProvenance::Tier`]). `Body` binds at
/// [`Layer::Awareness`] — the weakest layer — so any real edge, however
/// weak, can still pull
/// a mod out of the default tier.
fn membership_layer(tier: Tier, reason: &TierReason) -> Layer {
    match tier {
        Tier::Core | Tier::Dlc => Layer::Hard,
        Tier::Body => Layer::Awareness,
        Tier::Top | Tier::Bottom => match reason {
            TierReason::Placement(origin) => layer_of_origin(*origin),
            // Only `assign_one` ever produces `Top`/`Bottom` alongside a
            // `Placement` reason; any other pairing is a bug in this
            // module, not a real input to guard against.
            _ => Layer::UserDecision,
        },
    }
}

/// Adds `layer`'s tier sentinel edges to `graph` — called once per layer,
/// in the same precedence-ordered pass that adds that layer's real edges,
/// so [`super::cycles::break_cycles`]'s "any cycle found now includes an
/// edge from the layer just added" invariant holds for sentinel edges too.
///
/// Without this — if every sentinel edge were added upfront regardless of
/// its own layer — a mod's weak tier-membership edge could sit in the
/// graph for several layers before the real edge that actually conflicts
/// with it arrives, and by the time a cycle closes, cycle-breaking would
/// only ever be allowed to blame the *newest* layer's edge, never the
/// long-since-added (and rightfully weaker) sentinel — the opposite of
/// "a stronger edge promotes the mod out of its tier".
///
/// The boundary chain (`T_end(tier) -> T_start(next tier)`) is always
/// [`Layer::Hard`], so it's added once, during the `Hard` pass.
pub(super) fn add_sentinel_edges_for_layer(
    graph: &mut SortGraph,
    indices: &Indices,
    tier_of: &BTreeMap<ModId, (Tier, TierReason)>,
    layer: Layer,
) {
    for (mod_id, (tier, reason)) in tier_of {
        if membership_layer(*tier, reason) != layer {
            continue;
        }
        let mod_idx = indices.mod_index[mod_id];
        let start_idx = indices.tier_start[tier];
        let end_idx = indices.tier_end[tier];
        graph.add_edge(
            start_idx,
            mod_idx,
            GraphEdge {
                layer,
                kind: GraphEdgeKind::Membership {
                    mod_id: mod_id.clone(),
                    tier: *tier,
                },
            },
        );
        graph.add_edge(
            mod_idx,
            end_idx,
            GraphEdge {
                layer,
                kind: GraphEdgeKind::Membership {
                    mod_id: mod_id.clone(),
                    tier: *tier,
                },
            },
        );
    }

    if layer == Layer::Hard {
        for pair in Tier::ALL.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            graph.add_edge(
                indices.tier_end[&from],
                indices.tier_start[&to],
                GraphEdge {
                    layer: Layer::Hard,
                    kind: GraphEdgeKind::Boundary,
                },
            );
        }
        // A direct `T_start -> T_end` passthrough for every tier: without
        // it, a tier with zero members would leave `T_end` with no
        // incoming edge at all (no member edges to wait on), so it — and
        // everything chained after it — would be "ready" from the very
        // start, collapsing the whole boundary chain. Redundant, and
        // harmless, when the tier does have members.
        for tier in Tier::ALL {
            graph.add_edge(
                indices.tier_start[&tier],
                indices.tier_end[&tier],
                GraphEdge {
                    layer: Layer::Hard,
                    kind: GraphEdgeKind::Boundary,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rim_analyzer::domain::DeclaredOrder;

    use super::*;
    use crate::domain::{PlacementRule, RuleOrigin};

    fn plain_mod(id: &str, source: Source) -> Mod {
        Mod {
            id: ModId::new(id),
            name: id.to_string(),
            authors: Vec::new(),
            url: None,
            path: PathBuf::new(),
            source,
            supported_versions: Vec::new(),
            declared: DeclaredOrder::default(),
            loaded_folders: Vec::new(),
            hard_dependents: 0,
            soft_dependents: 0,
            awareness_dependents: 0,
            is_framework_candidate: false,
            generated: None,
            workshop_id: None,
            load_folders_version_matched: None,
        }
    }

    #[test]
    fn core_and_dlc_sources_win_regardless_of_other_facts() {
        let core = plain_mod("ludeon.rimworld", Source::Core);
        let dlc = plain_mod("ludeon.royalty", Source::Dlc);
        let mods = BTreeMap::from([(core.id.clone(), &core), (dlc.id.clone(), &dlc)]);

        let assigned = assign(&mods, &RuleSet::default());

        assert_eq!(assigned[&core.id].0, Tier::Core);
        assert_eq!(assigned[&dlc.id].0, Tier::Dlc);
    }

    #[test]
    fn framework_candidacy_no_longer_influences_tier_assignment() {
        // `is_framework_candidate` stays on `Mod` for the UI, but sorting
        // itself must treat such a mod exactly like any other Body mod —
        // the whole point of removing the `Framework` tier.
        let mut fw = plain_mod("framework.mod", Source::Local);
        fw.is_framework_candidate = true;
        fw.hard_dependents = 12;
        let mods = BTreeMap::from([(fw.id.clone(), &fw)]);

        let assigned = assign(&mods, &RuleSet::default());

        assert_eq!(assigned[&fw.id], (Tier::Body, TierReason::Body));
    }

    #[test]
    fn placement_rule_applies_regardless_of_framework_candidacy() {
        let mut fw = plain_mod("framework.mod", Source::Local);
        fw.is_framework_candidate = true;
        let mods = BTreeMap::from([(fw.id.clone(), &fw)]);
        let rules = RuleSet::new(vec![crate::domain::Rule::Placement(PlacementRule {
            mod_id: ModId::new("framework.mod"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: None,
        })]);

        let assigned = assign(&mods, &rules);

        assert_eq!(assigned[&fw.id].0, Tier::Top);
        assert_eq!(
            assigned[&fw.id].1,
            TierReason::Placement(RuleOrigin::UserDecision)
        );
    }

    #[test]
    fn placement_rule_matches_through_the_steam_suffix() {
        let plain = plain_mod("some.mod_steam", Source::Local);
        let mods = BTreeMap::from([(plain.id.clone(), &plain)]);
        let rules = RuleSet::new(vec![crate::domain::Rule::Placement(PlacementRule {
            mod_id: ModId::new("some.mod"),
            placement: Placement::Bottom,
            origin: RuleOrigin::RimSortUser,
            comment: None,
        })]);

        let assigned = assign(&mods, &rules);

        assert_eq!(assigned[&plain.id].0, Tier::Bottom);
    }

    #[test]
    fn everything_else_defaults_to_body() {
        let plain = plain_mod("plain.mod", Source::Local);
        let mods = BTreeMap::from([(plain.id.clone(), &plain)]);

        let assigned = assign(&mods, &RuleSet::default());

        assert_eq!(assigned[&plain.id], (Tier::Body, TierReason::Body));
    }
}
