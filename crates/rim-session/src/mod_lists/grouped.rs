//! The Groups view of the Mods page: the pane's rows laid out as a flat,
//! deterministic sequence of folder headers and mod rows. Pure over the
//! rows [`super::working_rows`] / [`super::inactive_rows`] build and the
//! loaded organization, so placement can never disagree with the labels a
//! row carries and no second membership rule exists.
//!
//! The rows ride along once (`list.items`); an entry only names a mod id,
//! so a mod in many groups multiplies a small entry, not a row. Entries are
//! bounded by [`MAX_GROUPED_ENTRIES`] with `total_entries` counted before
//! the cut, the same visible-cut pattern as [`super::MAX_MOD_LIST_ROWS`].

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;

use super::{GroupFilter, MAX_MOD_LIST_ROWS, ModList, ModListError, ModListQuery, ModListRow};
use crate::organization::{
    ColorToken, GroupId, GroupName, GroupNode, ModOrganization, OrganizationLimits,
};

/// The most entries a grouped list returns. Worst case under the
/// organization limits is about 1,000 headers plus 50,000 memberships plus
/// 5,000 ungrouped rows; collapsing folders is the user's way below the cut.
pub const MAX_GROUPED_ENTRIES: usize = 10_000;

/// The most collapsed-folder keys a query may carry: every group plus the
/// "Ungrouped" folder.
pub const MAX_COLLAPSED_FOLDERS: usize = OrganizationLimits::MAX_GROUPS + 1;

/// Which folder of the Groups view an entry or a fold refers to. Ordered
/// groups by id, then "Ungrouped".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FolderKey {
    /// A group's folder.
    Group(GroupId),
    /// The folder of mods that are in no group.
    Ungrouped,
}

/// A folder header. Closed: "Ungrouped" has no name or colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Folder {
    /// A group.
    Group {
        /// The group.
        id: GroupId,
        /// Its name.
        name: GroupName,
        /// Its colour.
        color: Option<ColorToken>,
    },
    /// The mods that are in no group.
    Ungrouped,
}

/// More collapsed-folder keys than [`MAX_COLLAPSED_FOLDERS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{count} collapsed folders, at most {} allowed", MAX_COLLAPSED_FOLDERS)]
pub struct TooManyCollapsedFolders {
    /// How many distinct keys were given.
    pub count: usize,
}

/// A grouped list request: the pane's own filter and sort plus the folds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupedListQuery {
    /// The pane's own filter and sort, unchanged.
    pub list: ModListQuery,
    /// The folders folded shut. A key naming no folder is ignored, unlike a
    /// stale group filter: a stale fold changes nothing.
    collapsed: BTreeSet<FolderKey>,
}

impl GroupedListQuery {
    /// A query over `list` with `collapsed` folders folded shut.
    ///
    /// # Errors
    ///
    /// [`TooManyCollapsedFolders`] past [`MAX_COLLAPSED_FOLDERS`] distinct keys.
    pub fn new(
        list: ModListQuery,
        collapsed: BTreeSet<FolderKey>,
    ) -> Result<Self, TooManyCollapsedFolders> {
        if collapsed.len() > MAX_COLLAPSED_FOLDERS {
            return Err(TooManyCollapsedFolders {
                count: collapsed.len(),
            });
        }
        Ok(Self { list, collapsed })
    }

    /// The folded folders.
    #[must_use]
    pub fn collapsed(&self) -> &BTreeSet<FolderKey> {
        &self.collapsed
    }
}

/// One line of the Groups view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupedEntry {
    /// A folder header.
    Folder {
        /// Which folder.
        folder: Folder,
        /// 1 is the top level of this view (rebased under a group scope).
        depth: usize,
        /// Distinct mods in the folder and every subfolder, in this pane,
        /// admitted by the pane's whole query. Collapse never changes it.
        matched: usize,
        /// Whether the folder is folded shut (its descendants are omitted).
        is_collapsed: bool,
    },
    /// One copy of a mod inside a folder.
    Mod {
        /// Joins to `list.items`.
        mod_id: ModId,
        /// The folder this copy sits in.
        folder: FolderKey,
        /// The folder's depth plus one.
        depth: usize,
        /// The mod's other direct groups (its label count minus this one).
        also_in: usize,
    },
}

/// The Groups view of one pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupedModList {
    /// The same rows the List view returns for the query; `reorderable` is
    /// always `false` here.
    pub list: ModList,
    /// The visible entries, at most [`MAX_GROUPED_ENTRIES`].
    pub entries: Vec<GroupedEntry>,
    /// Entries before the cut, so a cut is visible.
    pub total_entries: usize,
}

/// Lays `rows` (uncut, decorated, filtered and sorted) out as folders.
///
/// # Errors
///
/// [`ModListError::UnknownGroup`] when the query's group scope names a group
/// the organization lacks. The row builders refuse that first, so this is a
/// second guard rather than a path a caller reaches.
pub(super) fn build(
    organization: &ModOrganization,
    rows: Vec<ModListRow>,
    query: &GroupedListQuery,
) -> Result<GroupedModList, ModListError> {
    let (entries, total_entries) = Layout::new(organization, &rows, query).entries()?;
    Ok(GroupedModList {
        list: ModList::from_rows(rows.into_iter(), false),
        entries,
        total_entries,
    })
}

/// Whether the pane's own query narrows rows beyond the group scope. An
/// empty search is no filter.
fn has_row_filter(query: &ModListQuery) -> bool {
    query.search.as_deref().is_some_and(|text| !text.is_empty())
        || query.tag.is_some()
        || query.source.is_some()
}

/// Entries in order, counted past the cut.
struct Emitter {
    entries: Vec<GroupedEntry>,
    total: usize,
}

impl Emitter {
    fn push(&mut self, entry: GroupedEntry) {
        self.total += 1;
        if self.entries.len() < MAX_GROUPED_ENTRIES {
            self.entries.push(entry);
        }
    }
}

/// Everything one grouped call derives from the rows, built once.
struct Layout<'a> {
    rows: &'a [ModListRow],
    query: &'a GroupedListQuery,
    has_row_filter: bool,
    /// The forest depth first, siblings by name.
    nodes: Vec<GroupNode>,
    /// Indices into `nodes` of the top-level groups, in tree order.
    roots: Vec<usize>,
    /// Per node (same index as `nodes`), its subgroups' indices in tree order.
    children: Vec<Vec<usize>>,
    /// Row indices (ascending, so in pane order) per direct group. A group
    /// with no member has no entry, which is an empty list, not a gap.
    direct: BTreeMap<GroupId, Vec<usize>>,
    /// Row indices that are in no group.
    ungrouped: Vec<usize>,
    /// Per node (same index as `nodes`), the distinct matched mods in it,
    /// its subfolders included.
    matched: Vec<usize>,
}

/// The shape of a depth-first node list, by index.
struct Topology {
    roots: Vec<usize>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
}

impl Topology {
    /// A node's parent is the nearest earlier node one level up, which a
    /// depth-first list always has, so no id lookup can miss.
    fn of(nodes: &[GroupNode]) -> Self {
        let mut topology = Self {
            roots: Vec::new(),
            children: vec![Vec::new(); nodes.len()],
            parents: Vec::with_capacity(nodes.len()),
        };
        let mut ancestors: Vec<usize> = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            ancestors.truncate(node.depth.saturating_sub(1));
            let parent = ancestors.last().copied();
            topology.parents.push(parent);
            match parent {
                Some(parent) => topology.children[parent].push(index),
                None => topology.roots.push(index),
            }
            ancestors.push(index);
        }
        topology
    }
}

impl<'a> Layout<'a> {
    fn new(
        organization: &ModOrganization,
        rows: &'a [ModListRow],
        query: &'a GroupedListQuery,
    ) -> Self {
        // `tree()` once: calling it (or `scope()`) per node would walk each
        // subtree once per ancestor.
        let nodes = organization.tree();
        let topology = Topology::of(&nodes);
        let (direct, ungrouped) = place(rows);
        let matched = scope_counts(&nodes, &topology.parents, &direct);
        Self {
            rows,
            query,
            has_row_filter: has_row_filter(&query.list),
            nodes,
            roots: topology.roots,
            children: topology.children,
            direct,
            ungrouped,
            matched,
        }
    }

    /// The entries and the count before the cut.
    fn entries(&self) -> Result<(Vec<GroupedEntry>, usize), ModListError> {
        let mut out = Emitter {
            entries: Vec::new(),
            total: 0,
        };
        match self.query.list.group {
            Some(GroupFilter::Ungrouped) => self.emit_ungrouped(&mut out),
            Some(GroupFilter::InScope(id)) => {
                let root = self
                    .nodes
                    .iter()
                    .position(|node| node.id == id)
                    .ok_or(ModListError::UnknownGroup(id))?;
                self.emit_folder(root, self.nodes[root].depth, &mut out);
            }
            None => {
                for &root in &self.roots {
                    self.emit_folder(root, 1, &mut out);
                }
                self.emit_ungrouped(&mut out);
            }
        }
        Ok((out.entries, out.total))
    }

    /// A folder's header, then (unless folded) its subfolders, then its
    /// own mods. `base_depth` is the scope root's depth, so the root shows
    /// at level 1.
    fn emit_folder(&self, index: usize, base_depth: usize, out: &mut Emitter) {
        let node = &self.nodes[index];
        let matched = self.matched[index];
        // A folder with no match holds no match below it either, so a
        // filtered view leaves the whole branch out.
        if self.has_row_filter && matched == 0 {
            return;
        }
        let depth = node.depth + 1 - base_depth;
        let is_collapsed = self.query.collapsed().contains(&FolderKey::Group(node.id));
        out.push(GroupedEntry::Folder {
            folder: Folder::Group {
                id: node.id,
                name: node.name.clone(),
                color: node.color,
            },
            depth,
            matched,
            is_collapsed,
        });
        if is_collapsed {
            return;
        }
        for &child in &self.children[index] {
            self.emit_folder(child, base_depth, out);
        }
        let members = self.direct.get(&node.id).into_iter().flatten();
        for &row in members {
            self.emit_mod(row, FolderKey::Group(node.id), depth + 1, out);
        }
    }

    fn emit_ungrouped(&self, out: &mut Emitter) {
        if self.ungrouped.is_empty() {
            return;
        }
        let is_collapsed = self.query.collapsed().contains(&FolderKey::Ungrouped);
        out.push(GroupedEntry::Folder {
            folder: Folder::Ungrouped,
            depth: 1,
            matched: self.ungrouped.len(),
            is_collapsed,
        });
        if is_collapsed {
            return;
        }
        for &row in &self.ungrouped {
            self.emit_mod(row, FolderKey::Ungrouped, 2, out);
        }
    }

    fn emit_mod(&self, row_index: usize, folder: FolderKey, depth: usize, out: &mut Emitter) {
        // A row past the list's own cut has no `items` entry to join to:
        // it is counted, never emitted, so an entry never dangles.
        if row_index >= MAX_MOD_LIST_ROWS {
            out.total += 1;
            return;
        }
        let row = &self.rows[row_index];
        out.push(GroupedEntry::Mod {
            mod_id: row.mod_id.clone(),
            folder,
            depth,
            also_in: row.groups.len().saturating_sub(1),
        });
    }
}

/// Which rows sit directly in which group, by the labels the rows carry.
fn place(rows: &[ModListRow]) -> (BTreeMap<GroupId, Vec<usize>>, Vec<usize>) {
    let mut direct: BTreeMap<GroupId, Vec<usize>> = BTreeMap::new();
    let mut ungrouped = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if row.groups.is_empty() {
            ungrouped.push(index);
        }
        for label in &row.groups {
            direct.entry(label.id).or_default().push(index);
        }
    }
    (direct, ungrouped)
}

/// Distinct rows per group scope, one count per node, bottom up: in reverse
/// preorder every node's subtree has been merged into it already, so each
/// row index moves at most `depth` times and no subtree is walked twice.
/// `parents[i]` is node `i`'s parent index, always below `i`.
fn scope_counts(
    nodes: &[GroupNode],
    parents: &[Option<usize>],
    direct: &BTreeMap<GroupId, Vec<usize>>,
) -> Vec<usize> {
    let mut sets: Vec<BTreeSet<usize>> = nodes
        .iter()
        .map(|node| {
            direct
                .get(&node.id)
                .into_iter()
                .flatten()
                .copied()
                .collect()
        })
        .collect();
    let mut counts = vec![0; nodes.len()];
    for (index, parent) in parents.iter().enumerate().rev() {
        let set = std::mem::take(&mut sets[index]);
        counts[index] = set.len();
        if let Some(parent) = *parent {
            sets[parent].extend(set);
        }
    }
    counts
}

#[cfg(test)]
#[path = "grouped_tests.rs"]
mod tests;
