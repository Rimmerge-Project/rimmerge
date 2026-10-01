//! Filters and pages an [`EffectiveDef`]'s flattened field list —
//! `InspectDef`'s def-page sibling of [`crate::merge_workspace`]'s own
//! `field_page` for the merge editor. Lives here, not in a DTO layer, for
//! the same reason that one does (`apps/desktop/CLAUDE.md`: "filtering,
//! paging, and aggregation live in rim-session"), so that
//! `apps/desktop/src-tauri/src/dto/defs.rs` and `apps/cli`'s own
//! `defs inspect` text output share one flatten/filter/page over
//! `EffectiveDef::resolved.leaves()` rather than each computing it.

use rim_merge::effective::{EffectiveDef, Provenance};
use rim_merge::tree::{Content, FieldNode, FieldPath, PathSegment};

/// One row of an [`EffectiveDef`]'s flattened field list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveField {
    /// The field's address under the def root.
    pub path: FieldPath,
    /// Indentation depth: the path's segment count minus one.
    pub depth: usize,
    /// Whether this field is a `li` list item, as opposed to a named leaf.
    pub is_list_item: bool,
    /// The field's rendered value; `None` for an empty leaf.
    pub value: Option<String>,
    /// Which stage last set it.
    pub provenance: Provenance,
}

/// Filters [`page`]'s field list. `limit` is capped at
/// [`crate::MAX_PAGE_SIZE`] regardless of the requested value — the same
/// cap [`crate::merge_workspace::MergeFieldFilter`] applies to its own
/// `limit`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectiveFieldFilter {
    /// Keep only rows [`crate::merge_workspace`] would *not* call
    /// `Provenance::Owner` — i.e. hide rows the winner's own raw node
    /// already set untouched, so the page reads as "what changed".
    pub only_patched: bool,
    /// How many matching rows to skip before collecting the page.
    pub offset: usize,
    /// How many rows to return, capped at [`crate::MAX_PAGE_SIZE`].
    pub limit: usize,
}

/// One page of [`page`]'s field list: how many rows matched the filter
/// (before paging), and the page of rows itself, in
/// `EffectiveDef::resolved`'s own document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveFieldPage {
    /// Rows matching the filter, before paging.
    pub total: usize,
    /// The requested page of rows.
    pub items: Vec<EffectiveField>,
}

/// Whether `path` addresses a `li` list item rather than a named leaf —
/// its last segment is [`PathSegment::Item`].
fn is_list_item(path: &FieldPath) -> bool {
    matches!(path.segments().last(), Some(PathSegment::Item(_)))
}

/// Renders one leaf/`li` [`FieldNode`]'s own value: `Content::Empty` ->
/// `None`, `Content::Text` -> its text, `Content::Children` (a whole `li`
/// item's subtree) -> its XML rendering — the same three-way mapping
/// [`rim_merge::diff::field_value`] applies for a diff, applied here
/// directly to a [`FieldNode`] since [`EffectiveDef`] hands back real
/// tree nodes, not diff values.
fn field_value(node: &FieldNode) -> Option<String> {
    match &node.content {
        Content::Empty => None,
        Content::Text(text) => Some(text.clone()),
        Content::Children(_) => Some(rim_merge::xml::render_node(node, 0).trim_end().to_string()),
    }
}

/// Every field of `effective`'s resolved tree, attributed to whichever
/// stage last set it — unfiltered, unpaged, in document order. A field
/// [`EffectiveDef::provenance`] has no entry for (see that field's own
/// doc comment: it mirrors `resolved`'s own leaves, not the pipeline's
/// full history) is skipped rather than shown with a guessed provenance.
pub fn fields(effective: &EffectiveDef) -> impl Iterator<Item = EffectiveField> + '_ {
    effective.resolved.leaves().filter_map(|(path, node)| {
        let provenance = effective.provenance.get(&path)?.clone();
        Some(EffectiveField {
            depth: path.segments().len().saturating_sub(1),
            is_list_item: is_list_item(&path),
            value: field_value(node),
            path,
            provenance,
        })
    })
}

/// Filters and pages [`fields`]'s output — pure computation over
/// already-cached data, the one flattener both the desktop DTO layer and
/// the CLI should call instead of each recomputing it (see this module's
/// own doc comment).
#[must_use]
pub fn page(effective: &EffectiveDef, filter: &EffectiveFieldFilter) -> EffectiveFieldPage {
    let matching: Vec<EffectiveField> = fields(effective)
        .filter(|field| !filter.only_patched || !matches!(field.provenance, Provenance::Owner(_)))
        .collect();
    let total = matching.len();
    let limit = filter.limit.min(crate::MAX_PAGE_SIZE);
    let items = matching
        .into_iter()
        .skip(filter.offset)
        .take(limit)
        .collect();
    EffectiveFieldPage { total, items }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_merge::effective::Completeness;

    use super::*;

    /// An [`EffectiveDef`] with `count` plain text leaves,
    /// `field0..fieldN`, every one attributed to `owner` — the fixtures
    /// this module's own tests need never come from a real
    /// `effective::compute` call, since paging is pure computation over
    /// an already-built `EffectiveDef`.
    fn effective_def_with_fields(count: usize, owner: &ModId) -> EffectiveDef {
        let body: String = (0..count)
            .map(|i| format!("<field{i}>value{i}</field{i}>"))
            .collect();
        let resolved =
            rim_merge::xml::parse(&format!("<ThingDef>{body}</ThingDef>")).expect("must parse");
        let provenance = resolved
            .leaves()
            .map(|(path, _)| (path, Provenance::Owner(owner.clone())))
            .collect();
        EffectiveDef {
            resolved,
            provenance,
            completeness: Completeness::Complete,
            caveats: Vec::new(),
            top_level_outcomes: Vec::new(),
            suppressed_filter_head_ops: 0,
        }
    }

    fn default_filter() -> EffectiveFieldFilter {
        EffectiveFieldFilter {
            only_patched: false,
            offset: 0,
            limit: 200,
        }
    }

    #[test]
    fn only_patched_hides_owner_rows() {
        let owner = ModId::new("owner.mod");
        let patcher = ModId::new("patcher.mod");
        let mut effective = effective_def_with_fields(3, &owner);
        let patched_path: FieldPath = "field1".parse().expect("valid field path");
        effective.provenance.insert(
            patched_path.clone(),
            Provenance::Patch {
                mod_id: patcher,
                op_index: 0,
            },
        );

        let result = page(
            &effective,
            &EffectiveFieldFilter {
                only_patched: true,
                ..default_filter()
            },
        );

        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].path, patched_path);
    }

    #[test]
    fn offset_and_limit_page_through_the_matching_rows() {
        let owner = ModId::new("owner.mod");
        let effective = effective_def_with_fields(5, &owner);

        let result = page(
            &effective,
            &EffectiveFieldFilter {
                offset: 2,
                limit: 2,
                ..default_filter()
            },
        );

        assert_eq!(
            result.total, 5,
            "total counts every matching row, not just the page"
        );
        assert_eq!(result.items.len(), 2, "the page itself is still limited");
        let paths: Vec<String> = result
            .items
            .iter()
            .map(|item| item.path.to_string())
            .collect();
        assert_eq!(paths, vec!["field2".to_string(), "field3".to_string()]);
    }

    #[test]
    fn the_page_is_clamped_to_max_page_size_regardless_of_the_requested_limit() {
        let owner = ModId::new("owner.mod");
        let effective = effective_def_with_fields(crate::MAX_PAGE_SIZE + 50, &owner);

        let result = page(
            &effective,
            &EffectiveFieldFilter {
                limit: 10_000,
                ..default_filter()
            },
        );

        assert_eq!(result.total, crate::MAX_PAGE_SIZE + 50);
        assert_eq!(result.items.len(), crate::MAX_PAGE_SIZE);
    }

    #[test]
    fn fields_matches_an_unpaged_page_field_for_field() {
        let owner = ModId::new("owner.mod");
        let effective = effective_def_with_fields(5, &owner);

        let unpaged: Vec<EffectiveField> = fields(&effective).collect();
        let paged = page(
            &effective,
            &EffectiveFieldFilter {
                limit: crate::MAX_PAGE_SIZE,
                ..default_filter()
            },
        );

        assert_eq!(unpaged, paged.items);
    }
}
