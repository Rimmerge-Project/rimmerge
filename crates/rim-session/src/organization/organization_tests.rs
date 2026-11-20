//! The organization domain: every invariant of the forest, the value
//! objects, and the scope queries. Pure; no IO.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;

use super::*;
use crate::test_support::organized;

fn name(text: &str) -> GroupName {
    GroupName::new(text).expect("fixture name is valid")
}

fn mods(ids: &[&str]) -> BTreeSet<OrganizedModId> {
    ids.iter().map(|id| organized(id)).collect()
}

/// A top-level group.
fn root(organization: &mut ModOrganization, text: &str) -> GroupId {
    organization
        .create_group(name(text), None)
        .expect("a fresh top-level group")
}

fn child(organization: &mut ModOrganization, parent: GroupId, text: &str) -> GroupId {
    organization
        .create_group(name(text), Some(parent))
        .expect("a fresh subgroup")
}

/// A chain `g1 > g2 > ... > g<levels>` of single-child groups.
fn chain(organization: &mut ModOrganization, levels: usize) -> Vec<GroupId> {
    let mut ids = vec![root(organization, "level 1")];
    for level in 2..=levels {
        let parent = *ids.last().expect("the chain starts with a root");
        ids.push(child(organization, parent, &format!("level {level}")));
    }
    ids
}

fn parts_of(organization: &ModOrganization) -> Vec<GroupParts> {
    organization
        .groups()
        .map(|(id, group)| GroupParts {
            id,
            name: group.name().clone(),
            parent: group.parent(),
            color: group.color(),
            members: group.members().clone(),
        })
        .collect()
}

/// A next id far beyond any fixture group id.
const FAR_NEXT_ID: u32 = 5_000;

fn part(id: u32, text: &str, parent: Option<u32>) -> GroupParts {
    GroupParts {
        id: GroupId::new(id).expect("non-zero fixture id"),
        name: name(text),
        parent: parent.and_then(GroupId::new),
        color: None,
        members: BTreeSet::new(),
    }
}

// ---- value objects -----------------------------------------------------------

#[test]
fn group_names_are_trimmed_and_keep_their_unicode() {
    assert_eq!(name("  Waffen \u{00DF}  ").as_str(), "Waffen \u{00DF}");
}

#[test]
fn group_names_reject_empty_overlong_and_control_characters() {
    assert_eq!(GroupName::new("   "), Err(InvalidGroupName::Empty));
    assert_eq!(
        GroupName::new("x".repeat(OrganizationLimits::MAX_NAME_CHARS + 1)),
        Err(InvalidGroupName::TooLong {
            limit: OrganizationLimits::MAX_NAME_CHARS
        })
    );
    assert_eq!(
        GroupName::new("two\nlines"),
        Err(InvalidGroupName::ControlCharacter)
    );
    assert!(GroupName::new("x".repeat(OrganizationLimits::MAX_NAME_CHARS)).is_ok());
}

#[test]
fn a_group_name_limit_counts_characters_not_bytes() {
    let sixty_four_wide_characters = "\u{4E16}".repeat(OrganizationLimits::MAX_NAME_CHARS);

    assert!(GroupName::new(sixty_four_wide_characters).is_ok());
}

#[test]
fn group_id_zero_is_not_an_id() {
    assert_eq!(GroupId::new(0), None);
    assert_eq!(GroupId::new(7).map(GroupId::get), Some(7));
}

#[test]
fn organized_mod_ids_are_base_ids() {
    let workshop = OrganizedModId::new(&ModId::new("Some.Mod_steam")).expect("valid");
    let local = OrganizedModId::new(&ModId::new("some.mod")).expect("valid");

    assert_eq!(workshop, local);
    assert_eq!(workshop.as_str(), "some.mod");
}

#[test]
fn organized_mod_ids_reject_what_a_file_could_not_hold() {
    let overlong = ModId::new("a".repeat(OrganizationLimits::MAX_ID_CHARS + 1));

    assert_eq!(
        OrganizedModId::new(&ModId::new("_steam")),
        Err(InvalidOrganizedModId::Empty)
    );
    assert!(matches!(
        OrganizedModId::new(&overlong),
        Err(InvalidOrganizedModId::TooLong { .. })
    ));
    assert_eq!(
        OrganizedModId::new(&ModId::new("a\u{0007}b")),
        Err(InvalidOrganizedModId::ControlCharacter)
    );
}

#[test]
fn color_tokens_round_trip_through_their_wire_names() {
    for token in ColorToken::ALL {
        assert_eq!(token.as_str().parse::<ColorToken>(), Ok(token));
    }
    assert_eq!(ColorToken::ALL.len(), 12);
}

#[test]
fn color_parsing_is_exact_and_rejects_hex_and_unknown_names() {
    assert!(
        "Teal".parse::<ColorToken>().is_err(),
        "wire names are lowercase"
    );
    assert!("#ff0000".parse::<ColorToken>().is_err());
    assert!("".parse::<ColorToken>().is_err());
}

// ---- creating, renaming ------------------------------------------------------

#[test]
fn create_group_rejects_a_duplicate_sibling_name_ignoring_case() {
    let mut organization = ModOrganization::new();
    root(&mut organization, "Weapons");
    let before = organization.clone();

    let result = organization.create_group(name("weapons"), None);

    assert!(matches!(result, Err(OrganizationError::NameTaken { .. })));
    assert_eq!(organization, before);
}

#[test]
fn the_same_name_is_allowed_under_different_parents() {
    let mut organization = ModOrganization::new();
    let one = root(&mut organization, "One");
    let two = root(&mut organization, "Two");

    child(&mut organization, one, "Misc");
    let second = organization.create_group(name("Misc"), Some(two));

    assert!(second.is_ok());
}

#[test]
fn create_group_under_a_missing_parent_is_refused() {
    let mut organization = ModOrganization::new();
    let missing = GroupId::new(99).expect("non-zero");

    let result = organization.create_group(name("Orphan"), Some(missing));

    assert_eq!(result, Err(OrganizationError::GroupNotFound(missing)));
    assert_eq!(organization.group_count(), 0);
}

#[test]
fn group_ids_are_not_reused_after_a_delete() {
    let mut organization = ModOrganization::new();
    let first = root(&mut organization, "First");
    organization
        .delete_group(first, DeleteMode::WithSubgroups)
        .expect("delete");

    let second = root(&mut organization, "Second");

    assert_ne!(first, second);
}

#[test]
fn rename_group_rejects_a_sibling_name_but_allows_a_case_change_of_itself() {
    let mut organization = ModOrganization::new();
    let weapons = root(&mut organization, "Weapons");
    root(&mut organization, "Armor");

    let clash = organization.rename_group(weapons, name("ARMOR"));
    let own_case = organization.rename_group(weapons, name("WEAPONS"));

    assert!(matches!(clash, Err(OrganizationError::NameTaken { .. })));
    assert!(own_case.is_ok());
    assert_eq!(
        organization.group(weapons).map(|g| g.name().as_str()),
        Some("WEAPONS")
    );
}

#[test]
fn at_most_max_groups_can_exist() {
    let mut organization = ModOrganization::new();
    for index in 0..OrganizationLimits::MAX_GROUPS {
        root(&mut organization, &format!("group {index}"));
    }

    let result = organization.create_group(name("one too many"), None);

    assert_eq!(
        result,
        Err(OrganizationError::TooManyGroups {
            limit: OrganizationLimits::MAX_GROUPS
        })
    );
}

// ---- depth and cycles --------------------------------------------------------

#[test]
fn creating_below_the_maximum_depth_is_refused() {
    let mut organization = ModOrganization::new();
    let ids = chain(&mut organization, OrganizationLimits::MAX_DEPTH);
    let deepest = *ids.last().expect("a chain");

    let result = organization.create_group(name("too deep"), Some(deepest));

    assert_eq!(
        result,
        Err(OrganizationError::TooDeep {
            limit: OrganizationLimits::MAX_DEPTH
        })
    );
}

#[test]
fn reparent_rejects_itself() {
    let mut organization = ModOrganization::new();
    let only = root(&mut organization, "Only");

    let result = organization.reparent_group(only, Some(only));

    assert_eq!(result, Err(OrganizationError::WouldCreateCycle));
}

#[test]
fn reparent_rejects_a_descendant_two_levels_down() {
    let mut organization = ModOrganization::new();
    let ids = chain(&mut organization, 2);
    let before = organization.clone();

    let result = organization.reparent_group(ids[0], Some(ids[1]));

    assert_eq!(result, Err(OrganizationError::WouldCreateCycle));
    assert_eq!(organization, before);
}

#[test]
fn reparent_rejects_a_descendant_five_levels_down() {
    let mut organization = ModOrganization::new();
    let ids = chain(&mut organization, 5);
    let before = organization.clone();

    let result = organization.reparent_group(ids[0], Some(ids[4]));

    assert_eq!(result, Err(OrganizationError::WouldCreateCycle));
    assert_eq!(organization, before);
}

#[test]
fn reparent_rejects_exceeding_the_maximum_depth_counting_the_whole_subtree() {
    let mut organization = ModOrganization::new();
    let levels = chain(&mut organization, OrganizationLimits::MAX_DEPTH - 1);
    let moved = root(&mut organization, "moved");
    child(&mut organization, moved, "moved child");
    let level_seven = levels[OrganizationLimits::MAX_DEPTH - 2];
    let level_six = levels[OrganizationLimits::MAX_DEPTH - 3];
    let before = organization.clone();

    let too_deep = organization.reparent_group(moved, Some(level_seven));

    assert!(matches!(too_deep, Err(OrganizationError::TooDeep { .. })));
    assert_eq!(organization, before, "a refused move changes nothing");
    assert!(
        organization.reparent_group(moved, Some(level_six)).is_ok(),
        "a 2-level subtree under level 6 ends exactly at level 8"
    );
}

#[test]
fn reparent_to_the_current_parent_changes_nothing() {
    let mut organization = ModOrganization::new();
    let parent = root(&mut organization, "Parent");
    let kid = child(&mut organization, parent, "Kid");
    let before = organization.clone();

    organization
        .reparent_group(kid, Some(parent))
        .expect("a no-op move is accepted");

    assert_eq!(organization, before);
}

#[test]
fn reparent_refuses_a_sibling_name_clash_at_the_target() {
    let mut organization = ModOrganization::new();
    let a = root(&mut organization, "A");
    let b = root(&mut organization, "B");
    child(&mut organization, a, "Same");
    let other = child(&mut organization, b, "same");
    let before = organization.clone();

    let result = organization.reparent_group(other, Some(a));

    assert!(matches!(result, Err(OrganizationError::NameTaken { .. })));
    assert_eq!(organization, before);
}

#[test]
fn reparent_to_the_top_level_works_and_keeps_the_subtree() {
    let mut organization = ModOrganization::new();
    let ids = chain(&mut organization, 3);

    organization
        .reparent_group(ids[1], None)
        .expect("top level is always acyclic");

    assert_eq!(organization.group(ids[1]).and_then(Group::parent), None);
    assert_eq!(
        organization.group(ids[2]).and_then(Group::parent),
        Some(ids[1])
    );
}

#[test]
fn valid_parents_excludes_self_descendants_too_deep_targets_and_name_clashes() {
    let mut organization = ModOrganization::new();
    let a = root(&mut organization, "A");
    let a_kid = child(&mut organization, a, "Kid");
    let b = root(&mut organization, "B");
    let b_clash = child(&mut organization, b, "A");
    let c = root(&mut organization, "C");

    let for_a = organization.valid_parents(a).expect("a exists");
    let for_kid = organization.valid_parents(a_kid).expect("kid exists");

    // `A` cannot go inside its own kid or itself, nor under `B` (which
    // already holds an `A`); top level and `C` are fine.
    assert!(for_a.contains(&None));
    assert!(for_a.contains(&Some(c)));
    assert!(
        for_a.contains(&Some(b_clash)),
        "a deeper target without a clash"
    );
    assert!(!for_a.contains(&Some(a)));
    assert!(!for_a.contains(&Some(a_kid)));
    assert!(for_kid.contains(&Some(a)), "the current parent stays valid");
    assert!(!for_kid.contains(&Some(a_kid)));
}

#[test]
fn valid_parents_excludes_a_target_that_would_exceed_the_depth() {
    let mut organization = ModOrganization::new();
    let ids = chain(&mut organization, OrganizationLimits::MAX_DEPTH);
    let loose = root(&mut organization, "loose");

    let parents = organization.valid_parents(loose).expect("loose exists");

    assert!(
        !parents.contains(&ids.last().copied()),
        "level 9 is not allowed"
    );
    assert!(parents.contains(&Some(ids[OrganizationLimits::MAX_DEPTH - 2])));
}

// ---- membership --------------------------------------------------------------

#[test]
fn add_members_is_idempotent_and_reports_already() {
    let mut organization = ModOrganization::new();
    let group = root(&mut organization, "G");

    let first = organization
        .add_members(group, &mods(&["a", "b"]))
        .expect("add");
    let again = organization
        .add_members(group, &mods(&["b", "c"]))
        .expect("add");

    assert_eq!(
        first,
        MembershipAdd {
            added: 2,
            already: 0
        }
    );
    assert_eq!(
        again,
        MembershipAdd {
            added: 1,
            already: 1
        }
    );
    assert_eq!(
        organization.group(group).map(|g| g.members().len()),
        Some(3)
    );
}

#[test]
fn members_are_keyed_by_base_id() {
    let mut organization = ModOrganization::new();
    let group = root(&mut organization, "G");

    organization
        .add_members(group, &mods(&["x", "x_steam"]))
        .expect("both spellings are one mod");

    assert_eq!(organization.membership_count(), 1);
    assert_eq!(
        organization.groups_of(&organized("x_steam")),
        BTreeSet::from([group])
    );
}

#[test]
fn add_members_to_a_missing_group_is_refused() {
    let mut organization = ModOrganization::new();
    let missing = GroupId::new(5).expect("non-zero");

    let result = organization.add_members(missing, &mods(&["a"]));

    assert_eq!(result, Err(OrganizationError::GroupNotFound(missing)));
}

#[test]
fn add_members_refuses_to_exceed_the_per_group_limit_without_changing_anything() {
    let mut organization = ModOrganization::new();
    let group = root(&mut organization, "G");
    let full: BTreeSet<OrganizedModId> = (0..OrganizationLimits::MAX_MEMBERS_PER_GROUP)
        .map(|index| organized(&format!("mod.{index}")))
        .collect();
    organization
        .add_members(group, &full)
        .expect("exactly full");
    let before = organization.clone();

    let result = organization.add_members(group, &mods(&["one.more"]));

    assert!(matches!(
        result,
        Err(OrganizationError::TooManyMembers { .. })
    ));
    assert_eq!(organization, before);
}

#[test]
fn remove_members_reports_absent_and_keeps_other_groups() {
    let mut organization = ModOrganization::new();
    let one = root(&mut organization, "One");
    let two = root(&mut organization, "Two");
    organization
        .add_members(one, &mods(&["a", "b"]))
        .expect("add");
    organization.add_members(two, &mods(&["a"])).expect("add");

    let outcome = organization
        .remove_members(one, &mods(&["a", "zzz"]))
        .expect("remove");

    assert_eq!(
        outcome,
        MembershipRemoval {
            removed: 1,
            absent: 1
        }
    );
    assert_eq!(
        organization.groups_of(&organized("a")),
        BTreeSet::from([two])
    );
}

#[test]
fn move_members_refuses_the_same_group() {
    let mut organization = ModOrganization::new();
    let group = root(&mut organization, "G");

    let result = organization.move_members(group, group, &mods(&["a"]));

    assert_eq!(result, Err(OrganizationError::SameGroup));
}

#[test]
fn move_members_moves_only_direct_members_of_source_and_keeps_other_memberships() {
    let mut organization = ModOrganization::new();
    let source = root(&mut organization, "Source");
    let target = root(&mut organization, "Target");
    let other = root(&mut organization, "Other");
    organization
        .add_members(source, &mods(&["a", "b"]))
        .expect("add");
    organization.add_members(other, &mods(&["a"])).expect("add");
    organization
        .add_members(target, &mods(&["b"]))
        .expect("add");

    let outcome = organization
        .move_members(source, target, &mods(&["a", "b", "stranger"]))
        .expect("move");

    assert_eq!(
        outcome,
        MembershipMove {
            moved: 1,
            already_in_target: 1,
            not_in_source: 1
        }
    );
    assert!(organization.groups_of(&organized("stranger")).is_empty());
    assert_eq!(
        organization.groups_of(&organized("a")),
        BTreeSet::from([target, other]),
        "a left the source, joined the target, and kept its membership in Other"
    );
    assert_eq!(
        organization.groups_of(&organized("b")),
        BTreeSet::from([target])
    );
}

#[test]
fn a_failed_move_changes_nothing() {
    let mut organization = ModOrganization::new();
    let source = root(&mut organization, "Source");
    let target = root(&mut organization, "Target");
    organization
        .add_members(source, &mods(&["a"]))
        .expect("add");
    let full: BTreeSet<OrganizedModId> = (0..OrganizationLimits::MAX_MEMBERS_PER_GROUP)
        .map(|index| organized(&format!("filler.{index}")))
        .collect();
    organization.add_members(target, &full).expect("full");
    let before = organization.clone();

    let result = organization.move_members(source, target, &mods(&["a"]));

    assert!(matches!(
        result,
        Err(OrganizationError::TooManyMembers { .. })
    ));
    assert_eq!(organization, before);
}

// ---- scope and tree ----------------------------------------------------------

#[test]
fn scope_includes_descendants_deduplicated() {
    let mut organization = ModOrganization::new();
    let parent = root(&mut organization, "Parent");
    let kid = child(&mut organization, parent, "Kid");
    organization
        .add_members(parent, &mods(&["a", "b"]))
        .expect("add");
    organization
        .add_members(kid, &mods(&["b", "c"]))
        .expect("add");

    let scope = organization.scope(parent).expect("scope");

    assert_eq!(scope.groups, BTreeSet::from([parent, kid]));
    assert_eq!(scope.mods, mods(&["a", "b", "c"]));
    let node = organization
        .tree()
        .into_iter()
        .find(|node| node.id == parent)
        .expect("parent is in the tree");
    assert_eq!((node.direct_count, node.scope_count), (2, 3));
}

#[test]
fn scope_creates_no_ancestor_membership() {
    let mut organization = ModOrganization::new();
    let parent = root(&mut organization, "Parent");
    let kid = child(&mut organization, parent, "Kid");
    organization.add_members(kid, &mods(&["a"])).expect("add");

    let _ = organization.scope(parent).expect("scope");

    assert_eq!(
        organization.groups_of(&organized("a")),
        BTreeSet::from([kid])
    );
    assert_eq!(
        organization.group(parent).map(|g| g.members().len()),
        Some(0)
    );
}

#[test]
fn scope_of_a_missing_group_is_an_error() {
    let organization = ModOrganization::new();
    let missing = GroupId::new(3).expect("non-zero");

    assert_eq!(
        organization.scope(missing),
        Err(OrganizationError::GroupNotFound(missing))
    );
}

#[test]
fn the_tree_is_depth_first_with_siblings_by_name_then_id() {
    let mut organization = ModOrganization::new();
    let zeta = root(&mut organization, "zeta");
    let alpha = root(&mut organization, "Alpha");
    let beta = child(&mut organization, alpha, "beta");
    let ant = child(&mut organization, alpha, "Ant");

    let tree = organization.tree();

    assert_eq!(
        tree.iter().map(|node| node.id).collect::<Vec<_>>(),
        [alpha, ant, beta, zeta]
    );
    assert_eq!(
        tree.iter().map(|node| node.depth).collect::<Vec<_>>(),
        [1, 2, 2, 1]
    );
    assert_eq!(tree[0].child_count, 2);
}

// ---- deleting ----------------------------------------------------------------

#[test]
fn delete_lift_children_keeps_children_and_their_members() {
    let mut organization = ModOrganization::new();
    let top = root(&mut organization, "Top");
    let middle = child(&mut organization, top, "Middle");
    let leaf = child(&mut organization, middle, "Leaf");
    organization.add_members(leaf, &mods(&["a"])).expect("add");
    organization
        .add_members(middle, &mods(&["m"]))
        .expect("add");

    let outcome = organization
        .delete_group(middle, DeleteMode::LiftChildren)
        .expect("delete");

    assert_eq!(
        outcome,
        DeleteOutcome {
            groups_removed: 1,
            children_lifted: 1
        }
    );
    assert_eq!(organization.group(leaf).and_then(Group::parent), Some(top));
    assert_eq!(
        organization.groups_of(&organized("a")),
        BTreeSet::from([leaf])
    );
    assert!(organization.groups_of(&organized("m")).is_empty());
}

#[test]
fn delete_lift_children_refuses_a_name_clash_without_mutating() {
    let mut organization = ModOrganization::new();
    let top = root(&mut organization, "Top");
    let middle = child(&mut organization, top, "Middle");
    child(&mut organization, middle, "Twin");
    child(&mut organization, top, "twin");
    let before = organization.clone();

    let result = organization.delete_group(middle, DeleteMode::LiftChildren);

    assert!(matches!(result, Err(OrganizationError::NameTaken { .. })));
    assert_eq!(organization, before);
}

#[test]
fn deleting_a_group_may_lift_a_child_that_shares_the_deleted_groups_own_name() {
    let mut organization = ModOrganization::new();
    let top = root(&mut organization, "Top");
    let middle = child(&mut organization, top, "Same");
    let leaf = child(&mut organization, middle, "same");

    organization
        .delete_group(middle, DeleteMode::LiftChildren)
        .expect("the deleted group's own name does not clash");

    assert_eq!(organization.group(leaf).and_then(Group::parent), Some(top));
}

#[test]
fn delete_with_subgroups_removes_only_groups() {
    let mut organization = ModOrganization::new();
    let top = root(&mut organization, "Top");
    let kid = child(&mut organization, top, "Kid");
    let other = root(&mut organization, "Other");
    organization.add_members(kid, &mods(&["a"])).expect("add");
    organization.add_members(other, &mods(&["a"])).expect("add");
    organization
        .set_mod_colors(&mods(&["a"]), Some(ColorToken::Teal))
        .expect("colour");

    let outcome = organization
        .delete_group(top, DeleteMode::WithSubgroups)
        .expect("delete");

    assert_eq!(
        outcome,
        DeleteOutcome {
            groups_removed: 2,
            children_lifted: 0
        }
    );
    assert_eq!(organization.group_count(), 1);
    assert_eq!(
        organization.groups_of(&organized("a")),
        BTreeSet::from([other])
    );
    assert_eq!(
        organization.color_of(&organized("a")),
        Some(ColorToken::Teal)
    );
}

#[test]
fn deleting_a_missing_group_is_refused() {
    let mut organization = ModOrganization::new();
    let missing = GroupId::new(8).expect("non-zero");

    assert_eq!(
        organization.delete_group(missing, DeleteMode::LiftChildren),
        Err(OrganizationError::GroupNotFound(missing))
    );
}

// ---- colours -----------------------------------------------------------------

#[test]
fn set_mod_colors_sets_reports_and_resets() {
    let mut organization = ModOrganization::new();

    let first = organization
        .set_mod_colors(&mods(&["a", "b"]), Some(ColorToken::Red))
        .expect("set");
    let again = organization
        .set_mod_colors(&mods(&["a", "c"]), Some(ColorToken::Red))
        .expect("set");
    let reset = organization
        .set_mod_colors(&mods(&["a", "never.coloured"]), None)
        .expect("reset");

    assert_eq!(
        first,
        ColorChange {
            changed: 2,
            unchanged: 0
        }
    );
    assert_eq!(
        again,
        ColorChange {
            changed: 1,
            unchanged: 1
        }
    );
    assert_eq!(
        reset,
        ColorChange {
            changed: 1,
            unchanged: 1
        }
    );
    assert_eq!(organization.color_of(&organized("a")), None);
    assert_eq!(
        organization.color_of(&organized("b")),
        Some(ColorToken::Red)
    );
}

#[test]
fn set_mod_colors_reset_removes_entries() {
    let mut organization = ModOrganization::new();
    organization
        .set_mod_colors(&mods(&["a"]), Some(ColorToken::Pink))
        .expect("set");

    organization
        .set_mod_colors(&mods(&["a"]), None)
        .expect("reset");

    assert_eq!(organization.mod_color_count(), 0);
    assert!(organization.is_empty(), "no stale entry is left behind");
}

#[test]
fn a_mod_never_inherits_a_group_colour() {
    let mut organization = ModOrganization::new();
    let group = root(&mut organization, "G");
    organization
        .set_group_color(group, Some(ColorToken::Green))
        .expect("colour");
    organization.add_members(group, &mods(&["a"])).expect("add");

    assert_eq!(organization.color_of(&organized("a")), None);
}

#[test]
fn colouring_is_bounded() {
    let mut organization = ModOrganization::new();
    let many: BTreeSet<OrganizedModId> = (0..=OrganizationLimits::MAX_MOD_COLORS)
        .map(|index| organized(&format!("c.{index}")))
        .collect();

    let result = organization.set_mod_colors(&many, Some(ColorToken::Blue));

    assert!(matches!(
        result,
        Err(OrganizationError::TooManyColors { .. })
    ));
    assert!(organization.is_empty());
}

// ---- loading a document ------------------------------------------------------

#[test]
fn from_parts_round_trips_a_built_organization() {
    let mut organization = ModOrganization::new();
    let top = root(&mut organization, "Top");
    let kid = child(&mut organization, top, "Kid");
    organization.add_members(kid, &mods(&["a"])).expect("add");
    organization
        .set_group_color(top, Some(ColorToken::Indigo))
        .expect("colour");
    organization
        .set_mod_colors(&mods(&["a"]), Some(ColorToken::Slate))
        .expect("colour");
    let colors: BTreeMap<_, _> = organization
        .mod_colors()
        .map(|(id, color)| (id.clone(), color))
        .collect();

    let rebuilt = ModOrganization::from_parts(
        parts_of(&organization),
        colors,
        organization.next_group_id(),
    )
    .expect("valid");

    assert_eq!(rebuilt.tree(), organization.tree());
    assert_eq!(
        rebuilt.groups_of(&organized("a")),
        organization.groups_of(&organized("a"))
    );
    assert_eq!(rebuilt.color_of(&organized("a")), Some(ColorToken::Slate));
}

#[test]
fn from_parts_rejects_a_cycle() {
    let parts = vec![part(1, "A", Some(2)), part(2, "B", Some(1))];

    let result = ModOrganization::from_parts(parts, BTreeMap::new(), FAR_NEXT_ID);

    assert_eq!(
        result,
        Err(OrganizationDocumentError::Rule(
            OrganizationError::WouldCreateCycle
        ))
    );
}

#[test]
fn from_parts_rejects_a_group_that_is_its_own_parent() {
    let result =
        ModOrganization::from_parts(vec![part(1, "A", Some(1))], BTreeMap::new(), FAR_NEXT_ID);

    assert_eq!(
        result,
        Err(OrganizationDocumentError::Rule(
            OrganizationError::WouldCreateCycle
        ))
    );
}

#[test]
fn from_parts_rejects_a_dangling_parent() {
    let result =
        ModOrganization::from_parts(vec![part(1, "A", Some(9))], BTreeMap::new(), FAR_NEXT_ID);

    assert!(matches!(
        result,
        Err(OrganizationDocumentError::DanglingParent { .. })
    ));
}

#[test]
fn from_parts_rejects_a_duplicate_group_id() {
    let result = ModOrganization::from_parts(
        vec![part(1, "A", None), part(1, "B", None)],
        BTreeMap::new(),
        FAR_NEXT_ID,
    );

    assert!(matches!(
        result,
        Err(OrganizationDocumentError::DuplicateGroupId(_))
    ));
}

#[test]
fn from_parts_rejects_a_duplicate_sibling_name() {
    let result = ModOrganization::from_parts(
        vec![part(1, "A", None), part(2, "a", None)],
        BTreeMap::new(),
        FAR_NEXT_ID,
    );

    assert!(matches!(
        result,
        Err(OrganizationDocumentError::Rule(
            OrganizationError::NameTaken { .. }
        ))
    ));
}

#[test]
fn from_parts_rejects_a_chain_deeper_than_the_limit() {
    let parts: Vec<GroupParts> = (1..=u32::try_from(OrganizationLimits::MAX_DEPTH + 1)
        .expect("small"))
        .map(|id| part(id, &format!("g{id}"), id.checked_sub(1).filter(|p| *p > 0)))
        .collect();

    let result = ModOrganization::from_parts(parts, BTreeMap::new(), FAR_NEXT_ID);

    assert!(matches!(
        result,
        Err(OrganizationDocumentError::Rule(
            OrganizationError::TooDeep { .. }
        ))
    ));
}

#[test]
fn from_parts_rejects_too_many_groups() {
    let parts: Vec<GroupParts> = (1..=u32::try_from(OrganizationLimits::MAX_GROUPS + 1)
        .expect("small"))
        .map(|id| part(id, &format!("g{id}"), None))
        .collect();

    let result = ModOrganization::from_parts(parts, BTreeMap::new(), FAR_NEXT_ID);

    assert!(matches!(
        result,
        Err(OrganizationDocumentError::Rule(
            OrganizationError::TooManyGroups { .. }
        ))
    ));
}

#[test]
fn from_parts_rejects_an_over_full_group_and_too_many_memberships() {
    let mut over_full = part(1, "Full", None);
    over_full.members = (0..=OrganizationLimits::MAX_MEMBERS_PER_GROUP)
        .map(|index| organized(&format!("m.{index}")))
        .collect();
    let spread: Vec<GroupParts> = (1..=11_u32)
        .map(|id| {
            let mut group = part(id, &format!("g{id}"), None);
            group.members = (0..OrganizationLimits::MAX_MEMBERS_PER_GROUP)
                .map(|index| organized(&format!("m{id}.{index}")))
                .collect();
            group
        })
        .collect();

    assert!(matches!(
        ModOrganization::from_parts(vec![over_full], BTreeMap::new(), FAR_NEXT_ID),
        Err(OrganizationDocumentError::Rule(
            OrganizationError::TooManyMembers { .. }
        ))
    ));
    assert!(matches!(
        ModOrganization::from_parts(spread, BTreeMap::new(), FAR_NEXT_ID),
        Err(OrganizationDocumentError::Rule(
            OrganizationError::TooManyMembers { .. }
        ))
    ));
}

#[test]
fn from_parts_rejects_too_many_colours() {
    let colors: BTreeMap<_, _> = (0..=OrganizationLimits::MAX_MOD_COLORS)
        .map(|index| (organized(&format!("c.{index}")), ColorToken::Red))
        .collect();

    let result = ModOrganization::from_parts(Vec::new(), colors, 1);

    assert!(matches!(
        result,
        Err(OrganizationDocumentError::Rule(
            OrganizationError::TooManyColors { .. }
        ))
    ));
}

#[test]
fn a_loaded_organization_never_reuses_an_existing_group_id() {
    let mut organization =
        ModOrganization::from_parts(vec![part(7, "Seven", None)], BTreeMap::new(), 8)
            .expect("valid");

    let fresh = root(&mut organization, "Fresh");

    assert_eq!(fresh.get(), 8);
}

#[test]
fn the_next_group_id_survives_deleting_the_newest_group_and_a_reload() {
    let mut organization = ModOrganization::new();
    root(&mut organization, "One");
    let newest = root(&mut organization, "Two");
    organization
        .delete_group(newest, DeleteMode::WithSubgroups)
        .expect("delete");
    let colors = BTreeMap::new();

    let mut reloaded = ModOrganization::from_parts(
        parts_of(&organization),
        colors,
        organization.next_group_id(),
    )
    .expect("valid");
    let fresh = root(&mut reloaded, "Three");

    assert_eq!(fresh.get(), 3, "id 2 belonged to a deleted group");
}

#[test]
fn from_parts_rejects_a_next_id_that_would_reuse_an_existing_id() {
    let result = ModOrganization::from_parts(vec![part(7, "Seven", None)], BTreeMap::new(), 7);

    assert_eq!(
        result,
        Err(OrganizationDocumentError::NextIdBehind {
            next_id: 7,
            minimum: 8
        })
    );
}

#[test]
fn from_parts_rejects_a_hostile_group_id_or_next_id_with_an_id_error() {
    let huge_group =
        ModOrganization::from_parts(vec![part(u32::MAX, "Huge", None)], BTreeMap::new(), 1);
    let huge_next = ModOrganization::from_parts(Vec::new(), BTreeMap::new(), u32::MAX);

    assert!(matches!(
        huge_group,
        Err(OrganizationDocumentError::GroupIdOutOfRange { .. })
    ));
    assert!(matches!(
        huge_next,
        Err(OrganizationDocumentError::GroupIdOutOfRange { .. })
    ));
}

#[test]
fn creating_a_group_is_refused_once_the_id_space_is_used_up() {
    let mut organization = ModOrganization::from_parts(
        Vec::new(),
        BTreeMap::new(),
        OrganizationLimits::MAX_GROUP_ID + 1,
    )
    .expect("the exhausted state is representable");

    let result = organization.create_group(name("Late"), None);

    assert!(matches!(
        result,
        Err(OrganizationError::TooManyGroups { .. })
    ));
}

// ---- limits, exactly at them -----------------------------------------------

#[test]
fn from_parts_accepts_a_forest_exactly_as_deep_as_the_limit() {
    let parts: Vec<GroupParts> = (1..=u32::try_from(OrganizationLimits::MAX_DEPTH).expect("small"))
        .map(|id| part(id, &format!("g{id}"), id.checked_sub(1).filter(|p| *p > 0)))
        .collect();

    let result = ModOrganization::from_parts(parts, BTreeMap::new(), FAR_NEXT_ID);

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn from_parts_accepts_exactly_the_most_groups() {
    let parts: Vec<GroupParts> = (1..=u32::try_from(OrganizationLimits::MAX_GROUPS)
        .expect("small"))
        .map(|id| part(id, &format!("g{id}"), None))
        .collect();

    let result = ModOrganization::from_parts(parts, BTreeMap::new(), FAR_NEXT_ID);

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn from_parts_accepts_a_group_with_exactly_the_most_members() {
    let mut full = part(1, "Full", None);
    full.members = (0..OrganizationLimits::MAX_MEMBERS_PER_GROUP)
        .map(|index| organized(&format!("m.{index}")))
        .collect();

    let result = ModOrganization::from_parts(vec![full], BTreeMap::new(), FAR_NEXT_ID);

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn from_parts_accepts_exactly_the_most_total_memberships() {
    let groups =
        OrganizationLimits::MAX_TOTAL_MEMBERSHIPS / OrganizationLimits::MAX_MEMBERS_PER_GROUP;
    let spread: Vec<GroupParts> = (1..=u32::try_from(groups).expect("small"))
        .map(|id| {
            let mut group = part(id, &format!("g{id}"), None);
            group.members = (0..OrganizationLimits::MAX_MEMBERS_PER_GROUP)
                .map(|index| organized(&format!("m{id}.{index}")))
                .collect();
            group
        })
        .collect();

    let result = ModOrganization::from_parts(spread, BTreeMap::new(), FAR_NEXT_ID);

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn from_parts_accepts_exactly_the_most_coloured_mods() {
    let colors: BTreeMap<_, _> = (0..OrganizationLimits::MAX_MOD_COLORS)
        .map(|index| (organized(&format!("c.{index}")), ColorToken::Red))
        .collect();

    let result = ModOrganization::from_parts(Vec::new(), colors, 1);

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn the_edit_path_stops_at_the_limits_too_and_not_one_before() {
    let mut organization = ModOrganization::new();
    let ids = chain(&mut organization, OrganizationLimits::MAX_DEPTH);
    let deepest = *ids.last().expect("a chain");

    let one_too_deep = organization.create_group(name("too deep"), Some(deepest));

    assert_eq!(ids.len(), OrganizationLimits::MAX_DEPTH);
    assert!(matches!(
        one_too_deep,
        Err(OrganizationError::TooDeep { .. })
    ));
}

#[test]
fn a_mod_and_its_steam_copy_share_one_colour() {
    let mut organization = ModOrganization::new();

    let change = organization
        .set_mod_colors(&mods(&["x", "x_steam"]), Some(ColorToken::Teal))
        .expect("colour");

    assert_eq!(change.changed, 1, "one mod, not two ids");
    assert_eq!(
        organization.color_of(&organized("x")),
        Some(ColorToken::Teal)
    );
    assert_eq!(
        organization.color_of(&organized("x_steam")),
        Some(ColorToken::Teal)
    );
    assert_eq!(organization.mod_color_count(), 1);
}

// ---- state -------------------------------------------------------------------

#[test]
fn only_a_loaded_state_can_be_read_or_edited() {
    let mut not_loaded = OrganizationState::NotLoaded;
    let mut recovered = OrganizationState::Recovered {
        issue: OrganizationIssue::Malformed,
    };
    let mut loaded = OrganizationState::Loaded(ModOrganization::new());

    assert_eq!(not_loaded.loaded(), Err(OrganizationUnavailable::NotLoaded));
    assert_eq!(
        recovered.loaded(),
        Err(OrganizationUnavailable::Recovered(
            OrganizationIssue::Malformed
        ))
    );
    assert!(loaded.loaded().is_ok());
    assert!(not_loaded.loaded_mut().is_err());
    assert!(recovered.loaded_mut().is_err());
    assert!(loaded.loaded_mut().is_ok());
}

#[test]
fn a_load_result_becomes_the_matching_state() {
    assert_eq!(
        OrganizationState::from(OrganizationLoad::Missing),
        OrganizationState::Loaded(ModOrganization::new())
    );
    assert_eq!(
        OrganizationState::from(OrganizationLoad::Unusable(OrganizationIssue::Unreadable)),
        OrganizationState::Recovered {
            issue: OrganizationIssue::Unreadable
        }
    );
}

// ---- resolving against the install ----------------------------------------

mod resolving {
    use super::*;
    use crate::active_set::ActiveSet;
    use crate::mod_inventory::ModInventory;
    use crate::test_support::report_fixture_with_inactive;

    fn working(ids: &[&str]) -> ActiveSet {
        ActiveSet::trusted(ids.iter().map(|id| ModId::new(*id)).collect())
    }

    #[test]
    fn active_ones_keep_working_order_and_inactive_ones_resolve_by_base() {
        let report =
            report_fixture_with_inactive(&["c", "a"], &["b.local", "b.local_steam", "idle"]);
        let inventory = ModInventory::from_report(&report);

        let resolved = resolve_scope(
            &mods(&["a", "b.local", "c", "idle", "gone"]),
            &inventory,
            &working(&["c", "a"]),
        );

        assert_eq!(resolved.active, [ModId::new("c"), ModId::new("a")]);
        assert_eq!(
            resolved.inactive,
            [ModId::new("b.local"), ModId::new("idle")],
            "the primary copy is picked over the Workshop one"
        );
        assert_eq!(resolved.not_installed, 1);
    }

    #[test]
    fn a_copy_already_active_wins_over_an_inactive_copy_of_the_same_package() {
        let report = report_fixture_with_inactive(&["pkg_steam"], &["pkg"]);
        let inventory = ModInventory::from_report(&report);

        let resolved = resolve_scope(&mods(&["pkg"]), &inventory, &working(&["pkg_steam"]));

        assert_eq!(resolved.active, [ModId::new("pkg_steam")]);
        assert!(resolved.inactive.is_empty());
    }
}

// ---- properties --------------------------------------------------------------

mod properties {
    use proptest::prelude::*;

    use super::*;

    #[derive(Debug, Clone)]
    enum Op {
        Create {
            name: u8,
            parent: Option<usize>,
        },
        Rename {
            group: usize,
            name: u8,
        },
        Reparent {
            group: usize,
            parent: Option<usize>,
        },
        Delete {
            group: usize,
            lift: bool,
        },
        Add {
            group: usize,
            mods: Vec<u8>,
        },
        Remove {
            group: usize,
            mods: Vec<u8>,
        },
        Move {
            from: usize,
            to: usize,
            mods: Vec<u8>,
        },
        Color {
            mods: Vec<u8>,
            color: Option<usize>,
        },
    }

    fn op() -> impl Strategy<Value = Op> {
        let index = 0..12_usize;
        let mods = proptest::collection::vec(0..6_u8, 0..4);
        prop_oneof![
            (0..5_u8, proptest::option::of(index.clone()))
                .prop_map(|(name, parent)| Op::Create { name, parent }),
            (index.clone(), 0..5_u8).prop_map(|(group, name)| Op::Rename { group, name }),
            (index.clone(), proptest::option::of(index.clone()))
                .prop_map(|(group, parent)| Op::Reparent { group, parent }),
            (index.clone(), any::<bool>()).prop_map(|(group, lift)| Op::Delete { group, lift }),
            (index.clone(), mods.clone()).prop_map(|(group, mods)| Op::Add { group, mods }),
            (index.clone(), mods.clone()).prop_map(|(group, mods)| Op::Remove { group, mods }),
            (index.clone(), index.clone(), mods.clone()).prop_map(|(from, to, mods)| Op::Move {
                from,
                to,
                mods
            }),
            (mods, proptest::option::of(0..12_usize))
                .prop_map(|(mods, color)| Op::Color { mods, color }),
        ]
    }

    fn nth_group(organization: &ModOrganization, index: usize) -> GroupId {
        let ids: Vec<GroupId> = organization.groups().map(|(id, _)| id).collect();
        match ids.get(index % ids.len().max(1)) {
            Some(id) => *id,
            // An id that names nothing: the aggregate must refuse it.
            None => GroupId::new(u32::MAX).expect("non-zero"),
        }
    }

    fn mod_set(raw: &[u8]) -> BTreeSet<OrganizedModId> {
        raw.iter().map(|n| organized(&format!("mod.{n}"))).collect()
    }

    fn apply(organization: &mut ModOrganization, op: Op) -> Result<(), OrganizationError> {
        match op {
            Op::Create { name: n, parent } => {
                let target = parent.map(|p| nth_group(organization, p));
                organization
                    .create_group(name(&format!("g{n}")), target)
                    .map(|_| ())
            }
            Op::Rename { group, name: n } => {
                let id = nth_group(organization, group);
                organization.rename_group(id, name(&format!("g{n}")))
            }
            Op::Reparent { group, parent } => {
                let id = nth_group(organization, group);
                let target = parent.map(|p| nth_group(organization, p));
                organization.reparent_group(id, target)
            }
            Op::Delete { group, lift } => {
                let id = nth_group(organization, group);
                let mode = if lift {
                    DeleteMode::LiftChildren
                } else {
                    DeleteMode::WithSubgroups
                };
                organization.delete_group(id, mode).map(|_| ())
            }
            Op::Add { group, mods } => {
                let id = nth_group(organization, group);
                organization.add_members(id, &mod_set(&mods)).map(|_| ())
            }
            Op::Remove { group, mods } => {
                let id = nth_group(organization, group);
                organization.remove_members(id, &mod_set(&mods)).map(|_| ())
            }
            Op::Move { from, to, mods } => {
                let source = nth_group(organization, from);
                let target = nth_group(organization, to);
                organization
                    .move_members(source, target, &mod_set(&mods))
                    .map(|_| ())
            }
            Op::Color { mods, color } => organization
                .set_mod_colors(&mod_set(&mods), color.map(|c| ColorToken::ALL[c % 12]))
                .map(|_| ()),
        }
    }

    /// The transitive closure of `id` computed from the parent links alone.
    fn closure(organization: &ModOrganization, id: GroupId) -> BTreeSet<GroupId> {
        let mut found = BTreeSet::from([id]);
        loop {
            let grown: BTreeSet<GroupId> = organization
                .groups()
                .filter(|(_, group)| group.parent().is_some_and(|p| found.contains(&p)))
                .map(|(group_id, _)| group_id)
                .chain(found.iter().copied())
                .collect();
            if grown == found {
                return found;
            }
            found = grown;
        }
    }

    fn assert_invariants(organization: &ModOrganization) {
        let mut sibling_names = BTreeSet::new();
        for (id, group) in organization.groups() {
            // Acyclic and depth-bounded: walking up terminates within the limit.
            let mut level = 1;
            let mut current = group.parent();
            while let Some(parent) = current {
                level += 1;
                assert!(level <= OrganizationLimits::MAX_DEPTH, "too deep at {id}");
                let parent_group = organization.group(parent).expect("no dangling parent");
                current = parent_group.parent();
            }
            assert!(
                sibling_names.insert((group.parent(), group.name().uniqueness_key())),
                "duplicate sibling name"
            );
            assert_eq!(
                organization.scope(id).expect("exists").groups,
                closure(organization, id),
                "scope is the transitive closure"
            );
        }
        assert!(organization.group_count() <= OrganizationLimits::MAX_GROUPS);
    }

    proptest! {
        #[test]
        fn any_mutation_sequence_keeps_the_forest_valid_and_failures_change_nothing(
            ops in proptest::collection::vec(op(), 1..60)
        ) {
            let mut organization = ModOrganization::new();
            for op in ops {
                let before = organization.clone();
                match apply(&mut organization, op) {
                    Ok(()) => {}
                    Err(_) => prop_assert_eq!(&organization, &before),
                }
                assert_invariants(&organization);
                let colors: BTreeMap<_, _> = organization
                    .mod_colors()
                    .map(|(id, color)| (id.clone(), color))
                    .collect();
                let rebuilt = ModOrganization::from_parts(parts_of(&organization), colors, organization.next_group_id());
                prop_assert!(rebuilt.is_ok(), "what the aggregate holds is a valid document");
            }
        }
    }
}
