//! The Mods page's two lists, read from the **working** tier: the active
//! set in its own order (Active) and every discovered mod that is not in
//! it (Inactive). Pure over a session's report, tagging and working set;
//! filtering lives here so no interface re-implements it.
//!
//! Unlike [`crate::mod_index`] (the scanned tier, paged and capped at
//! [`crate::MAX_PAGE_SIZE`]) these lists return every matching row up to
//! [`MAX_MOD_LIST_ROWS`], the way `list_order` returns a whole order.
//! `total` is reported before the cut, so a cut is visible, never silent.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{GeneratedMarker, ModId, Report, Source};
use rim_resolve::domain::{Tag, Tagging};

use crate::active_set::ActiveSet;

/// The most rows either list returns. Real installs hold a few thousand
/// mods; the bound exists so a pathological `Mods` folder cannot make one
/// response unbounded.
pub const MAX_MOD_LIST_ROWS: usize = 5_000;

/// Filters for [`crate::Session::working_mod_list`] and
/// [`crate::Session::inactive_mod_list`]. There is no paging.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModListQuery {
    /// Only mods whose id or name contains this substring
    /// (case-insensitive).
    pub search: Option<String>,
    /// Only scanned mods carrying this tag. Ignored by the Inactive list:
    /// a tag is a scan result, and most inactive mods have none.
    pub tag: Option<Tag>,
    /// Only mods from this source (a missing row has none, so a specific
    /// source excludes it).
    pub source: Option<Source>,
}

/// What a row's facts are known from. Closed, so a tag on a row that was
/// never scanned is unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowState {
    /// Part of the last scan: tags and dependents are real.
    Scanned {
        /// Every tag the mod carries.
        tags: BTreeSet<Tag>,
        /// Distinct active mods with a Hard-strength edge pointing at it.
        hard_dependents: usize,
    },
    /// No scan facts, only identity. In the Active list: activated since
    /// the last rescan. In the Inactive list: an ordinary inactive mod
    /// (inactive mods are never scanned).
    PendingScan,
    /// In the working set but found nowhere on disk.
    Missing,
}

/// One row of either list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModListRow {
    /// The mod's exact id.
    pub mod_id: ModId,
    /// Display name; a missing row's own id text.
    pub name: String,
    /// Where its files come from; `None` only for a missing row.
    pub source: Option<Source>,
    /// Its place in the working set (0-based, unfiltered). `None` in the
    /// Inactive list.
    pub position: Option<usize>,
    /// What is known about the row.
    pub state: RowState,
    /// Its Steam Workshop id, when it has one.
    pub workshop_id: Option<u64>,
    /// Its `rimmerge.json` marker, when it carries one.
    pub generated: Option<GeneratedMarker>,
}

/// One list: `total` rows matched the query, `items` holds at most
/// [`MAX_MOD_LIST_ROWS`] of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModList {
    /// Rows matching the query before the [`MAX_MOD_LIST_ROWS`] cut.
    pub total: usize,
    /// The rows, in list order.
    pub items: Vec<ModListRow>,
}

impl ModList {
    fn from_rows(rows: impl Iterator<Item = ModListRow>) -> Self {
        let mut total = 0;
        let mut items = Vec::new();
        for row in rows {
            total += 1;
            if items.len() < MAX_MOD_LIST_ROWS {
                items.push(row);
            }
        }
        Self { total, items }
    }
}

fn matches(row: &ModListRow, query: &ModListQuery, needle: Option<&str>) -> bool {
    let search_ok = needle.is_none_or(|needle| {
        row.mod_id.as_str().to_lowercase().contains(needle)
            || row.name.to_lowercase().contains(needle)
    });
    search_ok && query.source.is_none_or(|source| row.source == Some(source))
}

fn scanned_row(
    mod_entry: &rim_analyzer::domain::Mod,
    tagging: &Tagging,
    position: Option<usize>,
) -> ModListRow {
    ModListRow {
        mod_id: mod_entry.id.clone(),
        name: mod_entry.name.clone(),
        source: Some(mod_entry.source),
        position,
        state: RowState::Scanned {
            tags: tagging.tags_of(&mod_entry.id).clone(),
            hard_dependents: mod_entry.hard_dependents,
        },
        workshop_id: mod_entry.workshop_id,
        generated: mod_entry.generated.clone(),
    }
}

fn pending_row(mod_entry: &rim_analyzer::domain::InactiveMod, position: usize) -> ModListRow {
    ModListRow {
        mod_id: mod_entry.id.clone(),
        name: mod_entry.name.clone(),
        source: Some(mod_entry.source),
        position: Some(position),
        state: RowState::PendingScan,
        workshop_id: mod_entry.workshop_id,
        generated: mod_entry.generated.clone(),
    }
}

/// The Active list: `working`'s ids in order.
pub(crate) fn working_list(
    report: &Report,
    tagging: &Tagging,
    working: &ActiveSet,
    query: &ModListQuery,
) -> ModList {
    let needle = query.search.as_deref().map(str::to_lowercase);
    let scanned: BTreeMap<&ModId, &rim_analyzer::domain::Mod> =
        report.mods.iter().map(|m| (&m.id, m)).collect();
    let unscanned: BTreeMap<&ModId, &rim_analyzer::domain::InactiveMod> =
        report.inactive_mods.iter().map(|m| (&m.id, m)).collect();
    let rows = working.ids().iter().enumerate().map(|(position, id)| {
        if let Some(mod_entry) = scanned.get(id) {
            return scanned_row(mod_entry, tagging, Some(position));
        }
        if let Some(mod_entry) = unscanned.get(id) {
            return pending_row(mod_entry, position);
        }
        ModListRow {
            mod_id: id.clone(),
            name: id.to_string(),
            source: None,
            position: Some(position),
            state: RowState::Missing,
            workshop_id: None,
            generated: None,
        }
    });
    ModList::from_rows(rows.filter(|row| {
        let tag_ok = query.tag.as_ref().is_none_or(|tag| match &row.state {
            RowState::Scanned { tags, .. } => tags.contains(tag),
            RowState::PendingScan | RowState::Missing => false,
        });
        tag_ok && matches(row, query, needle.as_deref())
    }))
}

/// The Inactive list: every mod found on disk whose base id is not in
/// `working`, sorted by id. A scanned-active mod deactivated since the
/// last scan appears here at once.
pub(crate) fn inactive_list(
    report: &Report,
    tagging: &Tagging,
    working: &ActiveSet,
    query: &ModListQuery,
) -> ModList {
    let needle = query.search.as_deref().map(str::to_lowercase);
    let working_bases: BTreeSet<ModId> = working.ids().iter().map(ModId::base).collect();
    let is_inactive = |id: &ModId| !working_bases.contains(&id.base());

    let mut rows: Vec<ModListRow> = report
        .mods
        .iter()
        .filter(|m| is_inactive(&m.id))
        .map(|m| scanned_row(m, tagging, None))
        .chain(
            report
                .inactive_mods
                .iter()
                .filter(|m| is_inactive(&m.id))
                .map(|m| ModListRow {
                    mod_id: m.id.clone(),
                    name: m.name.clone(),
                    source: Some(m.source),
                    position: None,
                    state: RowState::PendingScan,
                    workshop_id: m.workshop_id,
                    generated: m.generated.clone(),
                }),
        )
        .filter(|row| matches(row, query, needle.as_deref()))
        .collect();
    rows.sort_by(|a, b| a.mod_id.cmp(&b.mod_id));
    ModList::from_rows(rows.into_iter())
}

#[cfg(test)]
mod tests {
    use rim_resolve::domain::{TagAssignment, TagProvenance};
    use rim_resolve::test_support::ReportBuilder;

    use super::*;

    fn ids(list: &ModList) -> Vec<&str> {
        list.items.iter().map(|row| row.mod_id.as_str()).collect()
    }

    fn working(ids: &[&str]) -> ActiveSet {
        ActiveSet::trusted(ids.iter().map(|id| ModId::new(*id)).collect())
    }

    #[test]
    fn working_list_keeps_working_order_not_id_order() {
        let report = ReportBuilder::new().mod_("a").mod_("b").mod_("c").build();

        let list = working_list(
            &report,
            &Tagging::default(),
            &working(&["c", "a", "b"]),
            &ModListQuery::default(),
        );

        assert_eq!(ids(&list), ["c", "a", "b"]);
        assert_eq!(
            list.items
                .iter()
                .map(|row| row.position)
                .collect::<Vec<_>>(),
            [Some(0), Some(1), Some(2)]
        );
    }

    #[test]
    fn working_list_returns_every_row_past_the_old_200_cap() {
        let names: Vec<String> = (0..450).map(|i| format!("mod{i:04}")).collect();
        let mut builder = ReportBuilder::new();
        for name in &names {
            builder = builder.mod_(name);
        }
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();

        let list = working_list(
            &builder.build(),
            &Tagging::default(),
            &working(&refs),
            &ModListQuery::default(),
        );

        assert_eq!(list.total, 450);
        assert_eq!(list.items.len(), 450);
    }

    #[test]
    fn lists_cap_at_max_mod_list_rows_and_report_the_full_total() {
        let names: Vec<String> = (0..MAX_MOD_LIST_ROWS + 7)
            .map(|i| format!("mod{i:05}"))
            .collect();
        let mut builder = ReportBuilder::new();
        for name in &names {
            builder = builder.inactive(name);
        }
        let report = builder.build();

        let inactive = inactive_list(
            &report,
            &Tagging::default(),
            &working(&[]),
            &ModListQuery::default(),
        );

        assert_eq!(inactive.total, MAX_MOD_LIST_ROWS + 7);
        assert_eq!(inactive.items.len(), MAX_MOD_LIST_ROWS);
    }

    #[test]
    fn working_list_marks_pending_scan_and_missing_rows() {
        let report = ReportBuilder::new()
            .mod_("scanned")
            .inactive("fresh")
            .build();

        let list = working_list(
            &report,
            &Tagging::default(),
            &working(&["scanned", "fresh", "ghost"]),
            &ModListQuery::default(),
        );

        assert!(matches!(list.items[0].state, RowState::Scanned { .. }));
        assert_eq!(list.items[1].state, RowState::PendingScan);
        assert_eq!(list.items[2].state, RowState::Missing);
        assert_eq!(list.items[2].source, None);
        assert_eq!(list.items[2].name, "ghost");
    }

    #[test]
    fn inactive_list_holds_what_the_working_set_lacks_including_pending_deactivations() {
        let report = ReportBuilder::new()
            .mod_("kept")
            .mod_("dropped")
            .inactive("zzz")
            .inactive("aaa")
            .build();

        let list = inactive_list(
            &report,
            &Tagging::default(),
            &working(&["kept"]),
            &ModListQuery::default(),
        );

        assert_eq!(ids(&list), ["aaa", "dropped", "zzz"]);
        assert!(list.items.iter().all(|row| row.position.is_none()));
    }

    #[test]
    fn inactive_list_hides_a_mod_whose_base_id_is_already_active() {
        let report = ReportBuilder::new().inactive("x").build();

        let list = inactive_list(
            &report,
            &Tagging::default(),
            &working(&["x_steam"]),
            &ModListQuery::default(),
        );

        assert_eq!(list.total, 0);
    }

    #[test]
    fn search_matches_id_or_name_and_tag_filters_scanned_rows_only() {
        let tag = Tag::new("framework").expect("valid tag");
        let tagging = Tagging::new(vec![TagAssignment {
            mod_id: ModId::new("a"),
            tag: tag.clone(),
            provenance: TagProvenance::Manual,
        }]);
        let report = ReportBuilder::new()
            .mod_("a")
            .mod_("b")
            .inactive("c")
            .build();
        let active = working(&["a", "b", "c"]);

        let by_tag = working_list(
            &report,
            &tagging,
            &active,
            &ModListQuery {
                tag: Some(tag),
                ..ModListQuery::default()
            },
        );
        let by_search = working_list(
            &report,
            &tagging,
            &active,
            &ModListQuery {
                search: Some("B".to_string()),
                ..ModListQuery::default()
            },
        );

        assert_eq!(ids(&by_tag), ["a"]);
        assert_eq!(ids(&by_search), ["b"]);
        assert_eq!(by_search.total, 1);
    }

    #[test]
    fn a_specific_source_filter_excludes_missing_rows() {
        let report = ReportBuilder::new().mod_("a").build();

        let list = working_list(
            &report,
            &Tagging::default(),
            &working(&["a", "ghost"]),
            &ModListQuery {
                source: Some(Source::Local),
                ..ModListQuery::default()
            },
        );

        assert!(list.items.iter().all(|row| row.state != RowState::Missing));
    }
}
