//! [`ModDetail`]: one active mod's detail for the mods page, built by
//! [`crate::Session::mod_detail`]. Resolving the mod and splitting its edges
//! is pure; only the finding keys need the session's lazily built ledger.

use std::collections::BTreeSet;

use rim_analyzer::domain::{Edge, EdgeStatus, LoadOrder, Mod, ModId, Report};
use rim_resolve::domain::{FindingKey, Tag};

/// [`crate::Session::mod_detail`]'s failure: no active mod shares the
/// requested id's [`ModId::base`]. An inactive or missing mod is not active
/// either, so it fails the same way.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} is not active")]
pub struct ModNotActive(pub ModId);

/// One report edge touching the mod, with its status in the order the
/// detail was built for (evaluated fresh, never the scan-time status).
#[derive(Debug, Clone)]
pub struct ModEdge {
    /// The edge as the report holds it.
    pub edge: Edge,
    /// Whether the order satisfies it.
    pub status: EdgeStatus,
}

/// One active mod's detail. No `PartialEq`: [`Mod`] and [`Edge`] have none.
#[derive(Debug, Clone)]
pub struct ModDetail {
    /// The mod's report row. Its `id` is the resolved, canonical id, which
    /// may differ from the requested one by a `_steam` suffix.
    pub entry: Mod,
    /// Every tag it currently carries.
    pub tags: BTreeSet<Tag>,
    /// Edges pointing into it (it is the `after` side), in report order.
    pub edges_in: Vec<ModEdge>,
    /// Edges it points out to another mod (it is the `before` side), in
    /// report order.
    pub edges_out: Vec<ModEdge>,
    /// Live finding keys naming it, at most [`crate::MAX_PAGE_SIZE`], in
    /// the finding index's stable order.
    pub finding_keys: Vec<FindingKey>,
    /// How many live findings name it, before the cut.
    pub finding_keys_total: usize,
}

/// The active report row for `id`: the one with exactly that id first, else
/// the first sharing its [`ModId::base`]. The exact pass matters when both a
/// plain and a `_steam` copy of a mod are active: asking for one copy must
/// answer for that copy. Only active rows are searched (unlike
/// [`crate::mod_info`]'s resolution, which also reads inactive rows).
pub(crate) fn find_active<'report>(report: &'report Report, id: &ModId) -> Option<&'report Mod> {
    let target = id.base();
    report
        .mods
        .iter()
        .find(|entry| entry.id == *id)
        .or_else(|| report.mods.iter().find(|entry| entry.id.base() == target))
}

/// `id`'s incoming and outgoing report edges, each evaluated against
/// `order`. `id` is compared exactly, so pass the resolved id. An edge from
/// a mod to itself counts as incoming only.
pub(crate) fn edges_of(
    report: &Report,
    id: &ModId,
    order: &LoadOrder,
) -> (Vec<ModEdge>, Vec<ModEdge>) {
    let mut edges_in = Vec::new();
    let mut edges_out = Vec::new();
    for report_edge in &report.edges {
        let edge = &report_edge.edge;
        let direction = if edge.after == *id {
            &mut edges_in
        } else if edge.before == *id {
            &mut edges_out
        } else {
            continue;
        };
        direction.push(ModEdge {
            edge: edge.clone(),
            status: rim_resolve::evaluate::edge_status(edge, order),
        });
    }
    (edges_in, edges_out)
}

#[cfg(test)]
mod tests {
    use rim_resolve::test_support::ReportBuilder;

    use super::*;

    #[test]
    fn find_active_resolves_a_steam_copy_by_its_base_id() {
        let report = ReportBuilder::new().mod_("x_steam").mod_("y").build();

        let found = find_active(&report, &ModId::new("x")).map(|entry| entry.id.clone());

        assert_eq!(found, Some(ModId::new("x_steam")));
    }

    #[test]
    fn find_active_does_not_resolve_an_inactive_mod() {
        let report = ReportBuilder::new().mod_("a").inactive("b").build();

        assert!(find_active(&report, &ModId::new("b")).is_none());
    }

    #[test]
    fn edges_of_splits_by_direction_and_evaluates_against_the_order() {
        let report = ReportBuilder::new()
            .mod_("framework")
            .mod_("addon")
            .mod_("other")
            .hard_edge("addon", "framework")
            .hard_edge("other", "addon")
            .build();
        // `addon` before `framework`: its incoming edge is violated here,
        // its outgoing one (to `other`) is satisfied.
        let order = LoadOrder::new(vec![
            ModId::new("addon"),
            ModId::new("framework"),
            ModId::new("other"),
        ]);

        let (edges_in, edges_out) = edges_of(&report, &ModId::new("addon"), &order);

        assert_eq!(edges_in.len(), 1);
        assert_eq!(edges_in[0].edge.before, ModId::new("framework"));
        assert_eq!(edges_in[0].status, EdgeStatus::Violated);
        assert_eq!(edges_out.len(), 1);
        assert_eq!(edges_out[0].edge.after, ModId::new("other"));
        assert_eq!(edges_out[0].status, EdgeStatus::Satisfied);
    }
}
