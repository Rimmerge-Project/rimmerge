//! [`ListItems`]: defs of one item type across the active list, with
//! owner and a "first leaf field" hint, paged at [`crate::MAX_PAGE_SIZE`]
//! — the row editor's item picker.

use std::collections::BTreeSet;

use rim_analyzer::domain::{LoadOrder, ModId, Selector};
use rim_merge::tree::{Content, FieldTree, PathSegment};
use rim_resolve::domain::{AssignmentProject, AssignmentRow, DefKey, RowKey, RowValue};

use super::assignment_instances::AssignmentInstancesError;
use super::def_sources;
use crate::Session;
use crate::ports::DefSourceReader;

/// Filters and paging for [`ListItems::execute`]. `limit` is capped at
/// [`crate::MAX_PAGE_SIZE`] regardless of the requested value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListItemsFilter {
    /// Only defs whose `defName` contains this substring
    /// (case-insensitive).
    pub search: Option<String>,
    /// How many matching defs to skip before collecting the page.
    pub offset: usize,
    /// How many defs to collect, capped at [`crate::MAX_PAGE_SIZE`].
    pub limit: usize,
}

/// One item a slot can pick: its identity, owner, and a display hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// The item's own type and `defName`.
    pub def: DefKey,
    /// The mod whose copy wins under the selected order (the same
    /// last-in-order rule an ordinary `DefOverride` resolves by).
    pub owner: ModId,
    /// The first top-level scalar child's text, `defName` itself
    /// excluded — generically "the first leaf field", e.g.
    /// `example.PartDef`'s own `hediffName`. `None` when the item has no
    /// such child at all.
    pub hint: Option<String>,
}

/// One page of [`ListItems::execute`], plus the total matching the filter
/// (before paging).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItemsPage {
    /// Total defs of the type matching [`ListItemsFilter::search`],
    /// before `offset`/`limit`.
    pub total: usize,
    /// This page's items, in `defName` order.
    pub items: Vec<ListItem>,
}

/// The first top-level scalar leaf's text, `defName` excluded — see
/// [`ListItem::hint`]'s own doc comment.
fn first_leaf_hint(tree: &FieldTree) -> Option<String> {
    for (path, node) in tree.leaves() {
        let [PathSegment::Child(tag)] = path.segments() else {
            continue;
        };
        if tag == "defName" {
            continue;
        }
        if let Content::Text(text) = &node.content {
            return Some(text.clone());
        }
    }
    None
}

/// The mod whose copy of `def_name` wins under `order` — the last owner
/// (in `order`) among `owners`, mirroring
/// `super::def_sources::def_owner_and_raw`'s own `Selector::DefName` rule
/// without needing to read the XML just to answer "who owns this".
fn winner_owner(owners: &[ModId], order: &LoadOrder) -> Option<ModId> {
    order
        .as_slice()
        .iter()
        .rev()
        .find(|id| owners.contains(id))
        .cloned()
}

/// Every def of `def_type` matching `search` (case-insensitive substring
/// of `defName`, if given), with its owners, in `defName` order — the
/// filter step, before paging.
fn matching_defs(
    session: &Session,
    def_type: &str,
    search: Option<&str>,
) -> Vec<(String, Vec<ModId>)> {
    let needle = search.map(str::to_ascii_lowercase);
    let mut matching: Vec<(String, Vec<ModId>)> = session
        .sources()
        .owners_by_def
        .iter()
        .filter(|((candidate_type, def_name), _)| {
            candidate_type == def_type
                && needle
                    .as_deref()
                    .is_none_or(|n| def_name.to_ascii_lowercase().contains(n))
        })
        .map(|((_, def_name), owners)| (def_name.clone(), owners.clone()))
        .collect();
    matching.sort_by(|(a, _), (b, _)| a.cmp(b));
    matching
}

/// `matching`, skipped/taken to one page — `limit` is clamped to
/// [`crate::MAX_PAGE_SIZE`] regardless of the value given.
fn page_of(
    matching: Vec<(String, Vec<ModId>)>,
    offset: usize,
    limit: usize,
) -> Vec<(String, Vec<ModId>)> {
    let limit = limit.min(crate::MAX_PAGE_SIZE);
    matching.into_iter().skip(offset).take(limit).collect()
}

/// The row's first scalar value, if any — [`ListItem::hint`]'s own
/// definition for a free-standing project row, mirroring
/// [`first_leaf_hint`]'s "first leaf field" reading for an active def's
/// own tree. [`AssignmentRow::values`] is a `BTreeMap<FieldPath, RowValue>`,
/// so iterating it in key order (its own [`FieldPath`] order — the same
/// order the field appears in the schema/rendered XML) and taking the
/// first [`RowValue::Text`] is deterministic.
fn own_row_hint(row: &AssignmentRow) -> Option<String> {
    row.values.values().find_map(|value| match value {
        RowValue::Text(text) => Some(text.clone()),
        _ => None,
    })
}

/// `project`'s own free-standing (`RowKey::Own`) rows of `def_type`,
/// matching `search` the same way [`matching_defs`] does — the "own"
/// half of a picker's own item list, prepended ahead of the project's
/// other free-standing rows of that type. No
/// section for `def_type` at all (including one this project doesn't
/// have, or a target-keyed one with no free-standing rows to begin with)
/// is simply empty — never an error, matching a def type with no active
/// defs at all.
fn own_items(project: &AssignmentProject, def_type: &str, search: Option<&str>) -> Vec<ListItem> {
    let Some(section) = project.section(def_type) else {
        return Vec::new();
    };
    let needle = search.map(str::to_ascii_lowercase);
    let mut items: Vec<ListItem> = section
        .rows
        .iter()
        .filter_map(|(key, row)| {
            let RowKey::Own(name) = key else {
                return None;
            };
            if needle
                .as_deref()
                .is_some_and(|n| !name.to_ascii_lowercase().contains(n))
            {
                return None;
            }
            Some(ListItem {
                def: DefKey {
                    def_type: def_type.to_string(),
                    def_name: name.clone(),
                },
                owner: project.identity().package_id().clone(),
                hint: own_row_hint(row),
            })
        })
        .collect();
    items.sort_by(|a, b| a.def.def_name.cmp(&b.def.def_name));
    items
}

/// Splits one combined `(offset, limit)` window across `own`'s items
/// and the
/// active list's own matches: `own`'s own slice of the window, then
/// however many slots (if any) remain for the active list, starting from
/// its own beginning once `own` is exhausted by `offset` alone.
fn split_window(own_len: usize, offset: usize, limit: usize) -> (usize, usize, usize, usize) {
    if offset < own_len {
        let own_take = limit.min(own_len - offset);
        (offset, own_take, 0, limit - own_take)
    } else {
        (offset, 0, offset - own_len, limit)
    }
}

/// Lists every active def of one item type, with owner and hint,
/// filtered and paged.
pub struct ListItems<Reader> {
    reader: Reader,
}

impl<Reader: DefSourceReader> ListItems<Reader> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(reader: Reader) -> Self {
        Self { reader }
    }

    /// `project`, when given, prepends that project's own free-standing
    /// rows of `def_type` (owner = the project's own package id, hint =
    /// the row's first scalar value) ahead of the active list's own
    /// matches — paging spans the
    /// combined sequence, so `total`/pages stay consistent whether or not
    /// a project supplied any own rows. `None` (no project in context, or
    /// a caller with no assignment-project scope) behaves exactly as
    /// before: the active list alone.
    ///
    /// # Errors
    ///
    /// Returns [`AssignmentInstancesError`] when reading a page's own
    /// hint text fails — never for a def outside the current page, since
    /// only the page's own items are ever read back
    ///
    pub fn execute(
        &self,
        session: &mut Session,
        project: Option<&AssignmentProject>,
        def_type: &str,
        filter: &ListItemsFilter,
    ) -> Result<ListItemsPage, AssignmentInstancesError> {
        let order = session.orders().get(session.selected()).clone();
        let limit = filter.limit.min(crate::MAX_PAGE_SIZE);

        let own = project
            .map(|p| own_items(p, def_type, filter.search.as_deref()))
            .unwrap_or_default();
        let own_len = own.len();
        let (own_skip, own_take, active_skip, active_take) =
            split_window(own_len, filter.offset, limit);
        let mut items: Vec<ListItem> = own.into_iter().skip(own_skip).take(own_take).collect();

        let matching = matching_defs(session, def_type, filter.search.as_deref());
        let active_total = matching.len();
        let page = page_of(matching, active_skip, active_take);
        for (def_name, owners) in page {
            if let Some(item) = self.read_item(session, &order, def_type, def_name, &owners)? {
                items.push(item);
            }
        }

        Ok(ListItemsPage {
            total: own_len + active_total,
            items,
        })
    }

    /// Reads one page item back: `None` when no owner in `owners` is
    /// active in `order` at all (a stale index entry, never expected in
    /// practice); otherwise the winning owner's own raw tree, reduced to
    /// [`ListItem::hint`].
    fn read_item(
        &self,
        session: &mut Session,
        order: &LoadOrder,
        def_type: &str,
        def_name: String,
        owners: &[ModId],
    ) -> Result<Option<ListItem>, AssignmentInstancesError> {
        let Some(owner) = winner_owner(owners, order) else {
            return Ok(None);
        };
        let (_, tree) = def_sources::def_owner_and_raw(
            &self.reader,
            session,
            order,
            def_type,
            &def_name,
            Selector::DefName,
        )?;
        Ok(Some(ListItem {
            def: DefKey {
                def_type: def_type.to_string(),
                def_name,
            },
            owner,
            hint: first_leaf_hint(&tree),
        }))
    }
}

/// Every distinct `def_type` [`ListItems`] can be pointed at that some
/// active mod actually defines — a thin convenience over
/// [`rim_analyzer::analysis::SourceIndex::owners_by_def`], for a caller
/// building a picker's own type list without reaching into the index
/// directly.
#[must_use]
pub fn item_types(session: &Session) -> BTreeSet<String> {
    session
        .sources()
        .owners_by_def
        .keys()
        .map(|(def_type, _)| def_type.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{DefSourceError, ElementExpectation};
    use crate::test_support::{report_fixture, session_with_sources_and_mods};
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::{DefEntry, XmlLocator};
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::sync::Arc;

    struct FakeReader {
        by_ordinal: BTreeMap<u32, String>,
    }

    impl DefSourceReader for FakeReader {
        fn read_element(
            &self,
            locator: &XmlLocator,
            _expected: &ElementExpectation,
        ) -> Result<String, DefSourceError> {
            self.by_ordinal
                .get(&locator.element_path[0])
                .cloned()
                .ok_or_else(|| DefSourceError::Io {
                    file: locator.file.to_path_buf(),
                    message: "not seeded".to_string(),
                })
        }
    }

    fn locator(ordinal: u32) -> XmlLocator {
        XmlLocator::new(Arc::from(Path::new("Defs/fixture.xml")), vec![ordinal])
    }

    fn fixture() -> (SourceIndex, FakeReader) {
        let mut index = SourceIndex::default();
        for (ordinal, def_name, owner) in [
            (0u32, "PartA", "framework"),
            (1, "PartB", "framework"),
            (2, "PartC", "addon"),
        ] {
            index.defs.insert(
                (
                    ModId::new(owner),
                    ("example.PartDef".to_string(), def_name.to_string()),
                ),
                vec![DefEntry {
                    def_type: "example.PartDef".to_string(),
                    def_name: def_name.to_string(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: locator(ordinal),
                }],
            );
            index.owners_by_def.insert(
                ("example.PartDef".to_string(), def_name.to_string()),
                vec![ModId::new(owner)],
            );
        }
        let reader = FakeReader {
            by_ordinal: BTreeMap::from([
                (0,
                    "<example.PartDef><defName>PartA</defName><hediffName>HediffA</hediffName></example.PartDef>"
                        .to_string()),
                (1,
                    "<example.PartDef><defName>PartB</defName><hediffName>HediffB</hediffName></example.PartDef>"
                        .to_string()),
                (2,
                    "<example.PartDef><defName>PartC</defName></example.PartDef>".to_string()),
            ]),
        };
        (index, reader)
    }

    #[test]
    fn lists_every_item_with_owner_and_hint_in_def_name_order() {
        let (sources, reader) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report_fixture(&["framework", "addon"]),
            &["framework", "addon"],
        );
        let use_case = ListItems::new(reader);

        let page = use_case
            .execute(
                &mut session,
                None,
                "example.PartDef",
                &ListItemsFilter {
                    search: None,
                    offset: 0,
                    limit: 10,
                },
            )
            .expect("must succeed");

        assert_eq!(page.total, 3);
        let names: Vec<&str> = page.items.iter().map(|i| i.def.def_name.as_str()).collect();
        assert_eq!(names, vec!["PartA", "PartB", "PartC"]);
        assert_eq!(page.items[0].owner, ModId::new("framework"));
        assert_eq!(page.items[0].hint, Some("HediffA".to_string()));
        assert_eq!(page.items[2].owner, ModId::new("addon"));
        assert_eq!(
            page.items[2].hint, None,
            "PartC has no scalar child besides defName"
        );
    }

    #[test]
    fn search_filters_before_paging() {
        let (sources, reader) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report_fixture(&["framework", "addon"]),
            &["framework", "addon"],
        );
        let use_case = ListItems::new(reader);

        let page = use_case
            .execute(
                &mut session,
                None,
                "example.PartDef",
                &ListItemsFilter {
                    search: Some("partb".to_string()),
                    offset: 0,
                    limit: 10,
                },
            )
            .expect("must succeed");

        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].def.def_name, "PartB");
    }

    #[test]
    fn paging_only_reads_the_current_pages_own_items() {
        let (sources, reader) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report_fixture(&["framework", "addon"]),
            &["framework", "addon"],
        );
        let use_case = ListItems::new(reader);

        let page = use_case
            .execute(
                &mut session,
                None,
                "example.PartDef",
                &ListItemsFilter {
                    search: None,
                    offset: 1,
                    limit: 1,
                },
            )
            .expect("must succeed");

        assert_eq!(
            page.total, 3,
            "total counts every match, not just this page"
        );
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].def.def_name, "PartB");
    }

    /// A project's own free-standing
    /// rows of `def_type` are prepended ahead of the active list's own
    /// matches — even a name that would sort *after* every active match
    /// alphabetically still comes first, and `total`/paging span both.
    #[test]
    fn own_rows_are_prepended_ahead_of_the_active_list() {
        let (sources, reader) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report_fixture(&["framework", "addon"]),
            &["framework", "addon"],
        );
        let mut fields = std::collections::BTreeMap::new();
        fields.insert(
            "hediffName".parse().unwrap(),
            rim_resolve::domain::FieldSpec {
                role: rim_resolve::domain::FieldRole::Scalar {
                    kind: rim_resolve::domain::ScalarKind::Text,
                    default: None,
                },
                cardinality: rim_resolve::domain::Cardinality::Scalar,
                observed: (1, 1),
                inferred_role: None,
            },
        );
        let schema = rim_resolve::domain::AssignmentSchema {
            def_type: "example.PartDef".to_string(),
            refs: BTreeSet::new(),
            fields,
            target_shapes: std::collections::BTreeMap::new(),
        };
        let mut project = rim_resolve::domain::AssignmentProject::new(
            rim_resolve::domain::AssignmentId::derive(
                "profile",
                &ModId::new("sample.newpart"),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "New Part".to_string(),
            rim_resolve::domain::PatchModIdentity::new("sample.newpart", "Sample's New Part")
                .expect("valid identity"),
            BTreeSet::new(),
            BTreeSet::new(),
            schema,
            jiff::Timestamp::UNIX_EPOCH,
        );
        struct NoneKnown;
        impl rim_resolve::domain::KnownDefs for NoneKnown {
            fn contains(&self, _def_type: &str, _name: &str) -> bool {
                false
            }
        }
        project
            .set_row(
                "example.PartDef",
                rim_resolve::domain::RowKey::Own("ZZZOwnPart".to_string()),
                rim_resolve::domain::AssignmentRow {
                    values: std::collections::BTreeMap::from([(
                        "hediffName".parse().unwrap(),
                        rim_resolve::domain::RowValue::Text("HediffOwn".to_string()),
                    )]),
                    def_name: "ZZZOwnPart".to_string(),
                    note: None,
                },
                &NoneKnown,
            )
            .expect("a valid free-standing row");
        let use_case = ListItems::new(reader);

        let page = use_case
            .execute(
                &mut session,
                Some(&project),
                "example.PartDef",
                &ListItemsFilter {
                    search: None,
                    offset: 0,
                    limit: 10,
                },
            )
            .expect("must succeed");

        assert_eq!(page.total, 4, "3 active + 1 own");
        let names: Vec<&str> = page.items.iter().map(|i| i.def.def_name.as_str()).collect();
        assert_eq!(
            names,
            vec!["ZZZOwnPart", "PartA", "PartB", "PartC"],
            "the own row sorts first regardless of alphabetical order: {names:?}"
        );
        assert_eq!(
            page.items[0].owner,
            ModId::new("sample.newpart"),
            "an own row's owner is the project's own package id"
        );
        assert_eq!(page.items[0].hint, Some("HediffOwn".to_string()));
    }

    /// Paging spans the combined (own + active) sequence: a window that
    /// starts inside the own rows and runs into the active list must
    /// still read only the active items it actually needs.
    #[test]
    fn own_rows_and_the_active_list_share_one_paging_window() {
        let (sources, reader) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report_fixture(&["framework", "addon"]),
            &["framework", "addon"],
        );
        struct NoneKnown;
        impl rim_resolve::domain::KnownDefs for NoneKnown {
            fn contains(&self, _def_type: &str, _name: &str) -> bool {
                false
            }
        }
        let schema = rim_resolve::domain::AssignmentSchema {
            def_type: "example.PartDef".to_string(),
            refs: BTreeSet::new(),
            fields: std::collections::BTreeMap::new(),
            target_shapes: std::collections::BTreeMap::new(),
        };
        let mut project = rim_resolve::domain::AssignmentProject::new(
            rim_resolve::domain::AssignmentId::derive(
                "profile",
                &ModId::new("sample.newpart"),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "New Part".to_string(),
            rim_resolve::domain::PatchModIdentity::new("sample.newpart", "Sample's New Part")
                .expect("valid identity"),
            BTreeSet::new(),
            BTreeSet::new(),
            schema,
            jiff::Timestamp::UNIX_EPOCH,
        );
        project
            .set_row(
                "example.PartDef",
                rim_resolve::domain::RowKey::Own("OwnPart".to_string()),
                rim_resolve::domain::AssignmentRow {
                    values: std::collections::BTreeMap::new(),
                    def_name: "OwnPart".to_string(),
                    note: None,
                },
                &NoneKnown,
            )
            .expect("a valid free-standing row");
        let use_case = ListItems::new(reader);

        // Window [1, 3): skips the one own row, takes the first two active
        // matches (`PartA`, `PartB`).
        let page = use_case
            .execute(
                &mut session,
                Some(&project),
                "example.PartDef",
                &ListItemsFilter {
                    search: None,
                    offset: 1,
                    limit: 2,
                },
            )
            .expect("must succeed");

        assert_eq!(page.total, 4);
        let names: Vec<&str> = page.items.iter().map(|i| i.def.def_name.as_str()).collect();
        assert_eq!(names, vec!["PartA", "PartB"]);
    }

    /// A `limit` above [`crate::MAX_PAGE_SIZE`] is clamped, never trusted
    /// verbatim — [`ListItemsFilter::limit`]'s own doc comment.
    #[test]
    fn a_limit_above_max_page_size_is_clamped() {
        let synthetic: Vec<(String, Vec<ModId>)> = (0..crate::MAX_PAGE_SIZE + 500)
            .map(|i| (i.to_string(), Vec::new()))
            .collect();

        let page = page_of(synthetic, 0, crate::MAX_PAGE_SIZE + 500);

        assert_eq!(page.len(), crate::MAX_PAGE_SIZE);
    }

    #[test]
    fn item_types_lists_every_distinct_type_owners_by_def_carries() {
        let (sources, _reader) = fixture();
        let session = session_with_sources_and_mods(
            sources,
            report_fixture(&["framework", "addon"]),
            &["framework", "addon"],
        );

        assert_eq!(
            item_types(&session),
            BTreeSet::from(["example.PartDef".to_string()])
        );
    }
}
