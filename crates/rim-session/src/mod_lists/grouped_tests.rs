//! The Groups view read model: placement, order, counts, folds, bounds.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{ModId, Report, Source};
use rim_resolve::domain::{Tag, TagAssignment, TagProvenance, Tagging};
use rim_resolve::test_support::ReportBuilder;

use super::*;
use crate::active_set::ActiveSet;
use crate::file_baseline::FileMembership;
use crate::mod_lists::{
    ListSort, ListSortKey, ModListError, SortDirection, inactive_groups, working_groups,
};
use crate::mod_times::{ModUpdateTimes, UnixSeconds, UpdateTime, UpdateUnavailable};
use crate::organization::{
    GroupParts, ModOrganization, OrganizationIssue, OrganizationState, OrganizedModId,
};

fn member(id: &str) -> OrganizedModId {
    OrganizedModId::new(&ModId::new(id)).expect("valid fixture id")
}

fn working(ids: &[&str]) -> ActiveSet {
    ActiveSet::trusted(ids.iter().map(|id| ModId::new(*id)).collect())
}

/// One group of a fixture forest: name, parent's name, direct members.
type GroupSpec<'a> = (&'a str, Option<&'a str>, &'a [&'a str]);

/// A loaded organization from `specs` (a parent is listed before its
/// children), with each group's id by name.
fn organization(specs: &[GroupSpec<'_>]) -> (OrganizationState, BTreeMap<String, GroupId>) {
    let mut forest = ModOrganization::new();
    let mut ids: BTreeMap<String, GroupId> = BTreeMap::new();
    for (name, parent, members) in specs {
        let parent_id = parent.map(|parent| ids[parent]);
        let id = forest
            .create_group(GroupName::new(*name).expect("valid name"), parent_id)
            .expect("create");
        let members: BTreeSet<OrganizedModId> = members.iter().map(|id| member(id)).collect();
        forest.add_members(id, &members).expect("add");
        ids.insert((*name).to_string(), id);
    }
    (OrganizationState::Loaded(forest), ids)
}

/// Armor { a, d }, Empty, Weapons { a, b } > Melee { b, c } > Blades { c };
/// `e` and `f` are in no group.
const SCENE: &[GroupSpec<'static>] = &[
    ("Weapons", None, &["a", "b"]),
    ("Melee", Some("Weapons"), &["b", "c"]),
    ("Blades", Some("Melee"), &["c"]),
    ("Armor", None, &["a", "d"]),
    ("Empty", None, &[]),
];

/// Active `a`..`f` in load order; names make the Name sort differ from
/// load order inside folders.
fn scene_report() -> Report {
    let mut builder = ReportBuilder::new();
    for (id, name) in [
        ("a", "Alpha"),
        ("b", "Delta"),
        ("c", "Charlie"),
        ("d", "Bravo"),
        ("e", "Echo"),
        ("f", "Foxtrot"),
    ] {
        let name = name.to_string();
        builder = builder.mod_with(id, move |m| m.name = name);
    }
    builder.build()
}

fn scene_working() -> ActiveSet {
    working(&["a", "b", "c", "d", "e", "f"])
}

fn query(list: ModListQuery, collapsed: &[FolderKey]) -> GroupedListQuery {
    GroupedListQuery::new(list, collapsed.iter().copied().collect()).expect("within bound")
}

fn plain() -> GroupedListQuery {
    query(ModListQuery::default(), &[])
}

fn sorted(key: ListSortKey, direction: SortDirection) -> ModListQuery {
    ModListQuery {
        sort: Some(ListSort { key, direction }),
        ..ModListQuery::default()
    }
}

fn active_groups(
    report: &Report,
    state: &OrganizationState,
    query: &GroupedListQuery,
) -> Result<GroupedModList, ModListError> {
    working_groups(
        report,
        &Tagging::default(),
        &scene_working(),
        &ModUpdateTimes::default(),
        &FileMembership::default(),
        state,
        query,
    )
}

fn scene_active(query: &GroupedListQuery) -> GroupedModList {
    let (state, _) = organization(SCENE);
    active_groups(&scene_report(), &state, query).expect("groups are loaded")
}

fn inactive_of(
    report: &Report,
    active: &[&str],
    state: &OrganizationState,
) -> Result<GroupedModList, ModListError> {
    inactive_groups(
        report,
        &Tagging::default(),
        &working(active),
        &ModUpdateTimes::default(),
        &FileMembership::default(),
        state,
        &plain(),
    )
}

/// One line per entry: `depth name (matched)` for a folder (`~` is "Not in
/// a group", `[folded]` a collapsed one), `depth - id +also_in` for a mod.
fn render(list: &GroupedModList) -> Vec<String> {
    list.entries
        .iter()
        .map(|entry| match entry {
            GroupedEntry::Folder {
                folder,
                depth,
                matched,
                is_collapsed,
            } => {
                let name = match folder {
                    Folder::Group { name, .. } => name.as_str(),
                    Folder::Ungrouped => "~",
                };
                let fold = if *is_collapsed { " [folded]" } else { "" };
                format!("{depth} {name} ({matched}){fold}")
            }
            GroupedEntry::Mod {
                mod_id,
                depth,
                also_in,
                ..
            } => {
                let also = if *also_in > 0 {
                    format!(" +{also_in}")
                } else {
                    String::new()
                };
                format!("{depth} - {mod_id}{also}")
            }
        })
        .collect()
}

fn mod_entries(list: &GroupedModList) -> Vec<(FolderKey, &ModId)> {
    list.entries
        .iter()
        .filter_map(|entry| match entry {
            GroupedEntry::Mod { folder, mod_id, .. } => Some((*folder, mod_id)),
            GroupedEntry::Folder { .. } => None,
        })
        .collect()
}

fn ids_in(list: &GroupedModList, folder: FolderKey) -> Vec<&str> {
    mod_entries(list)
        .into_iter()
        .filter(|(key, _)| *key == folder)
        .map(|(_, id)| id.as_str())
        .collect()
}

#[test]
fn folders_nest_in_tree_order_with_subfolders_before_the_folders_own_mods() {
    let list = scene_active(&plain());

    assert_eq!(
        render(&list),
        [
            "1 Armor (2)",
            "2 - a +1",
            "2 - d",
            "1 Empty (0)",
            "1 Weapons (3)",
            "2 Melee (2)",
            "3 Blades (1)",
            "4 - c +1",
            "3 - b +1",
            "3 - c +1",
            "2 - a +1",
            "2 - b +1",
            "1 ~ (2)",
            "2 - e",
            "2 - f",
        ]
    );
    assert_eq!(list.total_entries, list.entries.len());
}

#[test]
fn a_member_of_a_parent_and_its_child_appears_in_both_once_each() {
    let (state, ids) = organization(SCENE);

    let list = active_groups(&scene_report(), &state, &plain()).expect("loaded");

    let copies_of_b: Vec<FolderKey> = mod_entries(&list)
        .into_iter()
        .filter(|(_, id)| id.as_str() == "b")
        .map(|(folder, _)| folder)
        .collect();
    assert_eq!(
        copies_of_b,
        [
            FolderKey::Group(ids["Melee"]),
            FolderKey::Group(ids["Weapons"])
        ]
    );
}

#[test]
fn a_mod_in_two_groups_shows_under_each_and_counts_once_in_a_common_ancestor() {
    let (state, ids) = organization(SCENE);

    let list = active_groups(&scene_report(), &state, &plain()).expect("loaded");

    // `a` sits in Armor and Weapons: two copies, each says "1 other".
    let copies_of_a: Vec<(FolderKey, usize)> = list
        .entries
        .iter()
        .filter_map(|entry| match entry {
            GroupedEntry::Mod {
                mod_id,
                folder,
                also_in,
                ..
            } if mod_id.as_str() == "a" => Some((*folder, *also_in)),
            _ => None,
        })
        .collect();
    assert_eq!(
        copies_of_a,
        [
            (FolderKey::Group(ids["Armor"]), 1),
            (FolderKey::Group(ids["Weapons"]), 1)
        ]
    );
    // Weapons holds a and b directly and b, c in Melee: b is one mod.
    let weapons = list.entries.iter().find_map(|entry| match entry {
        GroupedEntry::Folder {
            folder: Folder::Group { id, .. },
            matched,
            ..
        } if *id == ids["Weapons"] => Some(*matched),
        _ => None,
    });
    assert_eq!(weapons, Some(3));
}

#[test]
fn mods_in_no_group_come_last_and_the_folder_is_absent_when_there_are_none() {
    let (state, _) = organization(&[("All", None, &["a", "b", "c", "d", "e", "f"])]);

    let list = active_groups(&scene_report(), &state, &plain()).expect("loaded");

    assert_eq!(render(&list)[0], "1 All (6)");
    assert!(!list.entries.iter().any(|entry| matches!(
        entry,
        GroupedEntry::Folder {
            folder: Folder::Ungrouped,
            ..
        }
    )));
}

#[test]
fn a_group_scope_omits_the_ungrouped_folder() {
    let (state, ids) = organization(SCENE);
    let scoped = ModListQuery {
        group: Some(GroupFilter::InScope(ids["Armor"])),
        ..ModListQuery::default()
    };

    let list = active_groups(&scene_report(), &state, &query(scoped, &[])).expect("loaded");

    assert_eq!(render(&list), ["1 Armor (2)", "2 - a +1", "2 - d"]);
}

#[test]
fn a_group_scope_rebases_depth_and_walks_only_that_subtree() {
    let (state, ids) = organization(SCENE);
    let scoped = ModListQuery {
        group: Some(GroupFilter::InScope(ids["Melee"])),
        ..ModListQuery::default()
    };

    let list = active_groups(&scene_report(), &state, &query(scoped, &[])).expect("loaded");

    assert_eq!(
        render(&list),
        [
            "1 Melee (2)",
            "2 Blades (1)",
            "3 - c +1",
            "2 - b +1",
            "2 - c +1",
        ]
    );
    // Row facts are unchanged by the rebase: `items` is the scope's rows.
    let items: Vec<&str> = list.list.items.iter().map(|r| r.mod_id.as_str()).collect();
    assert_eq!(items, ["b", "c"]);
}

#[test]
fn the_ungrouped_scope_emits_only_the_ungrouped_folder() {
    let scoped = ModListQuery {
        group: Some(GroupFilter::Ungrouped),
        ..ModListQuery::default()
    };

    let list = scene_active(&query(scoped, &[]));

    assert_eq!(render(&list), ["1 ~ (2)", "2 - e", "2 - f"]);
}

#[test]
fn name_descending_orders_the_mods_inside_every_folder() {
    let list = scene_active(&query(
        sorted(ListSortKey::Name, SortDirection::Descending),
        &[],
    ));

    // Alpha a, Delta b, Charlie c, Bravo d, Echo e, Foxtrot f.
    assert_eq!(
        render(&list),
        [
            "1 Armor (2)",
            "2 - d",
            "2 - a +1",
            "1 Empty (0)",
            "1 Weapons (3)",
            "2 Melee (2)",
            "3 Blades (1)",
            "4 - c +1",
            "3 - b +1",
            "3 - c +1",
            "2 - b +1",
            "2 - a +1",
            "1 ~ (2)",
            "2 - f",
            "2 - e",
        ]
    );
}

#[test]
fn last_updated_puts_unavailable_times_last_in_both_directions_inside_a_folder() {
    let known = |seconds: u64| UpdateTime::Known(UnixSeconds::new(seconds));
    let times = ModUpdateTimes::new(
        [
            (ModId::new("b"), known(300)),
            (ModId::new("c"), known(200)),
            (ModId::new("d"), known(100)),
            (
                ModId::new("a"),
                UpdateTime::Unavailable(UpdateUnavailable::Unknown),
            ),
        ]
        .into_iter()
        .collect(),
    );
    let (state, ids) = organization(SCENE);
    let armor = FolderKey::Group(ids["Armor"]);

    for direction in [SortDirection::Ascending, SortDirection::Descending] {
        let list = working_groups(
            &scene_report(),
            &Tagging::default(),
            &scene_working(),
            &times,
            &FileMembership::default(),
            &state,
            &query(sorted(ListSortKey::LastUpdated, direction), &[]),
        )
        .expect("loaded");

        // `a` has no time, so it follows `d` whichever way the sort runs
        // (load order alone would list `a` first).
        assert_eq!(ids_in(&list, armor), ["d", "a"], "{direction:?}");
    }
}

#[test]
fn inside_every_folder_the_mods_keep_the_order_of_the_list_views_items() {
    let (state, _) = organization(SCENE);
    let sorts = [
        ModListQuery::default(),
        sorted(ListSortKey::Name, SortDirection::Ascending),
        sorted(ListSortKey::Name, SortDirection::Descending),
        sorted(ListSortKey::PackageId, SortDirection::Descending),
        sorted(ListSortKey::LoadOrder, SortDirection::Descending),
    ];

    for list_query in sorts {
        let list =
            active_groups(&scene_report(), &state, &query(list_query.clone(), &[])).expect("ok");

        let folders: BTreeSet<FolderKey> = mod_entries(&list).iter().map(|(f, _)| *f).collect();
        for folder in folders {
            let in_folder = ids_in(&list, folder);
            let expected: Vec<&str> = list
                .list
                .items
                .iter()
                .map(|row| row.mod_id.as_str())
                .filter(|id| in_folder.contains(id))
                .collect();
            assert_eq!(in_folder, expected, "{list_query:?} in {folder:?}");
        }
    }
}

#[test]
fn a_search_leaves_out_folders_with_no_match_and_counts_only_matches() {
    let searching = ModListQuery {
        search: Some("alpha".to_string()),
        ..ModListQuery::default()
    };

    let list = scene_active(&query(searching, &[]));

    // Only `a` matches: Armor and Weapons hold it, the rest are left out.
    assert_eq!(
        render(&list),
        ["1 Armor (1)", "2 - a +1", "1 Weapons (1)", "2 - a +1"]
    );
}

#[test]
fn an_empty_search_is_no_filter_so_empty_folders_stay() {
    let blank = ModListQuery {
        search: Some(String::new()),
        ..ModListQuery::default()
    };

    let list = scene_active(&query(blank, &[]));

    assert!(render(&list).contains(&"1 Empty (0)".to_string()));
}

#[test]
fn without_a_row_filter_empty_folders_show_with_a_zero_count() {
    let list = scene_active(&plain());

    assert!(render(&list).contains(&"1 Empty (0)".to_string()));
}

#[test]
fn a_tag_filter_and_a_source_filter_narrow_folders_like_a_search() {
    let tag = Tag::new("weapons").expect("valid tag");
    let tagging = Tagging::new(vec![TagAssignment {
        mod_id: ModId::new("d"),
        tag: tag.clone(),
        provenance: TagProvenance::Manual,
    }]);
    let (state, _) = organization(SCENE);
    let by_tag = working_groups(
        &scene_report(),
        &tagging,
        &scene_working(),
        &ModUpdateTimes::default(),
        &FileMembership::default(),
        &state,
        &query(
            ModListQuery {
                tag: Some(tag),
                ..ModListQuery::default()
            },
            &[],
        ),
    )
    .expect("loaded");
    let workshop_report = ReportBuilder::new()
        .mod_("a")
        .mod_with("b", |m| m.source = Source::Workshop)
        .build();
    let by_source = working_groups(
        &workshop_report,
        &Tagging::default(),
        &working(&["a", "b"]),
        &ModUpdateTimes::default(),
        &FileMembership::default(),
        &state,
        &query(
            ModListQuery {
                source: Some(Source::Workshop),
                ..ModListQuery::default()
            },
            &[],
        ),
    )
    .expect("loaded");

    assert_eq!(render(&by_tag), ["1 Armor (1)", "2 - d"]);
    assert_eq!(
        render(&by_source),
        ["1 Weapons (1)", "2 Melee (1)", "3 - b +1", "2 - b +1"]
    );
}

#[test]
fn counts_are_distinct_across_subfolders_and_unchanged_by_collapse() {
    let (state, ids) = organization(SCENE);
    let all_folded = [
        FolderKey::Group(ids["Weapons"]),
        FolderKey::Group(ids["Armor"]),
        FolderKey::Ungrouped,
    ];

    let list = active_groups(
        &scene_report(),
        &state,
        &query(ModListQuery::default(), &all_folded),
    )
    .expect("loaded");

    assert_eq!(
        render(&list),
        [
            "1 Armor (2) [folded]",
            "1 Empty (0)",
            "1 Weapons (3) [folded]",
            "1 ~ (2) [folded]",
        ]
    );
}

#[test]
fn a_collapsed_folder_keeps_its_header_and_omits_subfolders_and_mods() {
    let (state, ids) = organization(SCENE);
    let folded = [FolderKey::Group(ids["Melee"])];

    let list = active_groups(
        &scene_report(),
        &state,
        &query(ModListQuery::default(), &folded),
    )
    .expect("loaded");

    let lines = render(&list);
    assert!(lines.contains(&"2 Melee (2) [folded]".to_string()));
    assert!(!lines.iter().any(|line| line.contains("Blades")));
    // Weapons' own mods and everything outside Melee are still there.
    assert!(lines.contains(&"2 - a +1".to_string()));
    assert!(lines.contains(&"1 ~ (2)".to_string()));
}

#[test]
fn a_fold_under_a_collapsed_parent_changes_nothing_visible() {
    let (state, ids) = organization(SCENE);
    let parent_only = [FolderKey::Group(ids["Weapons"])];
    let both = [
        FolderKey::Group(ids["Weapons"]),
        FolderKey::Group(ids["Blades"]),
    ];

    let one = active_groups(
        &scene_report(),
        &state,
        &query(ModListQuery::default(), &parent_only),
    )
    .expect("loaded");
    let two = active_groups(
        &scene_report(),
        &state,
        &query(ModListQuery::default(), &both),
    )
    .expect("loaded");

    assert_eq!(one.entries, two.entries);
}

#[test]
fn folding_the_ungrouped_folder_keeps_its_header_and_count_only() {
    let list = scene_active(&query(ModListQuery::default(), &[FolderKey::Ungrouped]));

    let lines = render(&list);
    assert_eq!(lines.last().map(String::as_str), Some("1 ~ (2) [folded]"));
    assert!(!lines.contains(&"2 - e".to_string()));
}

#[test]
fn a_fold_naming_no_folder_is_ignored() {
    let stale = GroupId::new(999).expect("non-zero");

    let list = scene_active(&query(ModListQuery::default(), &[FolderKey::Group(stale)]));

    assert_eq!(list.entries, scene_active(&plain()).entries);
}

#[test]
fn more_than_a_thousand_and_one_collapsed_keys_are_refused() {
    let keys = |count: u32| -> BTreeSet<FolderKey> {
        (1..=count)
            .map(|raw| FolderKey::Group(GroupId::new(raw).expect("non-zero")))
            .collect()
    };
    let at_the_bound = u32::try_from(MAX_COLLAPSED_FOLDERS).expect("fits");

    assert!(GroupedListQuery::new(ModListQuery::default(), keys(at_the_bound)).is_ok());
    assert_eq!(
        GroupedListQuery::new(ModListQuery::default(), keys(at_the_bound + 1)),
        Err(TooManyCollapsedFolders {
            count: MAX_COLLAPSED_FOLDERS + 1
        })
    );
}

#[test]
fn both_panes_are_never_reorderable_in_groups_view() {
    let (state, _) = organization(SCENE);

    let active = active_groups(&scene_report(), &state, &plain()).expect("loaded");
    let inactive = inactive_of(&scene_report(), &[], &state).expect("loaded");

    assert!(!active.list.reorderable);
    assert!(!inactive.list.reorderable);
    assert_eq!(inactive.list.total, 6);
}

#[test]
fn the_inactive_pane_groups_only_its_own_mods_and_counts_them() {
    let (state, ids) = organization(SCENE);
    let report = ReportBuilder::new()
        .mod_("a")
        .inactive("d")
        .inactive("z")
        .build();

    let list = inactive_of(&report, &["a"], &state).expect("loaded");

    assert_eq!(
        render(&list),
        [
            "1 Armor (1)",
            "2 - d",
            "1 Empty (0)",
            "1 Weapons (0)",
            "2 Melee (0)",
            "3 Blades (0)",
            "1 ~ (1)",
            "2 - z",
        ]
    );
    assert_eq!(ids_in(&list, FolderKey::Group(ids["Armor"])), ["d"]);
}

#[test]
fn without_loaded_organization_the_view_is_unavailable_even_with_no_group_filter() {
    for state in [
        OrganizationState::NotLoaded,
        OrganizationState::Recovered {
            issue: OrganizationIssue::Malformed,
        },
    ] {
        let working_result = active_groups(&scene_report(), &state, &plain());
        let inactive_result = inactive_of(&scene_report(), &[], &state);

        assert!(
            matches!(
                working_result,
                Err(ModListError::OrganizationUnavailable(_))
            ),
            "{state:?}"
        );
        assert!(matches!(
            inactive_result,
            Err(ModListError::OrganizationUnavailable(_))
        ));
    }
}

#[test]
fn a_stale_group_scope_is_unknown_group_not_an_empty_view() {
    let (state, _) = organization(SCENE);
    let stale = GroupId::new(999).expect("non-zero");
    let scoped = ModListQuery {
        group: Some(GroupFilter::InScope(stale)),
        ..ModListQuery::default()
    };

    let result = active_groups(&scene_report(), &state, &query(scoped, &[]));

    assert_eq!(result, Err(ModListError::UnknownGroup(stale)));
}

/// The same organization with its groups listed in reverse document order.
fn rebuilt_in_reverse(state: &OrganizationState) -> OrganizationState {
    let forest = state.loaded().expect("loaded").clone();
    let mut parts: Vec<GroupParts> = forest
        .groups()
        .map(|(id, group)| GroupParts {
            id,
            name: group.name().clone(),
            parent: group.parent(),
            color: group.color(),
            members: group.members().clone(),
        })
        .collect();
    parts.reverse();
    let rebuilt = ModOrganization::from_parts(parts, BTreeMap::new(), forest.next_group_id())
        .expect("same document");
    OrganizationState::Loaded(rebuilt)
}

#[test]
fn two_calls_agree_and_the_document_order_of_the_groups_does_not_matter() {
    let (state, _) = organization(SCENE);

    let first = active_groups(&scene_report(), &state, &plain()).expect("loaded");
    let second = active_groups(&scene_report(), &state, &plain()).expect("loaded");
    let from_reordered =
        active_groups(&scene_report(), &rebuilt_in_reverse(&state), &plain()).expect("loaded");

    assert_eq!(first, second);
    assert_eq!(first, from_reordered);
}

#[test]
fn siblings_keep_display_order_not_id_or_ascii_order() {
    // "alpha" has the larger id and sorts after "Zeta" by ASCII, but
    // display order lowercases first: alpha before Zeta.
    let (state, _) = organization(&[("Zeta", None, &["a"]), ("alpha", None, &["b"])]);

    let list = active_groups(&scene_report(), &state, &plain()).expect("loaded");

    assert_eq!(
        render(&list)[..4],
        ["1 alpha (1)", "2 - b", "1 Zeta (1)", "2 - a"]
    );
}

#[test]
fn cousins_whose_names_differ_only_in_case_follow_their_parents_in_tree_order() {
    // Creation order is the reverse of display order at the top level (Beta
    // has id 1, Alpha id 2), and the cousins "x" and "X" are different groups
    // of different parents: only the tree's own order can place them.
    let (state, _) = organization(&[
        ("Beta", None, &[]),
        ("Alpha", None, &[]),
        ("x", Some("Beta"), &["a"]),
        ("X", Some("Alpha"), &["b"]),
    ]);
    let tree_order: Vec<GroupId> = state
        .loaded()
        .expect("loaded")
        .tree()
        .iter()
        .map(|node| node.id)
        .collect();

    let list = active_groups(&scene_report(), &state, &plain()).expect("loaded");
    let from_reordered =
        active_groups(&scene_report(), &rebuilt_in_reverse(&state), &plain()).expect("loaded");

    assert_eq!(
        render(&list)[..6],
        [
            "1 Alpha (1)",
            "2 X (1)",
            "3 - b",
            "1 Beta (1)",
            "2 x (1)",
            "3 - a"
        ]
    );
    let header_order: Vec<GroupId> = list
        .entries
        .iter()
        .filter_map(|entry| match entry {
            GroupedEntry::Folder {
                folder: Folder::Group { id, .. },
                ..
            } => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(header_order, tree_order);
    assert_eq!(list, from_reordered);
}

/// `count` inactive mods named `m00000`..
fn many_inactive(count: usize) -> (Report, Vec<String>) {
    let names: Vec<String> = (0..count).map(|i| format!("m{i:05}")).collect();
    let mut builder = ReportBuilder::new();
    for name in &names {
        builder = builder.inactive(name);
    }
    (builder.build(), names)
}

#[test]
fn entries_are_cut_at_the_bound_and_the_total_counts_past_it() {
    // Three groups holding the same 4,000 mods: 3 headers + 12,000 copies.
    let (report, names) = many_inactive(4_000);
    let members: Vec<&str> = names.iter().map(String::as_str).collect();
    let (state, _) = organization(&[
        ("A", None, &members),
        ("B", None, &members),
        ("C", None, &members),
    ]);

    let list = inactive_of(&report, &[], &state).expect("loaded");

    assert_eq!(list.total_entries, 12_003);
    assert_eq!(list.entries.len(), MAX_GROUPED_ENTRIES);
    // A cut never splits a header from its count: every emitted header
    // carries the full distinct count, not what was emitted.
    let counts: Vec<usize> = list
        .entries
        .iter()
        .filter_map(|entry| match entry {
            GroupedEntry::Folder { matched, .. } => Some(*matched),
            GroupedEntry::Mod { .. } => None,
        })
        .collect();
    assert_eq!(counts, [4_000, 4_000, 4_000]);
}

#[test]
fn rows_past_the_list_cut_are_counted_but_never_emitted() {
    // A group holds at most 5,000 members, so a second group takes the
    // 7 rows that sort past the list's own cut.
    let (report, names) = many_inactive(MAX_MOD_LIST_ROWS + 7);
    let (first, rest) = names.split_at(MAX_MOD_LIST_ROWS);
    let first: Vec<&str> = first.iter().map(String::as_str).collect();
    let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
    let (state, _) = organization(&[("First", None, &first), ("Rest", None, &rest)]);

    let list = inactive_of(&report, &[], &state).expect("loaded");

    assert_eq!(list.list.total, MAX_MOD_LIST_ROWS + 7);
    assert_eq!(list.list.items.len(), MAX_MOD_LIST_ROWS);
    // Two headers and every row are counted; the 7 past the cut are not
    // emitted, though their folder still counts them.
    assert_eq!(list.total_entries, MAX_MOD_LIST_ROWS + 9);
    assert_eq!(list.entries.len(), MAX_MOD_LIST_ROWS + 2);
    assert!(matches!(
        list.entries.last(),
        Some(GroupedEntry::Folder { matched: 7, .. })
    ));
    let item_ids: BTreeSet<&ModId> = list.list.items.iter().map(|row| &row.mod_id).collect();
    assert!(
        mod_entries(&list)
            .iter()
            .all(|(_, id)| item_ids.contains(id))
    );
}

#[test]
fn the_ungrouped_folder_counts_rows_past_the_list_cut_though_it_never_emits_them() {
    // One empty group keeps the organization loaded; every mod is ungrouped.
    let (report, _) = many_inactive(MAX_MOD_LIST_ROWS + 7);
    let (state, _) = organization(&[("Empty", None, &[])]);

    let list = inactive_of(&report, &[], &state).expect("loaded");

    assert!(matches!(
        mod_entries(&list).last(),
        Some((FolderKey::Ungrouped, _))
    ));
    assert_eq!(list.entries.len(), MAX_MOD_LIST_ROWS + 2);
    let counted = list.entries.iter().find_map(|entry| match entry {
        GroupedEntry::Folder {
            folder: Folder::Ungrouped,
            matched,
            ..
        } => Some(*matched),
        _ => None,
    });
    assert_eq!(counted, Some(MAX_MOD_LIST_ROWS + 7));
    assert_eq!(list.total_entries, MAX_MOD_LIST_ROWS + 9);
}

#[test]
fn a_group_scope_missing_from_the_organization_is_unknown_group_at_the_layout_too() {
    let (state, _) = organization(SCENE);
    let forest = state.loaded().expect("loaded");
    let stale = GroupId::new(999).expect("non-zero");
    let scoped = ModListQuery {
        group: Some(GroupFilter::InScope(stale)),
        ..ModListQuery::default()
    };

    let result = build(forest, Vec::new(), &query(scoped, &[]));

    assert_eq!(result, Err(ModListError::UnknownGroup(stale)));
}

mod properties {
    use proptest::prelude::*;

    use super::*;
    use crate::organization::GroupNode;

    const MODS: [&str; 6] = ["m0", "m1", "m2", "m3", "m4", "m5"];

    type Shape = Vec<(Option<usize>, Vec<usize>)>;

    /// A random forest of up to 7 groups (a parent is always an earlier
    /// group, so no cycle and the depth stays within the limit) with
    /// random direct members.
    fn forest() -> impl Strategy<Value = Shape> {
        proptest::collection::vec(
            (
                proptest::option::of(0..7_usize),
                proptest::collection::vec(0..MODS.len(), 0..5),
            ),
            0..8,
        )
    }

    fn group_id(index: usize) -> GroupId {
        GroupId::new(u32::try_from(index + 1).expect("small")).expect("non-zero")
    }

    fn build(shape: &Shape) -> OrganizationState {
        let parts = shape
            .iter()
            .enumerate()
            .map(|(index, (parent, members))| GroupParts {
                id: group_id(index),
                name: GroupName::new(format!("g{index}")).expect("valid"),
                parent: parent.filter(|parent| *parent < index).map(group_id),
                color: None,
                members: members.iter().map(|i| member(MODS[*i])).collect(),
            })
            .collect();
        let next = u32::try_from(shape.len() + 1).expect("small");
        OrganizationState::Loaded(
            ModOrganization::from_parts(parts, BTreeMap::new(), next).expect("valid forest"),
        )
    }

    fn grouped_of(state: &OrganizationState) -> GroupedModList {
        let mut builder = ReportBuilder::new();
        for id in MODS {
            builder = builder.mod_(id);
        }
        working_groups(
            &builder.build(),
            &Tagging::default(),
            &working(&MODS),
            &ModUpdateTimes::default(),
            &FileMembership::default(),
            state,
            &plain(),
        )
        .expect("loaded")
    }

    proptest! {
        #[test]
        fn every_listed_mod_appears_in_each_folder_it_belongs_to_and_no_other(shape in forest()) {
            let list = grouped_of(&build(&shape));

            for row in &list.list.items {
                let copies: Vec<FolderKey> = mod_entries(&list)
                    .into_iter()
                    .filter(|(_, id)| **id == row.mod_id)
                    .map(|(folder, _)| folder)
                    .collect();
                let expected: Vec<FolderKey> = if row.groups.is_empty() {
                    vec![FolderKey::Ungrouped]
                } else {
                    row.groups.iter().map(|label| FolderKey::Group(label.id)).collect()
                };
                prop_assert!(!copies.is_empty(), "{} has no entry", row.mod_id);
                prop_assert!(
                    copies.iter().all(|copy| expected.contains(copy)),
                    "{} sits in a folder it is not in",
                    row.mod_id
                );
                prop_assert_eq!(copies.len(), expected.len());
            }
        }

        #[test]
        fn headers_count_their_scope_once_and_sit_one_level_below_their_parent(shape in forest()) {
            let state = build(&shape);
            let list = grouped_of(&state);
            let organization = state.loaded().expect("loaded");
            let nodes: BTreeMap<GroupId, GroupNode> = organization
                .tree()
                .into_iter()
                .map(|node| (node.id, node))
                .collect();
            // The headers above the current one, one per level.
            let mut open: Vec<GroupId> = Vec::new();
            let mut header_depths: BTreeMap<GroupId, usize> = BTreeMap::new();

            for entry in &list.entries {
                let GroupedEntry::Folder {
                    folder: Folder::Group { id, .. },
                    depth,
                    matched,
                    ..
                } = entry
                else {
                    continue;
                };
                let scope = organization.scope(*id).expect("the group exists");
                let in_scope = list
                    .list
                    .items
                    .iter()
                    .filter(|row| scope.mods.contains(&member(row.mod_id.as_str())))
                    .count();
                prop_assert_eq!(*matched, in_scope, "matched of {:?}", id);

                let parent = nodes[id].parent;
                open.truncate(*depth - 1);
                prop_assert_eq!(open.last().copied(), parent, "the header above {:?}", id);
                prop_assert_eq!(*depth, open.len() + 1);
                if let Some(parent) = parent {
                    prop_assert_eq!(*depth, header_depths[&parent] + 1);
                }
                header_depths.insert(*id, *depth);
                open.push(*id);
            }
        }
    }
}
