//! [`ModFilter`]/[`ModPage`]: search, filter, and page over a session's
//! active mod list — the same shape [`crate::FindingFilter`]/
//! [`crate::FindingPage`] give findings, moved here (rather than left to
//! each interface's own command) so every composition root gets the same
//! search/sort/cap behavior for free.

use std::collections::BTreeSet;

use rim_analyzer::domain::{InactiveMod, ModId, Report, Source};
use rim_resolve::domain::{Tag, Tagging};

use crate::MAX_PAGE_SIZE;

/// Filters and paging for [`crate::Session::mods`]. `limit` is capped at
/// [`MAX_PAGE_SIZE`] regardless of the requested value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModFilter {
    /// Only mods whose id or name contains this substring
    /// (case-insensitive).
    pub search: Option<String>,
    /// Only mods carrying this tag.
    pub tag: Option<Tag>,
    /// Only mods from this source.
    pub source: Option<Source>,
    /// How many matching mods to skip before collecting the page.
    pub offset: usize,
    /// How many mods to collect, capped at [`MAX_PAGE_SIZE`].
    pub limit: usize,
}

/// One mod summarized for a [`ModPage`] row: identity plus the derived
/// facts a mod list needs, so the caller never has to look tags/hard
/// dependents up separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModSummary {
    /// The mod's id.
    pub mod_id: ModId,
    /// Its display name.
    pub name: String,
    /// Where its files come from. `None` only for a [`Self::missing`]
    /// row — it was never found on disk, so there is no `About.xml` to
    /// have read a source from.
    pub source: Option<Source>,
    /// Every tag it currently carries. Always empty for a
    /// [`Self::missing`] row.
    pub tags: BTreeSet<Tag>,
    /// Distinct active mods with a Hard-strength edge pointing at it.
    /// Always `0` for a [`Self::missing`] row.
    pub hard_dependents: usize,
    /// Active per `ModsConfig.xml` but never found on disk — from
    /// `report.missing_mods`, which
    /// carries bare ids with no `About.xml` behind them at all, so
    /// `name` is the id's own text and `source`/`tags`/`hard_dependents`
    /// are the empty defaults above.
    pub missing: bool,
}

/// One page of [`crate::Session::mods`], plus the total number of mods
/// matching the filter (before paging).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModPage {
    /// Total mods matching the filter, before `offset`/`limit`.
    pub total: usize,
    /// This page's rows, sorted by id.
    pub items: Vec<ModSummary>,
}

/// Searches, filters, and pages `report`'s active mod list, **plus every
/// missing mod** —
/// active per `ModsConfig.xml` but never found on disk, so it has
/// no `report.mods` row of its own and would otherwise never reach a
/// caller at all. Server-side deliberately: search/source
/// filtering over a missing row shares exactly the same code path a
/// real row goes through, rather than a second, client-side
/// reimplementation of the same rule. A free function (not a method) so
/// [`crate::Session::mods`] can call it without borrowing more of `self`
/// than `report`/`tagging`.
pub(crate) fn query(report: &Report, tagging: &Tagging, filter: &ModFilter) -> ModPage {
    let search = filter.search.as_deref().map(str::to_lowercase);

    let mut matching: Vec<ModSummary> = report
        .mods
        .iter()
        .map(|mod_entry| ModSummary {
            mod_id: mod_entry.id.clone(),
            name: mod_entry.name.clone(),
            source: Some(mod_entry.source),
            tags: tagging.tags_of(&mod_entry.id).clone(),
            hard_dependents: mod_entry.hard_dependents,
            missing: false,
        })
        .chain(report.missing_mods.iter().map(|id| ModSummary {
            mod_id: id.clone(),
            name: id.to_string(),
            source: None,
            tags: BTreeSet::new(),
            hard_dependents: 0,
            missing: true,
        }))
        .filter(|row| {
            search.as_deref().is_none_or(|needle| {
                row.mod_id.as_str().to_lowercase().contains(needle)
                    || row.name.to_lowercase().contains(needle)
            })
        })
        .filter(|row| filter.tag.as_ref().is_none_or(|tag| row.tags.contains(tag)))
        .filter(|row| {
            filter
                .source
                .is_none_or(|source| row.source == Some(source))
        })
        .collect();
    matching.sort_by(|a, b| a.mod_id.cmp(&b.mod_id));

    let total = matching.len();
    let limit = filter.limit.min(MAX_PAGE_SIZE);
    let items = matching
        .into_iter()
        .skip(filter.offset)
        .take(limit)
        .collect();

    ModPage { total, items }
}

/// Searches, filters, and pages `inactive_mods` —
/// [`crate::Session::inactive_mods`]'s own worker. Only
/// `search`/`source` apply: an inactive mod carries no tags (`filter.tag`
/// is ignored, never treated as "match nothing") and no
/// Hard-strength-edge dependents (the analyzer only ever computes edges
/// between active mods), so every row's `tags` is empty and
/// `hard_dependents` is `0` — a free function, not a method, for the same
/// borrow-shape reason [`query`] is.
pub(crate) fn query_inactive(inactive_mods: &[InactiveMod], filter: &ModFilter) -> ModPage {
    let search = filter.search.as_deref().map(str::to_lowercase);

    let mut matching: Vec<_> = inactive_mods
        .iter()
        .filter(|mod_entry| {
            search.as_deref().is_none_or(|needle| {
                mod_entry.id.as_str().to_lowercase().contains(needle)
                    || mod_entry.name.to_lowercase().contains(needle)
            })
        })
        .filter(|mod_entry| {
            filter
                .source
                .is_none_or(|source| mod_entry.source == source)
        })
        .collect();
    matching.sort_by(|a, b| a.id.cmp(&b.id));

    let total = matching.len();
    let limit = filter.limit.min(MAX_PAGE_SIZE);
    let items = matching
        .into_iter()
        .skip(filter.offset)
        .take(limit)
        .map(|mod_entry| ModSummary {
            mod_id: mod_entry.id.clone(),
            name: mod_entry.name.clone(),
            source: Some(mod_entry.source),
            tags: BTreeSet::new(),
            hard_dependents: 0,
            missing: false,
        })
        .collect();

    ModPage { total, items }
}

#[cfg(test)]
mod tests {
    use rim_resolve::domain::{TagAssignment, TagProvenance};

    use super::*;

    fn report(ids: &[&str]) -> Report {
        let mut builder = rim_resolve::test_support::ReportBuilder::new();
        for id in ids {
            builder = builder.mod_(id);
        }
        builder.build()
    }

    #[test]
    fn search_matches_id_or_name_case_insensitively() {
        let report = report(&["framework.core", "addon.one"]);
        let page = query(
            &report,
            &Tagging::default(),
            &ModFilter {
                search: Some("FRAMEWORK".to_string()),
                limit: 10,
                ..ModFilter::default()
            },
        );
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].mod_id, ModId::new("framework.core"));
    }

    #[test]
    fn filters_by_tag() {
        let report = report(&["a", "b"]);
        let tag = Tag::new("framework").expect("valid tag");
        let tagging = Tagging::new(vec![TagAssignment {
            mod_id: ModId::new("a"),
            tag: tag.clone(),
            provenance: TagProvenance::Manual,
        }]);

        let page = query(
            &report,
            &tagging,
            &ModFilter {
                tag: Some(tag),
                limit: 10,
                ..ModFilter::default()
            },
        );

        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].mod_id, ModId::new("a"));
    }

    #[test]
    fn results_are_sorted_by_id_regardless_of_report_order() {
        let report = report(&["z.mod", "a.mod"]);
        let page = query(
            &report,
            &Tagging::default(),
            &ModFilter {
                limit: 10,
                ..ModFilter::default()
            },
        );
        assert_eq!(
            page.items
                .iter()
                .map(|m| m.mod_id.clone())
                .collect::<Vec<_>>(),
            vec![ModId::new("a.mod"), ModId::new("z.mod")]
        );
    }

    #[test]
    fn limit_is_capped_at_max_page_size() {
        let ids: Vec<String> = (0..MAX_PAGE_SIZE + 10).map(|i| format!("mod{i}")).collect();
        let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let report = report(&id_refs);

        let page = query(
            &report,
            &Tagging::default(),
            &ModFilter {
                limit: 10_000,
                ..ModFilter::default()
            },
        );

        assert_eq!(page.total, MAX_PAGE_SIZE + 10);
        assert_eq!(page.items.len(), MAX_PAGE_SIZE);
    }

    /// A missing mod (`report.missing_mods`) has no
    /// `report.mods` row of its own, so it must be merged into `query`'s
    /// own output — not recovered client-side — to be reachable at all.
    #[test]
    fn missing_mods_are_merged_in_as_rows_with_no_tags_or_source() {
        let mut report = report(&["a.mod"]);
        report.missing_mods.push(ModId::new("ghost.mod"));

        let page = query(
            &report,
            &Tagging::default(),
            &ModFilter {
                limit: 10,
                ..ModFilter::default()
            },
        );

        assert_eq!(page.total, 2);
        let ghost = page
            .items
            .iter()
            .find(|m| m.mod_id == ModId::new("ghost.mod"))
            .expect("the missing mod must appear as a row");
        assert!(ghost.missing);
        assert_eq!(ghost.name, "ghost.mod");
        assert_eq!(ghost.source, None);
        assert!(ghost.tags.is_empty());
        assert_eq!(ghost.hard_dependents, 0);
        let real = page
            .items
            .iter()
            .find(|m| m.mod_id == ModId::new("a.mod"))
            .expect("the real mod must still appear");
        assert!(!real.missing);
    }

    /// A missing mod's own `name` is its id, so the search filter still
    /// matches it by substring; a specific source filter excludes it
    /// (its own `source` is `None` — never found on disk, so it can
    /// never honestly match a real source).
    #[test]
    fn missing_mods_are_searchable_but_excluded_by_a_specific_source_filter() {
        let mut report = report(&["a.mod"]);
        report.missing_mods.push(ModId::new("ghost.mod"));

        let page = query(
            &report,
            &Tagging::default(),
            &ModFilter {
                search: Some("ghost".to_string()),
                limit: 10,
                ..ModFilter::default()
            },
        );
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].mod_id, ModId::new("ghost.mod"));

        let page = query(
            &report,
            &Tagging::default(),
            &ModFilter {
                source: Some(Source::Local),
                limit: 10,
                ..ModFilter::default()
            },
        );
        assert!(
            page.items.iter().all(|m| !m.missing),
            "a specific source filter must exclude every missing row"
        );
    }

    fn inactive_mods(ids: &[&str]) -> Vec<InactiveMod> {
        let mut builder = rim_resolve::test_support::ReportBuilder::new();
        for id in ids {
            builder = builder.inactive(id);
        }
        builder.build().inactive_mods
    }

    #[test]
    fn query_inactive_searches_by_id_or_name_and_sorts_by_id() {
        let inactive = inactive_mods(&["zzz.mod", "aaa.mod"]);

        let page = query_inactive(
            &inactive,
            &ModFilter {
                limit: 10,
                ..ModFilter::default()
            },
        );

        assert_eq!(page.total, 2);
        assert_eq!(
            page.items
                .iter()
                .map(|m| m.mod_id.clone())
                .collect::<Vec<_>>(),
            vec![ModId::new("aaa.mod"), ModId::new("zzz.mod")]
        );
        assert!(
            page.items
                .iter()
                .all(|m| m.tags.is_empty() && m.hard_dependents == 0)
        );
    }

    #[test]
    fn query_inactive_filters_by_source_and_ignores_tag() {
        let inactive = inactive_mods(&["a.mod", "b.mod"]);

        let page = query_inactive(
            &inactive,
            &ModFilter {
                tag: Some(Tag::new("framework").expect("valid tag")),
                limit: 10,
                ..ModFilter::default()
            },
        );

        assert_eq!(
            page.total, 2,
            "an inactive mod carries no tags, so a tag filter must not exclude it"
        );

        let page = query_inactive(
            &inactive,
            &ModFilter {
                source: Some(Source::Workshop),
                limit: 10,
                ..ModFilter::default()
            },
        );
        assert_eq!(page.total, 0, "the fixture's inactive mods are all Local");
    }
}
