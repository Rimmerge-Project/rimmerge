//! [`ChangeFilter`]/[`ChangePage`]: what one mod changes — the defs it
//! owns, the templates it registers, the foreign defs it patches, and the
//! assets it overrides —
//! search, filter, and page over that inventory the same shape
//! [`crate::mod_index`]/[`crate::finding_index`] already give mods and
//! findings.
//!
//! Pure and order-independent, unlike [`crate::Session::ledger`]: every
//! fact here comes from [`SourceIndex`] (built once per scan — the same
//! for `Current`/`Suggested`, since only which mods are *active* changes
//! either order, never who owns/patches what) and [`Report::conflicts`]
//! (likewise order-independent: a `DefOverride`/`PatchCollision`/asset
//! conflict exists because more than one active mod touches the same
//! thing, regardless of which order is selected). [`crate::Session::changes`]
//! therefore takes `&self` and touches none of the ledger/finding-index
//! caches [`crate::Session::invalidate_ledgers`] clears — nothing here
//! needs building or caching.

use std::collections::BTreeMap;

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{ModId, Report, Selector};
use rim_resolve::domain::{DefKey, DefRef, GeneratedMods};

use crate::MAX_PAGE_SIZE;
use conflicts::index_conflicts;
use inventory::{
    active_raw_ids, asset_rows, owns_def_rows, owns_template_rows, patches_def_rows, search_text,
};

mod conflicts;
mod inventory;
mod rows;

pub use rows::{AssetKind, ChangeFilter, ChangeKind, ChangePage, ChangeRow};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "changes/changes_tests.rs"]
mod tests;

/// Builds `mod_id`'s [`ChangePage`] — every def it owns, every template it
/// registers, every foreign def/template it patches, and every asset it
/// overrides, filtered/searched/paged like [`crate::mod_index::query`].
/// `pub(crate)`, called only from [`crate::Session::changes`], which owns
/// resolving `mod_id` through [`ModId::base`] the same way every other
/// mod-scoped query in this crate does.
pub(crate) fn query(
    mod_id: &ModId,
    report: &Report,
    sources: &SourceIndex,
    filter: &ChangeFilter,
) -> ChangePage {
    let target = mod_id.base();
    let raw_ids = active_raw_ids(report, &target);
    let conflicts = index_conflicts(report);
    let generated = GeneratedMods::from_report(report);

    let mut rows = owns_def_rows(sources, &conflicts, &generated, &target, &raw_ids);
    rows.extend(owns_template_rows(
        sources, &conflicts, &generated, &target, &raw_ids,
    ));
    rows.extend(patches_def_rows(
        sources, &conflicts, &generated, &target, &raw_ids,
    ));
    rows.extend(asset_rows(report, &generated, &target));

    // `kind_counts` is computed after `search` narrows the rows but
    // before `kinds` does — see `ChangePage::kind_counts`'s own doc
    // comment for why the order matters.
    let search = filter.search.as_deref().map(str::to_lowercase);
    let after_search: Vec<ChangeRow> = rows
        .into_iter()
        .filter(|row| {
            search
                .as_deref()
                .is_none_or(|needle| search_text(row).to_lowercase().contains(needle))
        })
        .collect();

    let mut kind_counts: BTreeMap<ChangeKind, usize> = BTreeMap::new();
    for row in &after_search {
        *kind_counts.entry(row.kind).or_default() += 1;
    }

    let mut matching: Vec<ChangeRow> = after_search
        .into_iter()
        .filter(|row| {
            filter
                .kinds
                .as_ref()
                .is_none_or(|kinds| kinds.contains(&row.kind))
        })
        .collect();
    matching.sort_by(|a, b| {
        b.other_touchers
            .cmp(&a.other_touchers)
            .then(a.kind.cmp(&b.kind))
            .then(a.def_ref.cmp(&b.def_ref))
            .then(a.asset_path.cmp(&b.asset_path))
    });

    let total = matching.len();
    let limit = filter.limit.min(MAX_PAGE_SIZE);
    let items = matching
        .into_iter()
        .skip(filter.offset)
        .take(limit)
        .collect();

    ChangePage {
        total,
        items,
        kind_counts,
    }
}

/// Which of `def_name`/`def_type` `needle` (already lowercased) matches —
/// `Some(0)` for the name (ranked first), `Some(1)` for the type only,
/// `None` for neither. `pub(crate)` doc per [`crate::Session::search_defs`]:
/// "case-insensitive substring on the name then the type".
fn match_rank(needle: &str, def_type: &str, def_name: &str) -> Option<u8> {
    if def_name.to_lowercase().contains(needle) {
        Some(0)
    } else if def_type.to_lowercase().contains(needle) {
        Some(1)
    } else {
        None
    }
}

/// Searches every def and `Name`-attributed template the scan indexed —
/// active mods only, since [`SourceIndex`] only ever indexes what the
/// scan actually found active — ranked name matches before type-only
/// matches, deterministic by [`DefRef`] within each rank, capped at
/// [`MAX_PAGE_SIZE`]. An empty `query` returns no hits at all rather than
/// every indexed def/template (every string trivially "contains" the
/// empty substring, so this needs its own guard rather than falling out
/// of [`match_rank`]) — the def page's search box has nothing useful to
/// show before the user types anything. `pub(crate)`, called only from
/// [`crate::Session::search_defs`].
pub(crate) fn search(query: &str, limit: usize, sources: &SourceIndex) -> Vec<(DefRef, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let needle = query.to_lowercase();
    let mut hits: Vec<(u8, DefRef, usize)> = Vec::new();

    for ((def_type, def_name), owners) in &sources.owners_by_def {
        if let Some(rank) = match_rank(&needle, def_type, def_name) {
            hits.push((
                rank,
                DefRef::new(
                    DefKey {
                        def_type: def_type.clone(),
                        def_name: def_name.clone(),
                    },
                    Selector::DefName,
                ),
                owners.len(),
            ));
        }
    }
    for ((def_type, name), owners) in &sources.templates {
        if let Some(rank) = match_rank(&needle, def_type, name) {
            hits.push((
                rank,
                DefRef::new(
                    DefKey {
                        def_type: def_type.clone(),
                        def_name: name.clone(),
                    },
                    Selector::NameAttr,
                ),
                owners.len(),
            ));
        }
    }

    hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    hits.into_iter()
        .take(limit.min(MAX_PAGE_SIZE))
        .map(|(_, def_ref, count)| (def_ref, count))
        .collect()
}
