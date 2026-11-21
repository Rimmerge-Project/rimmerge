//! The Mods page's two lists, read from the **working** tier: the active
//! set in its own order (Active) and every discovered mod that is not in
//! it (Inactive). Pure over a session's report, tagging and working set;
//! filtering lives here so no interface re-implements it.
//!
//! Unlike [`crate::mod_index`] (the scanned tier, paged and capped at
//! [`crate::MAX_PAGE_SIZE`]) these lists return every matching row up to
//! [`MAX_MOD_LIST_ROWS`], the way `list_order` returns a whole order.
//! `total` is reported before the cut, so a cut is visible, never silent.

use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{GeneratedMarker, ModId, Report, Source};
use rim_resolve::domain::{Tag, Tagging};

use crate::active_set::ActiveSet;
use crate::file_baseline::{FileMembership, MembershipMark};
use crate::mod_times::{ModUpdateTimes, UpdateTime};
use crate::organization::{
    ColorToken, GroupId, GroupName, OrganizationState, OrganizationUnavailable,
};

mod organized;

use organized::ListOrganization;

/// The most rows either list returns. Real installs hold a few thousand
/// mods; the bound exists so a pathological `Mods` folder cannot make one
/// response unbounded.
pub const MAX_MOD_LIST_ROWS: usize = 5_000;

/// What a list is ordered by. Display only: no key changes the working
/// set, the load orders, or what the game loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListSortKey {
    /// The mod's place in the working set. Meaningful only for the Active
    /// list; the Inactive list has no load order, so it falls back to its
    /// default sort there.
    LoadOrder,
    /// The display name, compared Unicode-lowercased, then as written.
    Name,
    /// The package id (the mod's exact id), compared the same way.
    PackageId,
    /// When the mod was last updated; see [`UpdateTime`]. A mod with no
    /// time sorts after every mod with one, in both directions.
    LastUpdated,
}

/// Which way a [`ListSortKey`] runs. Only the key's own comparison is
/// reversed; the final tie-break on exact id is always ascending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    /// Smallest / earliest / A first.
    Ascending,
    /// Largest / latest / Z first.
    Descending,
}

/// A list's display sort. Equal keys fall back to the exact [`ModId`],
/// ascending, so every sort is a total, deterministic order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListSort {
    /// What to order by.
    pub key: ListSortKey,
    /// Which way.
    pub direction: SortDirection,
}

impl ListSort {
    /// The Active list's own order: the working set as it stands.
    pub const ACTIVE_DEFAULT: Self = Self {
        key: ListSortKey::LoadOrder,
        direction: SortDirection::Ascending,
    };

    /// The Inactive list's default: by name, A first.
    pub const INACTIVE_DEFAULT: Self = Self {
        key: ListSortKey::Name,
        direction: SortDirection::Ascending,
    };

    /// Whether this sort shows the working set in its own order, the only
    /// view in which reordering the Active list is meaningful.
    #[must_use]
    pub fn is_load_order(self) -> bool {
        self == Self::ACTIVE_DEFAULT
    }
}

/// Filters and sort for [`crate::Session::working_mod_list`] and
/// [`crate::Session::inactive_mod_list`]. There is no paging.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModListQuery {
    /// Only mods whose id or name contains this substring
    /// (case-insensitive).
    pub search: Option<String>,
    /// Only mods carrying this tag: a scanned mod's every tag, an unscanned
    /// (or inactive) mod's tags added by hand.
    pub tag: Option<Tag>,
    /// Only mods from this source (a missing row has none, so a specific
    /// source excludes it).
    pub source: Option<Source>,
    /// The display sort; `None` is the list's own default
    /// ([`ListSort::ACTIVE_DEFAULT`] / [`ListSort::INACTIVE_DEFAULT`]).
    pub sort: Option<ListSort>,
    /// Only mods in this group scope. A filter, never a reorder: the Active
    /// list stays in load order and stays reorderable.
    pub group: Option<GroupFilter>,
}

/// A group scope a list can be narrowed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupFilter {
    /// Mods in this group or any group below it, each mod once.
    InScope(GroupId),
    /// Mods that are in no group at all.
    Ungrouped,
}

/// Why a list could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModListError {
    /// The group filter names a group that does not exist (deleted
    /// meanwhile): never read as "no filter".
    #[error("no group {0}")]
    UnknownGroup(GroupId),
    /// A group filter needs organization, which is not available.
    #[error(transparent)]
    OrganizationUnavailable(#[from] OrganizationUnavailable),
}

/// One group a mod is a direct member of, as a row shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupLabel {
    /// The group.
    pub id: GroupId,
    /// Its name.
    pub name: GroupName,
    /// Its colour.
    pub color: Option<ColorToken>,
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
    /// (inactive mods are never scanned). An unscanned mod has no inferred
    /// tags, but a tag added by hand is real, so it shows here too.
    PendingScan {
        /// The tags added to the mod by hand (rules page or a decision).
        manual_tags: BTreeSet<Tag>,
    },
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
    /// When it was last updated, or why that is unknown.
    pub updated: UpdateTime,
    /// How its membership differs from the file's active list, by base
    /// id: `Added` on an Active row the file lacks, `Removed` on an
    /// Inactive row the file lists. Order never makes a mark.
    pub membership: MembershipMark,
    /// The groups the mod is a **direct** member of, in group-tree order
    /// (depth first, siblings by name). Empty while organization is
    /// unavailable.
    pub groups: Vec<GroupLabel>,
    /// The mod's own colour. A mod never inherits a group's colour.
    pub color: Option<ColorToken>,
}

/// One list: `total` rows matched the query, `items` holds at most
/// [`MAX_MOD_LIST_ROWS`] of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModList {
    /// Rows matching the query before the [`MAX_MOD_LIST_ROWS`] cut.
    pub total: usize,
    /// The rows, in list order.
    pub items: Vec<ModListRow>,
    /// Whether the rows may be reordered through this view: the Active list
    /// shown in its own load order. A search or filter keeps it on (a move
    /// is anchored to a visible neighbour's id, so it stays exact); another
    /// sort turns it off, since a row's place on screen is then not its
    /// place in the load order. Always `false` for the Inactive list.
    pub reorderable: bool,
}

impl ModList {
    fn from_rows(rows: impl Iterator<Item = ModListRow>, reorderable: bool) -> Self {
        let mut total = 0;
        let mut items = Vec::new();
        for row in rows {
            total += 1;
            if items.len() < MAX_MOD_LIST_ROWS {
                items.push(row);
            }
        }
        Self {
            total,
            items,
            reorderable,
        }
    }
}

fn matches(row: &ModListRow, query: &ModListQuery, needle: Option<&str>) -> bool {
    let search_ok = needle.is_none_or(|needle| {
        row.mod_id.as_str().to_lowercase().contains(needle)
            || row.name.to_lowercase().contains(needle)
    });
    search_ok && query.source.is_none_or(|source| row.source == Some(source))
}

/// Whether the row carries the query's tag (always, with no tag asked for):
/// a scanned mod's every tag, an unscanned mod's tags added by hand.
fn has_tag(row: &ModListRow, query: &ModListQuery) -> bool {
    query.tag.as_ref().is_none_or(|tag| match &row.state {
        RowState::Scanned { tags, .. } => tags.contains(tag),
        RowState::PendingScan { manual_tags } => manual_tags.contains(tag),
        RowState::Missing => false,
    })
}

fn admitted(
    organized: Option<&ListOrganization>,
    row: &ModListRow,
    query: &ModListQuery,
    needle: Option<&str>,
) -> bool {
    ListOrganization::admits(organized, row) && matches(row, query, needle)
}

fn scanned_row(
    mod_entry: &rim_analyzer::domain::Mod,
    tagging: &Tagging,
    times: &ModUpdateTimes,
    position: Option<usize>,
    membership: MembershipMark,
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
        updated: times.get(&mod_entry.id),
        membership,
        groups: Vec::new(),
        color: None,
    }
}

fn pending_row(
    mod_entry: &rim_analyzer::domain::InactiveMod,
    tagging: &Tagging,
    times: &ModUpdateTimes,
    position: Option<usize>,
    membership: MembershipMark,
) -> ModListRow {
    ModListRow {
        mod_id: mod_entry.id.clone(),
        name: mod_entry.name.clone(),
        source: Some(mod_entry.source),
        position,
        // An unscanned mod has no evidence to infer from, so every tag the
        // tagging holds for it was added by hand.
        state: RowState::PendingScan {
            manual_tags: tagging.tags_of(&mod_entry.id).clone(),
        },
        workshop_id: mod_entry.workshop_id,
        generated: mod_entry.generated.clone(),
        updated: times.get(&mod_entry.id),
        membership,
        groups: Vec::new(),
        color: None,
    }
}

/// Orders `rows` by `sort`. Unavailable update times go last whichever
/// way the sort runs; every tie falls back to the exact id, ascending.
fn sort_rows(rows: &mut [ModListRow], sort: ListSort) {
    match sort.key {
        ListSortKey::LoadOrder => rows.sort_by(|a, b| {
            apply_direction(a.position.cmp(&b.position), sort.direction)
                .then_with(|| a.mod_id.cmp(&b.mod_id))
        }),
        ListSortKey::LastUpdated => rows.sort_by(|a, b| {
            compare_updated(a, b, sort.direction).then_with(|| a.mod_id.cmp(&b.mod_id))
        }),
        ListSortKey::Name => sort_by_text(rows, sort.direction, |row| row.name.as_str()),
        ListSortKey::PackageId => {
            sort_by_text(rows, sort.direction, |row| row.mod_id.as_str());
        }
    }
}

fn compare_updated(a: &ModListRow, b: &ModListRow, direction: SortDirection) -> Ordering {
    match (a.updated.known(), b.updated.known()) {
        (Some(left), Some(right)) => apply_direction(left.cmp(&right), direction),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// A sort key that runs ascending or descending, so a cached key can carry
/// the direction while the id tie-break stays ascending beside it. Every
/// key in one sort is the same variant.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Directed<T> {
    Ascending(T),
    Descending(Reverse<T>),
}

/// Text order: Unicode-lowercased first (so "apple" sits with "Apple"),
/// then the text as written so two names differing only in case still
/// order stably, then the id. No locale collation: the backend's order
/// must not depend on the machine. Each row's lowercase form is built once.
fn sort_by_text(
    rows: &mut [ModListRow],
    direction: SortDirection,
    text_of: impl Fn(&ModListRow) -> &str,
) {
    rows.sort_by_cached_key(|row| {
        let text = text_of(row);
        let key = (text.to_lowercase(), text.to_owned());
        let directed = match direction {
            SortDirection::Ascending => Directed::Ascending(key),
            SortDirection::Descending => Directed::Descending(Reverse(key)),
        };
        (directed, row.mod_id.clone())
    });
}

fn apply_direction(ordering: Ordering, direction: SortDirection) -> Ordering {
    match direction {
        SortDirection::Ascending => ordering,
        SortDirection::Descending => ordering.reverse(),
    }
}

/// The Active list: `working`'s ids in order, or in `query.sort`'s order.
///
/// # Errors
///
/// [`ModListError`] when the query's group filter cannot be applied.
pub(crate) fn working_list(
    report: &Report,
    tagging: &Tagging,
    working: &ActiveSet,
    times: &ModUpdateTimes,
    file: &FileMembership,
    organization: &OrganizationState,
    query: &ModListQuery,
) -> Result<ModList, ModListError> {
    let organized = ListOrganization::open(organization, query.group)?;
    let needle = query.search.as_deref().map(str::to_lowercase);
    let scanned: BTreeMap<&ModId, &rim_analyzer::domain::Mod> =
        report.mods.iter().map(|m| (&m.id, m)).collect();
    let unscanned: BTreeMap<&ModId, &rim_analyzer::domain::InactiveMod> =
        report.inactive_mods.iter().map(|m| (&m.id, m)).collect();
    let rows = working.ids().iter().enumerate().map(|(position, id)| {
        let membership = file.mark_active(id);
        if let Some(mod_entry) = scanned.get(id) {
            return scanned_row(mod_entry, tagging, times, Some(position), membership);
        }
        if let Some(mod_entry) = unscanned.get(id) {
            return pending_row(mod_entry, tagging, times, Some(position), membership);
        }
        ModListRow {
            mod_id: id.clone(),
            name: id.to_string(),
            source: None,
            position: Some(position),
            state: RowState::Missing,
            workshop_id: None,
            generated: None,
            updated: times.get(id),
            membership,
            groups: Vec::new(),
            color: None,
        }
    });
    let mut rows: Vec<ModListRow> = rows
        .map(|row| ListOrganization::decorate(organized.as_ref(), row))
        .filter(|row| {
            has_tag(row, query) && admitted(organized.as_ref(), row, query, needle.as_deref())
        })
        .collect();
    let sort = query.sort.unwrap_or(ListSort::ACTIVE_DEFAULT);
    // Working order is already the ascending load order; only another
    // sort needs work.
    if !sort.is_load_order() {
        sort_rows(&mut rows, sort);
    }
    Ok(ModList::from_rows(rows.into_iter(), sort.is_load_order()))
}

/// The Inactive list: every mod found on disk whose base id is not in
/// `working`, in `query.sort`'s order (name, A first, by default). A
/// scanned-active mod deactivated since the last scan appears here at once.
///
/// # Errors
///
/// [`ModListError`] when the query's group filter cannot be applied.
pub(crate) fn inactive_list(
    report: &Report,
    tagging: &Tagging,
    working: &ActiveSet,
    times: &ModUpdateTimes,
    file: &FileMembership,
    organization: &OrganizationState,
    query: &ModListQuery,
) -> Result<ModList, ModListError> {
    let organized = ListOrganization::open(organization, query.group)?;
    let needle = query.search.as_deref().map(str::to_lowercase);
    let working_bases: BTreeSet<ModId> = working.ids().iter().map(ModId::base).collect();
    let is_inactive = |id: &ModId| !working_bases.contains(&id.base());

    let mut rows: Vec<ModListRow> = report
        .mods
        .iter()
        .filter(|m| is_inactive(&m.id))
        .map(|m| scanned_row(m, tagging, times, None, file.mark_inactive(&m.id)))
        .chain(
            report
                .inactive_mods
                .iter()
                .filter(|m| is_inactive(&m.id))
                .map(|m| pending_row(m, tagging, times, None, file.mark_inactive(&m.id))),
        )
        .map(|row| ListOrganization::decorate(organized.as_ref(), row))
        .filter(|row| {
            has_tag(row, query) && admitted(organized.as_ref(), row, query, needle.as_deref())
        })
        .collect();
    // An inactive mod has no load-order position, so that key means the
    // list's default there.
    let sort = match query.sort {
        Some(sort) if sort.key != ListSortKey::LoadOrder => sort,
        Some(_) | None => ListSort::INACTIVE_DEFAULT,
    };
    sort_rows(&mut rows, sort);
    Ok(ModList::from_rows(rows.into_iter(), false))
}

#[cfg(test)]
mod tests {
    use rim_resolve::domain::{TagAssignment, TagProvenance};
    use rim_resolve::test_support::ReportBuilder;

    use super::*;

    /// The tests that predate the sort have no update times.
    fn working_list(
        report: &Report,
        tagging: &Tagging,
        working: &ActiveSet,
        query: &ModListQuery,
    ) -> ModList {
        super::working_list(
            report,
            tagging,
            working,
            &ModUpdateTimes::default(),
            &FileMembership::of(working.ids()),
            &OrganizationState::NotLoaded,
            query,
        )
        .expect("no group filter, so no failure")
    }

    fn inactive_list(
        report: &Report,
        tagging: &Tagging,
        working: &ActiveSet,
        query: &ModListQuery,
    ) -> ModList {
        super::inactive_list(
            report,
            tagging,
            working,
            &ModUpdateTimes::default(),
            &FileMembership::of(working.ids()),
            &OrganizationState::NotLoaded,
            query,
        )
        .expect("no group filter, so no failure")
    }

    fn ids(list: &ModList) -> Vec<&str> {
        list.items.iter().map(|row| row.mod_id.as_str()).collect()
    }

    fn working(ids: &[&str]) -> ActiveSet {
        ActiveSet::trusted(ids.iter().map(|id| ModId::new(*id)).collect())
    }

    #[test]
    fn the_active_list_is_reorderable_only_in_load_order_and_filters_keep_it_on() {
        let report = ReportBuilder::new().mod_("a").mod_("b").build();
        let tagging = Tagging::default();
        let set = working(&["a", "b"]);
        let by_name = ModListQuery {
            sort: Some(ListSort {
                key: ListSortKey::Name,
                direction: SortDirection::Ascending,
            }),
            ..ModListQuery::default()
        };
        let descending_load_order = ModListQuery {
            sort: Some(ListSort {
                key: ListSortKey::LoadOrder,
                direction: SortDirection::Descending,
            }),
            ..ModListQuery::default()
        };
        let searching = ModListQuery {
            search: Some("a".to_string()),
            ..ModListQuery::default()
        };

        assert!(working_list(&report, &tagging, &set, &ModListQuery::default()).reorderable);
        assert!(working_list(&report, &tagging, &set, &searching).reorderable);
        assert!(!working_list(&report, &tagging, &set, &by_name).reorderable);
        assert!(!working_list(&report, &tagging, &set, &descending_load_order).reorderable);
    }

    #[test]
    fn the_inactive_list_is_never_reorderable() {
        let report = ReportBuilder::new().mod_("a").inactive("x").build();
        let tagging = Tagging::default();

        let list = inactive_list(
            &report,
            &tagging,
            &working(&["a"]),
            &ModListQuery::default(),
        );

        assert!(!list.reorderable);
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
        assert_eq!(
            list.items[1].state,
            RowState::PendingScan {
                manual_tags: BTreeSet::new()
            }
        );
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
    fn a_manual_tag_on_an_unscanned_inactive_mod_shows_on_its_row_and_filters_it() {
        let tag = Tag::new("framework").expect("valid tag");
        let tagging = Tagging::new(vec![TagAssignment {
            mod_id: ModId::new("idle"),
            tag: tag.clone(),
            provenance: TagProvenance::Manual,
        }]);
        let report = ReportBuilder::new()
            .mod_("a")
            .inactive("idle")
            .inactive("other")
            .build();
        let active = working(&["a"]);

        let tagged = inactive_list(
            &report,
            &tagging,
            &active,
            &ModListQuery {
                tag: Some(tag.clone()),
                ..ModListQuery::default()
            },
        );
        let untagged_active = working_list(
            &report,
            &tagging,
            &active,
            &ModListQuery {
                tag: Some(tag.clone()),
                ..ModListQuery::default()
            },
        );

        assert_eq!(ids(&tagged), ["idle"]);
        assert_eq!(
            tagged.items[0].state,
            RowState::PendingScan {
                manual_tags: BTreeSet::from([tag])
            }
        );
        assert_eq!(
            untagged_active.total, 0,
            "the Active list's own mods carry no such tag"
        );
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
    // ---- display sort -------------------------------------------------

    use crate::mod_times::{UnixSeconds, UpdateUnavailable};

    fn sort(key: ListSortKey, direction: SortDirection) -> ModListQuery {
        ModListQuery {
            sort: Some(ListSort { key, direction }),
            ..ModListQuery::default()
        }
    }

    fn times(entries: &[(&str, UpdateTime)]) -> ModUpdateTimes {
        ModUpdateTimes::new(
            entries
                .iter()
                .map(|(id, time)| (ModId::new(*id), *time))
                .collect(),
        )
    }

    fn at(seconds: u64) -> UpdateTime {
        UpdateTime::Known(UnixSeconds::new(seconds))
    }

    fn unknown() -> UpdateTime {
        UpdateTime::Unavailable(UpdateUnavailable::Unknown)
    }

    fn named(builder: ReportBuilder, id: &str, name: &str) -> ReportBuilder {
        let name = name.to_string();
        builder.mod_with(id, move |m| m.name = name)
    }

    fn sorted_active(
        report: &Report,
        active: &[&str],
        times: &ModUpdateTimes,
        query: &ModListQuery,
    ) -> Vec<String> {
        let set = working(active);
        super::working_list(
            report,
            &Tagging::default(),
            &set,
            times,
            &FileMembership::of(set.ids()),
            &OrganizationState::NotLoaded,
            query,
        )
        .expect("no group filter, so no failure")
        .items
        .iter()
        .map(|row| row.mod_id.to_string())
        .collect()
    }

    #[test]
    fn active_default_is_the_working_order_and_descending_load_order_reverses_it() {
        let report = ReportBuilder::new().mod_("a").mod_("b").mod_("c").build();
        let none = ModUpdateTimes::default();

        let default = sorted_active(&report, &["c", "a", "b"], &none, &ModListQuery::default());
        let reversed = sorted_active(
            &report,
            &["c", "a", "b"],
            &none,
            &sort(ListSortKey::LoadOrder, SortDirection::Descending),
        );

        assert_eq!(default, ["c", "a", "b"]);
        assert_eq!(reversed, ["b", "a", "c"]);
    }

    #[test]
    fn name_sort_runs_both_ways_and_keeps_ties_in_ascending_id_order() {
        let mut builder = ReportBuilder::new();
        for (id, name) in [
            ("m3", "Same"),
            ("m1", "Same"),
            ("m2", "Alpha"),
            ("m4", "Zulu"),
        ] {
            builder = named(builder, id, name);
        }
        let report = builder.build();
        let none = ModUpdateTimes::default();
        let active = ["m3", "m1", "m2", "m4"];

        let ascending = sorted_active(
            &report,
            &active,
            &none,
            &sort(ListSortKey::Name, SortDirection::Ascending),
        );
        let descending = sorted_active(
            &report,
            &active,
            &none,
            &sort(ListSortKey::Name, SortDirection::Descending),
        );

        assert_eq!(ascending, ["m2", "m1", "m3", "m4"]);
        // Descending reverses the names only; the equal pair stays m1, m3.
        assert_eq!(descending, ["m4", "m1", "m3", "m2"]);
    }

    #[test]
    fn name_collation_is_unicode_lowercase_then_original_text() {
        let mut builder = ReportBuilder::new();
        for (id, name) in [
            ("a", "Banana"),
            ("b", "apple"),
            ("c", "\u{c9}z"),
            ("d", "\u{e9}a"),
            ("e", "Apple"),
        ] {
            builder = named(builder, id, name);
        }
        let report = builder.build();

        let order = sorted_active(
            &report,
            &["a", "b", "c", "d", "e"],
            &ModUpdateTimes::default(),
            &sort(ListSortKey::Name, SortDirection::Ascending),
        );

        // Apple before apple (original text breaks the lowercase tie);
        // "apple" before "Banana" (not a raw byte compare); the accented
        // pair orders by its lowercased letters, not its raw code points.
        assert_eq!(order, ["e", "b", "a", "d", "c"]);
    }

    #[test]
    fn package_id_sort_orders_by_id_text_not_by_name_and_runs_both_ways() {
        let mut builder = ReportBuilder::new();
        for (id, name) in [
            ("zed.mod", "Aardvark"),
            ("alpha.mod", "Zebra"),
            ("mid.mod", "Mid"),
        ] {
            builder = named(builder, id, name);
        }
        let report = builder.build();
        let active = ["zed.mod", "alpha.mod", "mid.mod"];
        let none = ModUpdateTimes::default();

        let ascending = sorted_active(
            &report,
            &active,
            &none,
            &sort(ListSortKey::PackageId, SortDirection::Ascending),
        );
        let descending = sorted_active(
            &report,
            &active,
            &none,
            &sort(ListSortKey::PackageId, SortDirection::Descending),
        );

        assert_eq!(ascending, ["alpha.mod", "mid.mod", "zed.mod"]);
        assert_eq!(descending, ["zed.mod", "mid.mod", "alpha.mod"]);
    }

    #[test]
    fn last_updated_orders_known_times_and_puts_unavailable_last_in_both_directions() {
        let report = ReportBuilder::new()
            .mod_("old")
            .mod_("new")
            .mod_("none_b")
            .mod_("none_a")
            .core("core")
            .build();
        let active = ["old", "new", "none_b", "none_a", "core"];
        let table = times(&[
            ("old", at(100)),
            ("new", at(900)),
            ("none_b", unknown()),
            (
                "core",
                UpdateTime::Unavailable(UpdateUnavailable::ShippedWithGame),
            ),
        ]);

        let ascending = sorted_active(
            &report,
            &active,
            &table,
            &sort(ListSortKey::LastUpdated, SortDirection::Ascending),
        );
        let descending = sorted_active(
            &report,
            &active,
            &table,
            &sort(ListSortKey::LastUpdated, SortDirection::Descending),
        );

        assert_eq!(ascending, ["old", "new", "core", "none_a", "none_b"]);
        assert_eq!(descending, ["new", "old", "core", "none_a", "none_b"]);
    }

    #[test]
    fn equal_update_times_tie_break_on_ascending_id_in_both_directions() {
        let report = ReportBuilder::new().mod_("b").mod_("a").mod_("c").build();
        let table = times(&[("a", at(50)), ("b", at(50)), ("c", at(50))]);

        let ascending = sorted_active(
            &report,
            &["b", "a", "c"],
            &table,
            &sort(ListSortKey::LastUpdated, SortDirection::Ascending),
        );
        let descending = sorted_active(
            &report,
            &["b", "a", "c"],
            &table,
            &sort(ListSortKey::LastUpdated, SortDirection::Descending),
        );

        assert_eq!(ascending, ["a", "b", "c"]);
        assert_eq!(descending, ["a", "b", "c"]);
    }

    #[test]
    fn inactive_list_defaults_to_name_ascending_not_id() {
        let mut report = ReportBuilder::new()
            .inactive("a_id")
            .inactive("b_id")
            .build();
        report.inactive_mods[0].name = "Zulu".to_string();
        report.inactive_mods[1].name = "Alpha".to_string();

        let list = inactive_list(
            &report,
            &Tagging::default(),
            &working(&[]),
            &ModListQuery::default(),
        );

        assert_eq!(ids(&list), ["b_id", "a_id"]);
    }

    #[test]
    fn inactive_load_order_key_means_the_default_not_an_arbitrary_order() {
        let mut report = ReportBuilder::new()
            .inactive("a_id")
            .inactive("b_id")
            .build();
        report.inactive_mods[0].name = "Zulu".to_string();
        report.inactive_mods[1].name = "Alpha".to_string();

        let list = inactive_list(
            &report,
            &Tagging::default(),
            &working(&[]),
            &sort(ListSortKey::LoadOrder, SortDirection::Descending),
        );

        assert_eq!(ids(&list), ["b_id", "a_id"]);
    }

    #[test]
    fn inactive_last_updated_sort_covers_scanned_and_unscanned_rows() {
        let report = ReportBuilder::new()
            .mod_("deactivated")
            .inactive("fresh")
            .inactive("undated")
            .build();
        let table = times(&[("deactivated", at(10)), ("fresh", at(20))]);

        let ascending = super::inactive_list(
            &report,
            &Tagging::default(),
            &working(&[]),
            &table,
            &FileMembership::default(),
            &OrganizationState::NotLoaded,
            &sort(ListSortKey::LastUpdated, SortDirection::Ascending),
        )
        .expect("no group filter, so no failure");
        let descending = super::inactive_list(
            &report,
            &Tagging::default(),
            &working(&[]),
            &table,
            &FileMembership::default(),
            &OrganizationState::NotLoaded,
            &sort(ListSortKey::LastUpdated, SortDirection::Descending),
        )
        .expect("no group filter, so no failure");

        assert_eq!(ids(&ascending), ["deactivated", "fresh", "undated"]);
        assert_eq!(ids(&descending), ["fresh", "deactivated", "undated"]);
    }

    #[test]
    fn sort_applies_after_search_and_source_filters_and_total_counts_the_matches() {
        let mut builder = ReportBuilder::new();
        for (id, name) in [
            ("a", "Cedar tools"),
            ("b", "Birch tools"),
            ("c", "Other"),
            ("d", "Alder tools"),
        ] {
            builder = named(builder, id, name);
        }
        let report = builder.build();
        let mut query = sort(ListSortKey::Name, SortDirection::Ascending);
        query.search = Some("TOOLS".to_string());

        let list = super::working_list(
            &report,
            &Tagging::default(),
            &working(&["a", "b", "c", "d"]),
            &ModUpdateTimes::default(),
            &FileMembership::default(),
            &OrganizationState::NotLoaded,
            &query,
        )
        .expect("no group filter, so no failure");

        assert_eq!(ids(&list), ["d", "b", "a"]);
        assert_eq!(list.total, 3);
    }

    #[test]
    fn a_sorted_row_keeps_its_load_order_position() {
        let report = ReportBuilder::new().mod_("b").mod_("a").build();

        let list = super::working_list(
            &report,
            &Tagging::default(),
            &working(&["b", "a"]),
            &ModUpdateTimes::default(),
            &FileMembership::default(),
            &OrganizationState::NotLoaded,
            &sort(ListSortKey::PackageId, SortDirection::Ascending),
        )
        .expect("no group filter, so no failure");

        assert_eq!(ids(&list), ["a", "b"]);
        assert_eq!(
            list.items
                .iter()
                .map(|row| row.position)
                .collect::<Vec<_>>(),
            [Some(1), Some(0)]
        );
    }

    mod organization {
        use super::*;
        use crate::organization::{
            ColorToken, GroupId, GroupName, ModOrganization, OrganizationIssue, OrganizedModId,
        };

        fn member(id: &str) -> OrganizedModId {
            OrganizedModId::new(&ModId::new(id)).expect("valid fixture id")
        }

        fn set(ids: &[&str]) -> BTreeSet<OrganizedModId> {
            ids.iter().map(|id| member(id)).collect()
        }

        fn group(
            organization: &mut ModOrganization,
            text: &str,
            parent: Option<GroupId>,
        ) -> GroupId {
            organization
                .create_group(GroupName::new(text).expect("valid name"), parent)
                .expect("create")
        }

        /// Active `a`, `b_steam`, `c`; inactive `idle`, `other`. Group
        /// `Parent` holds `a` and `idle`; its subgroup `Kid` holds `a` and
        /// `b` (the base of `b_steam`).
        fn fixture() -> (Report, ActiveSet, OrganizationState, GroupId, GroupId) {
            let report = ReportBuilder::new()
                .mod_("a")
                .mod_("b_steam")
                .mod_("c")
                .inactive("idle")
                .inactive("other")
                .build();
            let mut organization = ModOrganization::new();
            let parent = group(&mut organization, "Parent", None);
            let kid = group(&mut organization, "Kid", Some(parent));
            organization
                .add_members(parent, &set(&["a", "idle"]))
                .expect("add");
            organization
                .add_members(kid, &set(&["a", "b"]))
                .expect("add");
            organization
                .set_mod_colors(&set(&["c"]), Some(ColorToken::Teal))
                .expect("colour");
            (
                report,
                working(&["a", "b_steam", "c"]),
                OrganizationState::Loaded(organization),
                parent,
                kid,
            )
        }

        fn query_of(group: Option<GroupFilter>) -> ModListQuery {
            ModListQuery {
                group,
                ..ModListQuery::default()
            }
        }

        fn active_list(
            report: &Report,
            set: &ActiveSet,
            state: &OrganizationState,
            query: &ModListQuery,
        ) -> Result<ModList, ModListError> {
            super::super::working_list(
                report,
                &Tagging::default(),
                set,
                &ModUpdateTimes::default(),
                &FileMembership::default(),
                state,
                query,
            )
        }

        fn idle_list(
            report: &Report,
            set: &ActiveSet,
            state: &OrganizationState,
            query: &ModListQuery,
        ) -> Result<ModList, ModListError> {
            super::super::inactive_list(
                report,
                &Tagging::default(),
                set,
                &ModUpdateTimes::default(),
                &FileMembership::default(),
                state,
                query,
            )
        }

        #[test]
        fn a_scope_filter_includes_descendants_and_lists_each_mod_once() {
            let (report, set, state, parent, _) = fixture();
            let query = query_of(Some(GroupFilter::InScope(parent)));

            let active = active_list(&report, &set, &state, &query).expect("filter applies");
            let inactive = idle_list(&report, &set, &state, &query).expect("filter applies");

            assert_eq!(ids(&active), ["a", "b_steam"], "load order, a once");
            assert_eq!(active.total, 2);
            assert_eq!(ids(&inactive), ["idle"]);
        }

        #[test]
        fn a_subgroup_scope_excludes_its_parents_own_members() {
            let (report, set, state, _, kid) = fixture();
            let query = query_of(Some(GroupFilter::InScope(kid)));

            let active = active_list(&report, &set, &state, &query).expect("ok");
            let inactive = idle_list(&report, &set, &state, &query).expect("ok");

            assert_eq!(ids(&active), ["a", "b_steam"]);
            assert!(ids(&inactive).is_empty());
        }

        #[test]
        fn the_ungrouped_filter_lists_mods_in_no_group() {
            let (report, set, state, _, _) = fixture();
            let query = query_of(Some(GroupFilter::Ungrouped));

            let active = active_list(&report, &set, &state, &query).expect("ok");
            let inactive = idle_list(&report, &set, &state, &query).expect("ok");

            assert_eq!(ids(&active), ["c"]);
            assert_eq!(ids(&inactive), ["other"]);
        }

        #[test]
        fn a_group_filter_keeps_the_active_list_reorderable_in_load_order() {
            let (report, set, state, parent, _) = fixture();
            let query = query_of(Some(GroupFilter::InScope(parent)));

            let active = active_list(&report, &set, &state, &query).expect("ok");

            assert!(active.reorderable);
            assert_eq!(
                active
                    .items
                    .iter()
                    .map(|row| row.position)
                    .collect::<Vec<_>>(),
                [Some(0), Some(1)],
                "positions are the unfiltered working positions"
            );
        }

        #[test]
        fn rows_carry_labels_by_base_id_in_group_tree_order_and_their_own_colour() {
            let (report, set, state, parent, kid) = fixture();
            let query = ModListQuery::default();

            let active = active_list(&report, &set, &state, &query).expect("ok");
            let inactive = idle_list(&report, &set, &state, &query).expect("ok");

            let labels =
                |row: &ModListRow| row.groups.iter().map(|label| label.id).collect::<Vec<_>>();
            assert_eq!(
                labels(&active.items[0]),
                [parent, kid],
                "tree order lists a group before its subgroup, though Kid sorts first by name"
            );
            assert_eq!(
                labels(&active.items[1]),
                [kid],
                "b_steam is a member through its base id b"
            );
            assert!(active.items[2].groups.is_empty());
            assert_eq!(active.items[2].color, Some(ColorToken::Teal));
            assert_eq!(
                active.items[0].color, None,
                "no colour is inherited from a group"
            );
            assert_eq!(labels(&inactive.items[0]), [parent]);
        }

        #[test]
        fn labels_follow_the_tree_depth_first_with_siblings_by_name_not_creation_order() {
            let report = ReportBuilder::new().mod_("a").build();
            let mut organization = ModOrganization::new();
            // Created Zeta, Alpha, Child, Beta: ids 1..=4, which is not the tree order.
            let zeta = group(&mut organization, "Zeta", None);
            let alpha = group(&mut organization, "Alpha", None);
            let child = group(&mut organization, "Child", Some(alpha));
            let beta = group(&mut organization, "Beta", None);
            for id in [zeta, alpha, child, beta] {
                organization.add_members(id, &set(&["a"])).expect("add");
            }
            let state = OrganizationState::Loaded(organization);

            let active = active_list(&report, &working(&["a"]), &state, &ModListQuery::default())
                .expect("ok");

            let names: Vec<_> = active.items[0]
                .groups
                .iter()
                .map(|label| label.name.as_str())
                .collect();
            assert_eq!(names, ["Alpha", "Child", "Beta", "Zeta"]);
        }

        #[test]
        fn a_stale_group_id_is_an_error_never_no_filter() {
            let (report, set, state, _, _) = fixture();
            let stale = GroupId::new(99).expect("non-zero");
            let query = query_of(Some(GroupFilter::InScope(stale)));

            assert_eq!(
                active_list(&report, &set, &state, &query),
                Err(ModListError::UnknownGroup(stale))
            );
            assert_eq!(
                idle_list(&report, &set, &state, &query),
                Err(ModListError::UnknownGroup(stale))
            );
        }

        #[test]
        fn without_organization_rows_have_no_labels_and_a_filter_is_refused() {
            let (report, set, _, parent, _) = fixture();

            for state in [
                OrganizationState::NotLoaded,
                OrganizationState::Recovered {
                    issue: OrganizationIssue::Malformed,
                },
            ] {
                let plain =
                    active_list(&report, &set, &state, &ModListQuery::default()).expect("ok");
                assert!(
                    plain
                        .items
                        .iter()
                        .all(|row| row.groups.is_empty() && row.color.is_none())
                );
                let filtered = active_list(
                    &report,
                    &set,
                    &state,
                    &query_of(Some(GroupFilter::InScope(parent))),
                );
                assert!(matches!(
                    filtered,
                    Err(ModListError::OrganizationUnavailable(_))
                ));
            }
        }

        #[test]
        fn search_runs_inside_the_group_scope() {
            let (report, set, state, parent, _) = fixture();
            let query = ModListQuery {
                group: Some(GroupFilter::InScope(parent)),
                search: Some("b".to_string()),
                ..ModListQuery::default()
            };

            let list = active_list(&report, &set, &state, &query).expect("ok");

            assert_eq!(ids(&list), ["b_steam"]);
        }
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        const NAMES: [&str; 5] = ["Alpha", "alpha", "Beta", "beta tools", ""];

        /// A report of `specs.len()` mods `m0..`, their update times, and
        /// the active order given by `order` (indices into the mods).
        fn fixture(
            specs: &[(usize, Option<u64>)],
            order: &[usize],
        ) -> (Report, ModUpdateTimes, Vec<String>) {
            let mut builder = ReportBuilder::new();
            let mut entries = Vec::new();
            for (index, (name, time)) in specs.iter().enumerate() {
                builder = named(builder, &format!("m{index}"), NAMES[*name]);
                entries.push((
                    ModId::new(format!("m{index}")),
                    time.map_or_else(unknown, at),
                ));
            }
            let active = order.iter().map(|index| format!("m{index}")).collect();
            (
                builder.build(),
                ModUpdateTimes::new(entries.into_iter().collect()),
                active,
            )
        }

        fn run(
            fixture: &(Report, ModUpdateTimes, Vec<String>),
            query: &ModListQuery,
        ) -> Vec<String> {
            let active: Vec<&str> = fixture.2.iter().map(String::as_str).collect();
            sorted_active(&fixture.0, &active, &fixture.1, query)
        }

        fn key_strategy() -> impl Strategy<Value = ListSortKey> {
            prop_oneof![
                Just(ListSortKey::LoadOrder),
                Just(ListSortKey::Name),
                Just(ListSortKey::PackageId),
                Just(ListSortKey::LastUpdated),
            ]
        }

        fn direction_strategy() -> impl Strategy<Value = SortDirection> {
            prop_oneof![
                Just(SortDirection::Ascending),
                Just(SortDirection::Descending)
            ]
        }

        /// Each mod's name index and update time, then two active orders.
        type ModsAndOrders = (Vec<(usize, Option<u64>)>, Vec<usize>, Vec<usize>);

        fn mods_and_orders() -> impl Strategy<Value = ModsAndOrders> {
            prop::collection::vec((0..NAMES.len(), prop::option::of(1u64..50)), 1..10)
                .prop_flat_map(|specs| {
                    let indices: Vec<usize> = (0..specs.len()).collect();
                    (
                        Just(specs),
                        Just(indices.clone()).prop_shuffle(),
                        Just(indices).prop_shuffle(),
                    )
                })
        }

        proptest! {
            #[test]
            fn a_sorted_list_is_a_permutation_of_the_filtered_set(
                (specs, order, _) in mods_and_orders(),
                key in key_strategy(),
                direction in direction_strategy(),
                is_searching in any::<bool>(),
            ) {
                let data = fixture(&specs, &order);
                let search = is_searching.then(|| "alp".to_string());
                let mut query = sort(key, direction);
                query.search.clone_from(&search);
                let unsorted = ModListQuery { search, ..ModListQuery::default() };

                let mut sorted = run(&data, &query);
                let mut expected = run(&data, &unsorted);
                sorted.sort();
                expected.sort();

                prop_assert_eq!(sorted, expected);
            }

            #[test]
            fn a_display_sort_other_than_load_order_ignores_the_input_order(
                (specs, order, other_order) in mods_and_orders(),
                key in prop_oneof![
                    Just(ListSortKey::Name),
                    Just(ListSortKey::PackageId),
                    Just(ListSortKey::LastUpdated),
                ],
                direction in direction_strategy(),
            ) {
                let query = sort(key, direction);

                let first = run(&fixture(&specs, &order), &query);
                let second = run(&fixture(&specs, &other_order), &query);

                prop_assert_eq!(first, second);
            }
        }
    }

    #[test]
    fn only_ascending_load_order_counts_as_the_load_order_view() {
        assert!(ListSort::ACTIVE_DEFAULT.is_load_order());
        assert!(
            !ListSort {
                key: ListSortKey::LoadOrder,
                direction: SortDirection::Descending
            }
            .is_load_order()
        );
        assert!(!ListSort::INACTIVE_DEFAULT.is_load_order());
    }
}
