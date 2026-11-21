//! [`Session::mod_detail`]: one active mod's detail for the mods page,
//! built over the pure [`crate::mod_detail`] module.

use rim_analyzer::domain::ModId;
use rim_resolve::domain::OrderSource;

use super::Session;
use crate::finding_index::{FindingFilter, MAX_PAGE_SIZE};
use crate::mod_detail::{ModDetail, ModNotActive, edges_of, find_active};

impl Session {
    /// Resolves `id` by [`ModId::base`] among the active mods and builds its
    /// [`ModDetail`]: its tags, its report edges evaluated against `source`'s
    /// order, and the live finding keys naming it in `source`'s ledger. Every
    /// lookup after the resolution uses the resolved id, so a `_steam` copy
    /// reached through its base id (or the other way round) cannot miss
    /// again. `&mut self`: the finding keys read `source`'s lazily built
    /// ledger ([`Session::findings`]).
    ///
    /// # Errors
    ///
    /// Returns [`ModNotActive`] when no active mod shares `id`'s base id.
    pub fn mod_detail(
        &mut self,
        id: &ModId,
        source: OrderSource,
    ) -> Result<ModDetail, ModNotActive> {
        let entry = find_active(&self.report, id)
            .cloned()
            .ok_or_else(|| ModNotActive(id.clone()))?;
        let (edges_in, edges_out) = edges_of(&self.report, &entry.id, self.orders.get(source));
        let tags = self.tagging.tags_of(&entry.id).clone();
        let finding_page = self.findings(
            source,
            &FindingFilter {
                mod_id: Some(entry.id.clone()),
                offset: 0,
                limit: MAX_PAGE_SIZE,
                ..FindingFilter::default()
            },
        );
        Ok(ModDetail {
            entry,
            tags,
            edges_in,
            edges_out,
            finding_keys: finding_page.items,
            finding_keys_total: finding_page.total,
        })
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::{EdgeStatus, ModId};
    use rim_resolve::domain::{FindingKey, OrderSource, Tag, TagMode};

    use crate::ModNotActive;
    use crate::test_support::{
        report_fixture, session_fixture, session_fixture_with_inactive,
        session_with_sources_and_mods,
    };

    #[test]
    fn a_steam_copy_resolves_by_its_base_id() {
        let mut session = session_fixture(&["x_steam", "y"]);

        let detail = session
            .mod_detail(&ModId::new("x"), OrderSource::Current)
            .expect("x_steam must be found by its base id");

        assert_eq!(detail.entry.id, ModId::new("x_steam"));
    }

    #[test]
    fn tags_are_read_for_the_resolved_mod() {
        let mut session = session_fixture(&["x_steam", "y"]);
        let tag = Tag::new("library").expect("a valid tag slug");
        session.set_manual_tag(ModId::new("x_steam"), tag.clone(), TagMode::Add);

        let detail = session
            .mod_detail(&ModId::new("x"), OrderSource::Current)
            .expect("x_steam must be found by its base id");

        assert!(detail.tags.contains(&tag), "tags: {:?}", detail.tags);
    }

    #[test]
    fn an_inactive_mod_is_not_active() {
        let mut session = session_fixture_with_inactive(&["a"], &["b"]);

        let result = session.mod_detail(&ModId::new("b"), OrderSource::Current);

        assert_eq!(result.unwrap_err(), ModNotActive(ModId::new("b")));
    }

    #[test]
    fn an_unknown_mod_is_not_active() {
        let mut session = session_fixture(&["a"]);

        let result = session.mod_detail(&ModId::new("nobody"), OrderSource::Current);

        assert_eq!(result.unwrap_err(), ModNotActive(ModId::new("nobody")));
    }

    #[test]
    fn edges_are_evaluated_against_the_requested_order() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("framework")
            .mod_("addon")
            .hard_edge("addon", "framework")
            .build();
        // The file loads `addon` first, so Current violates the edge while
        // the sorter's Suggested order satisfies it.
        let mut session =
            session_with_sources_and_mods(SourceIndex::default(), report, &["addon", "framework"]);

        let current = session
            .mod_detail(&ModId::new("addon"), OrderSource::Current)
            .expect("addon is active");
        let suggested = session
            .mod_detail(&ModId::new("addon"), OrderSource::Suggested)
            .expect("addon is active");

        assert_eq!(current.edges_in.len(), 1);
        assert!(current.edges_out.is_empty());
        assert_eq!(current.edges_in[0].status, EdgeStatus::Violated);
        assert_eq!(suggested.edges_in[0].status, EdgeStatus::Satisfied);
    }

    #[test]
    fn finding_keys_name_the_resolved_mod() {
        let mut report = report_fixture(&["x_steam", "y"]);
        report.unsupported_version_mods = vec![ModId::new("x_steam")];
        let mut session =
            session_with_sources_and_mods(SourceIndex::default(), report, &["x_steam", "y"]);

        let detail = session
            .mod_detail(&ModId::new("x"), OrderSource::Current)
            .expect("x_steam is active");

        assert_eq!(detail.finding_keys_total, 1);
        assert_eq!(
            detail.finding_keys,
            vec![FindingKey::UnsupportedVersion {
                mod_id: ModId::new("x_steam"),
            }]
        );
    }
}
