//! [`OrderRow`]: one row of the load-order page, built by
//! [`crate::Session::order_rows`]. The join itself is pure: it reads facts
//! the session already has (the order, the sorter's placements, the report,
//! the tagging and the needs-input counts) and does no I/O.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{LoadOrder, ModId, Report};
use rim_resolve::domain::{Tag, Tagging};
use rim_resolve::sort::{PlacementExplanation, Tier};

/// One mod in an order, with the facts the load-order page shows for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderRow {
    /// The mod's id, exactly as the order holds it.
    pub mod_id: ModId,
    /// Its display name, empty when the report has no row for it.
    pub name: String,
    /// Its zero-based position in this order.
    pub position: usize,
    /// Its position in the current order. `None` when this order is the
    /// current order itself, or when the mod is not in the current order.
    pub previous_position: Option<usize>,
    /// The tier the sorter assigned it, [`Tier::Body`] when the sorter has
    /// no placement for it.
    pub tier: Tier,
    /// Every tag it currently carries.
    pub tags: BTreeSet<Tag>,
    /// Distinct active mods with a Hard-strength edge pointing at it, `0`
    /// when the report has no row for it.
    pub hard_dependents: usize,
    /// How many live findings naming its base id still need input.
    pub needs_input_count: usize,
}

/// Everything [`order_rows`] joins. Borrowed from the session, so building
/// the rows clones only what each row keeps.
pub(crate) struct OrderRowFacts<'session> {
    /// The order to list.
    pub(crate) order: &'session LoadOrder,
    /// The current order to compare positions with, `None` when `order`
    /// is the current order itself.
    pub(crate) previous: Option<&'session LoadOrder>,
    /// The sorter's placements, keyed by exact id.
    pub(crate) placements: &'session BTreeMap<ModId, PlacementExplanation>,
    /// The scan report, for names and hard-dependent counts.
    pub(crate) report: &'session Report,
    /// The session's tagging.
    pub(crate) tagging: &'session Tagging,
    /// Needs-input counts keyed by [`ModId::base`].
    pub(crate) needs_input_by_mod: &'session BTreeMap<ModId, usize>,
}

/// One [`OrderRow`] per mod in `facts.order`, in that order.
pub(crate) fn order_rows(facts: &OrderRowFacts<'_>) -> Vec<OrderRow> {
    // Keyed maps, not a per-row search: an order holds every active mod,
    // so a linear lookup per row would be quadratic on a large install.
    let names: BTreeMap<&ModId, &str> = facts
        .report
        .mods
        .iter()
        .map(|entry| (&entry.id, entry.name.as_str()))
        .collect();
    let hard_dependents: BTreeMap<&ModId, usize> = facts
        .report
        .mods
        .iter()
        .map(|entry| (&entry.id, entry.hard_dependents))
        .collect();

    facts
        .order
        .as_slice()
        .iter()
        .enumerate()
        .map(|(position, mod_id)| OrderRow {
            mod_id: mod_id.clone(),
            name: names
                .get(mod_id)
                .map(|name| (*name).to_string())
                .unwrap_or_default(),
            position,
            previous_position: facts
                .previous
                .and_then(|previous| previous.position(mod_id)),
            tier: facts
                .placements
                .get(mod_id)
                .map_or(Tier::Body, |placement| placement.tier),
            tags: facts.tagging.tags_of(mod_id).clone(),
            hard_dependents: hard_dependents.get(mod_id).copied().unwrap_or_default(),
            needs_input_count: facts
                .needs_input_by_mod
                .get(&mod_id.base())
                .copied()
                .unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use rim_resolve::test_support::ReportBuilder;

    use super::*;

    fn order_of(ids: &[&str]) -> LoadOrder {
        LoadOrder::new(ids.iter().map(|id| ModId::new(*id)).collect())
    }

    #[test]
    fn a_mod_without_a_placement_defaults_to_the_body_tier() {
        let report = ReportBuilder::new().mod_("a").build();
        let order = order_of(&["a"]);

        let rows = order_rows(&OrderRowFacts {
            order: &order,
            previous: None,
            placements: &BTreeMap::new(),
            report: &report,
            tagging: &Tagging::default(),
            needs_input_by_mod: &BTreeMap::new(),
        });

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tier, Tier::Body);
    }

    #[test]
    fn a_mod_missing_from_the_previous_order_has_no_previous_position() {
        let report = ReportBuilder::new().mod_("a").mod_("b").build();
        let order = order_of(&["b", "a"]);
        let previous = order_of(&["a"]);

        let rows = order_rows(&OrderRowFacts {
            order: &order,
            previous: Some(&previous),
            placements: &BTreeMap::new(),
            report: &report,
            tagging: &Tagging::default(),
            needs_input_by_mod: &BTreeMap::new(),
        });

        assert_eq!(rows[0].mod_id, ModId::new("b"));
        assert_eq!(rows[0].previous_position, None);
        assert_eq!(rows[1].mod_id, ModId::new("a"));
        assert_eq!(rows[1].previous_position, Some(0));
    }

    #[test]
    fn a_mod_the_report_lacks_gets_an_empty_name_and_no_hard_dependents() {
        let report = ReportBuilder::new()
            .mod_("a")
            .hard_dependents("a", 3)
            .build();
        let order = order_of(&["a", "ghost"]);

        let rows = order_rows(&OrderRowFacts {
            order: &order,
            previous: None,
            placements: &BTreeMap::new(),
            report: &report,
            tagging: &Tagging::default(),
            needs_input_by_mod: &BTreeMap::new(),
        });

        assert_eq!(rows[0].name, "a");
        assert_eq!(rows[0].hard_dependents, 3);
        assert_eq!(rows[1].name, "");
        assert_eq!(rows[1].hard_dependents, 0);
    }

    #[test]
    fn a_row_carries_its_placement_tier_and_its_tags() {
        let report = ReportBuilder::new().mod_("a").build();
        let order = order_of(&["a"]);
        let id = ModId::new("a");
        let placements = BTreeMap::from([(
            id.clone(),
            PlacementExplanation {
                mod_id: id.clone(),
                position: 0,
                previous_position: None,
                tier: Tier::Top,
                tier_reason: rim_resolve::sort::TierReason::Body,
                became_ready_after: None,
                lower_bounds: Vec::new(),
                upper_bounds: Vec::new(),
                dropped: Vec::new(),
                advisory: Vec::new(),
                tie_break: rim_resolve::sort::PlacementTieBreak {
                    current_position: None,
                    effective_key: 0,
                    pulled_forward_by: None,
                    mods_preferred_ahead: 0,
                },
            },
        )]);
        let tag = Tag::new("library").expect("a valid tag slug");
        let tagging = Tagging::new(vec![rim_resolve::domain::TagAssignment {
            mod_id: id.clone(),
            tag: tag.clone(),
            provenance: rim_resolve::domain::TagProvenance::Manual,
        }]);

        let rows = order_rows(&OrderRowFacts {
            order: &order,
            previous: None,
            placements: &placements,
            report: &report,
            tagging: &tagging,
            needs_input_by_mod: &BTreeMap::new(),
        });

        assert_eq!(rows[0].tier, Tier::Top);
        assert_eq!(rows[0].tags, BTreeSet::from([tag]));
    }
}
