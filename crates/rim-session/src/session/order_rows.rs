//! [`Session::order_rows`]: the load-order page's rows, joined by the pure
//! [`crate::order_rows`] module.

use rim_resolve::domain::OrderSource;

use super::Session;
use crate::order_rows::{OrderRow, OrderRowFacts, order_rows};

impl Session {
    /// One [`OrderRow`] per mod in `source`'s order, in that order.
    /// `previous_position` compares with the current order, so it is `None`
    /// on every row when `source` is [`OrderSource::Current`]. The
    /// needs-input count is looked up by [`rim_analyzer::domain::ModId::base`],
    /// so a `_steam` copy shows its base id's count. `&mut self`: that count
    /// reads `source`'s lazily built ledger ([`Session::needs_input_by_mod`]).
    pub fn order_rows(&mut self, source: OrderSource) -> Vec<OrderRow> {
        // Cloned because the ledger accessor borrows the whole session
        // mutably, and the join below reads other session fields.
        let needs_input_by_mod = self.needs_input_by_mod(source).clone();
        order_rows(&OrderRowFacts {
            order: self.orders.get(source),
            previous: (source != OrderSource::Current).then_some(&self.orders.current),
            placements: &self.sort.placements,
            report: &self.report,
            tagging: &self.tagging,
            needs_input_by_mod: &needs_input_by_mod,
        })
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Confidence, OrderSource};

    use crate::Session;
    use crate::test_support::{report_fixture, session_fixture, session_with_sources_and_mods};

    /// `x_steam` carries an `UnsupportedVersion` finding that needs input:
    /// its suggestion's confidence (85) is below the raised threshold.
    fn session_with_needs_input_on_x_steam() -> Session {
        let mut report = report_fixture(&["x_steam", "y"]);
        report.unsupported_version_mods = vec![ModId::new("x_steam")];
        let mut session =
            session_with_sources_and_mods(SourceIndex::default(), report, &["x_steam", "y"]);
        let mut settings = session.settings();
        settings.threshold = Confidence::new(90).unwrap_or_else(|_| unreachable!());
        session.update_settings(settings);
        session
    }

    #[test]
    fn rows_follow_the_order_and_carry_the_report_facts() {
        let mut session = session_fixture(&["a", "b"]);

        let rows = session.order_rows(OrderSource::Current);

        let ids: Vec<&str> = rows.iter().map(|row| row.mod_id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(rows[1].position, 1);
        assert_eq!(rows[1].name, "b");
    }

    #[test]
    fn the_needs_input_count_joins_a_steam_copy_by_its_base_id() {
        let mut session = session_with_needs_input_on_x_steam();

        let rows = session.order_rows(OrderSource::Current);

        let x = rows
            .iter()
            .find(|row| row.mod_id == ModId::new("x_steam"))
            .expect("x_steam is in the order");
        let y = rows
            .iter()
            .find(|row| row.mod_id == ModId::new("y"))
            .expect("y is in the order");
        assert_eq!(x.needs_input_count, 1);
        assert_eq!(y.needs_input_count, 0);
    }

    #[test]
    fn the_current_order_has_no_previous_positions() {
        let mut session = session_fixture(&["a", "b"]);

        let rows = session.order_rows(OrderSource::Current);

        assert!(rows.iter().all(|row| row.previous_position.is_none()));
    }

    #[test]
    fn the_suggested_order_reports_each_mods_current_position() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("framework")
            .mod_("addon")
            .hard_edge("addon", "framework")
            .build();
        // The file loads `addon` first, so the sorter moves `framework`
        // ahead of it and Suggested differs from Current.
        let mut session =
            session_with_sources_and_mods(SourceIndex::default(), report, &["addon", "framework"]);

        let rows = session.order_rows(OrderSource::Suggested);

        assert_eq!(rows[0].mod_id, ModId::new("framework"));
        assert_eq!(rows[0].previous_position, Some(1));
        assert_eq!(rows[1].mod_id, ModId::new("addon"));
        assert_eq!(rows[1].previous_position, Some(0));
    }
}
