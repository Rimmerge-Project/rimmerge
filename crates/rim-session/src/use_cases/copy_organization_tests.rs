//! `CopyOrganization`: the merge and replace rules, every refusal, the
//! one atomic save, and the proof that a copy touches only organization.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rim_resolve::domain::OrderSource;

use super::*;
use crate::organization::{
    ColorToken, GroupId, GroupName, ModOrganization, OrganizationLimits, OrganizationState,
};
use crate::test_support::{InMemoryOrganizationStore, organized, session_fixture_with_inactive};

fn source_dir() -> PathBuf {
    PathBuf::from("other-profile")
}

fn name(text: &str) -> GroupName {
    GroupName::new(text).expect("fixture name is valid")
}

fn mods(ids: &[&str]) -> BTreeSet<OrganizedModId> {
    ids.iter().map(|id| organized(id)).collect()
}

fn group(
    organization: &mut ModOrganization,
    text: &str,
    parent: Option<GroupId>,
    members: &[&str],
    color: Option<ColorToken>,
) -> GroupId {
    let id = organization
        .create_group_with_members(name(text), parent, &mods(members))
        .expect("create fixture group");
    organization
        .set_group_color(id, color)
        .expect("colour fixture group");
    id
}

fn session_with(organization: ModOrganization) -> Session {
    let mut session = session_fixture_with_inactive(&["a", "b"], &["idle"]);
    session.set_organization_state(OrganizationState::Loaded(organization));
    session
}

/// A store whose source folder holds `source`.
fn store_with_source(source: ModOrganization) -> InMemoryOrganizationStore {
    let store = InMemoryOrganizationStore::new();
    store.seed_dir(source_dir(), OrganizationLoad::Loaded(source));
    store
}

fn loaded(session: &Session) -> &ModOrganization {
    session.organization().loaded().expect("loaded")
}

fn names_of(organization: &ModOrganization) -> BTreeSet<String> {
    organization
        .tree()
        .iter()
        .map(|node| node.name.to_string())
        .collect()
}

fn members_of(organization: &ModOrganization, id: GroupId) -> BTreeSet<OrganizedModId> {
    organization.group(id).expect("group").members().clone()
}

#[test]
fn copying_into_an_empty_profile_adds_every_group_member_and_colour() {
    let mut source = ModOrganization::new();
    let top = group(
        &mut source,
        "Weapons",
        None,
        &["a", "x"],
        Some(ColorToken::Red),
    );
    group(&mut source, "Melee", Some(top), &["b"], None);
    source
        .set_mod_colors(&mods(&["a"]), Some(ColorToken::Teal))
        .expect("colour");
    let mut session = session_with(ModOrganization::new());
    let store = store_with_source(source.clone());

    let report = CopyOrganization::new(&store)
        .execute(&mut session, &source_dir(), CopyMode::Merge)
        .expect("copy");

    let after = loaded(&session);
    assert_eq!(
        names_of(after),
        BTreeSet::from(["Weapons".into(), "Melee".into()])
    );
    let new_top = after.tree()[0].id;
    assert_eq!(members_of(after, new_top), mods(&["a", "x"]));
    assert_eq!(
        after.group(new_top).expect("group").color(),
        Some(ColorToken::Red)
    );
    assert_eq!(
        after.tree()[1].parent,
        Some(new_top),
        "Melee stays under Weapons"
    );
    assert_eq!(after.color_of(&organized("a")), Some(ColorToken::Teal));
    assert_eq!(report.groups_added, 2);
    assert_eq!(report.groups_merged, 0);
    assert_eq!(report.memberships_added, 3);
    assert_eq!(report.mod_colors_added, 1);
    assert_eq!(report.not_installed_mods, 1, "x is installed nowhere");
    assert_eq!(store.save_count(), 1, "one atomic save");
    assert_eq!(store.last_saved().as_ref(), Some(after));
}

#[test]
fn a_copy_never_writes_to_the_source_folder() {
    let mut source = ModOrganization::new();
    group(&mut source, "G", None, &["a"], None);
    let mut session = session_with(ModOrganization::new());
    let store = store_with_source(source);

    CopyOrganization::new(&store)
        .execute(&mut session, &source_dir(), CopyMode::Merge)
        .expect("copy");

    assert_eq!(
        store.saved_dirs(),
        vec![session.paths().profile_dir.clone()],
        "only this profile's folder is written"
    );
}

#[test]
fn a_group_at_the_same_path_takes_the_members_and_keeps_its_id() {
    let mut mine = ModOrganization::new();
    let gear = group(&mut mine, "Gear", None, &["a"], None);
    let swords = group(&mut mine, "Swords", Some(gear), &["b"], None);
    let mut source = ModOrganization::new();
    let their_gear = group(&mut source, "gear", None, &["a", "c"], None);
    group(&mut source, "SWORDS", Some(their_gear), &["b", "d"], None);
    group(&mut source, "Bows", Some(their_gear), &["e"], None);
    let mut session = session_with(mine);
    let store = store_with_source(source);

    let report = CopyOrganization::new(&store)
        .execute(&mut session, &source_dir(), CopyMode::Merge)
        .expect("copy");

    let after = loaded(&session);
    assert_eq!(members_of(after, gear), mods(&["a", "c"]));
    assert_eq!(members_of(after, swords), mods(&["b", "d"]));
    assert_eq!(
        after.group(gear).expect("group").name().to_string(),
        "Gear",
        "this profile's spelling stays"
    );
    let bows = after
        .tree()
        .into_iter()
        .find(|node| node.name.to_string() == "Bows")
        .expect("Bows was added");
    assert_eq!(
        bows.parent,
        Some(gear),
        "a new group goes under the same parent path"
    );
    assert_eq!(after.group_count(), 3);
    assert_eq!(report.groups_merged, 2);
    assert_eq!(report.groups_added, 1);
    assert_eq!(report.memberships_added, 3);
    assert_eq!(report.memberships_already, 2);
}

#[test]
fn this_profiles_group_colour_wins_and_the_clash_names_the_path() {
    let mut mine = ModOrganization::new();
    let gear = group(&mut mine, "Gear", None, &[], Some(ColorToken::Blue));
    group(&mut mine, "Swords", Some(gear), &[], Some(ColorToken::Red));
    group(&mut mine, "Bows", Some(gear), &[], None);
    let mut source = ModOrganization::new();
    let their_gear = group(&mut source, "Gear", None, &[], Some(ColorToken::Green));
    group(
        &mut source,
        "Swords",
        Some(their_gear),
        &[],
        Some(ColorToken::Red),
    );
    group(
        &mut source,
        "Bows",
        Some(their_gear),
        &[],
        Some(ColorToken::Pink),
    );
    let mut session = session_with(mine);
    let store = store_with_source(source);

    let report = CopyOrganization::new(&store)
        .execute(&mut session, &source_dir(), CopyMode::Merge)
        .expect("copy");

    let after = loaded(&session);
    assert_eq!(
        after.group(gear).expect("group").color(),
        Some(ColorToken::Blue)
    );
    let bows = after
        .tree()
        .into_iter()
        .find(|node| node.name.to_string() == "Bows")
        .expect("Bows");
    assert_eq!(
        bows.color,
        Some(ColorToken::Pink),
        "an unset colour is filled in"
    );
    assert_eq!(report.group_colors_adopted, 1);
    assert_eq!(report.clashes.len(), 1, "an equal colour is no clash");
    assert_eq!(report.clashes[0].path, vec![name("Gear")]);
    assert_eq!(report.clashes[0].kept, ColorToken::Blue);
    assert_eq!(report.clashes[0].offered, ColorToken::Green);
}

#[test]
fn this_profiles_mod_colour_wins_and_only_unset_colours_are_added() {
    let mut mine = ModOrganization::new();
    mine.set_mod_colors(&mods(&["a"]), Some(ColorToken::Red))
        .expect("colour");
    let mut source = ModOrganization::new();
    source
        .set_mod_colors(&mods(&["a"]), Some(ColorToken::Teal))
        .expect("colour");
    source
        .set_mod_colors(&mods(&["b"]), Some(ColorToken::Teal))
        .expect("colour");
    let mut session = session_with(mine);
    let store = store_with_source(source);

    let report = CopyOrganization::new(&store)
        .execute(&mut session, &source_dir(), CopyMode::Merge)
        .expect("copy");

    let after = loaded(&session);
    assert_eq!(after.color_of(&organized("a")), Some(ColorToken::Red));
    assert_eq!(after.color_of(&organized("b")), Some(ColorToken::Teal));
    assert_eq!(report.mod_colors_kept, 1);
    assert_eq!(report.mod_colors_added, 1);
}

#[test]
fn the_same_name_at_a_different_depth_is_a_different_group() {
    let mut mine = ModOrganization::new();
    let gear = group(&mut mine, "Gear", None, &[], None);
    let nested = group(&mut mine, "Weapons", Some(gear), &["a"], None);
    let mut source = ModOrganization::new();
    let top = group(&mut source, "Weapons", None, &["b"], None);
    group(&mut source, "Gear", Some(top), &["c"], None);
    let mut session = session_with(mine);
    let store = store_with_source(source);

    let report = CopyOrganization::new(&store)
        .execute(&mut session, &source_dir(), CopyMode::Merge)
        .expect("copy");

    let after = loaded(&session);
    assert_eq!(
        members_of(after, nested),
        mods(&["a"]),
        "the nested Weapons is untouched"
    );
    assert_eq!(
        after.group_count(),
        4,
        "a top-level Weapons and a Weapons/Gear are new"
    );
    assert_eq!(report.groups_added, 2);
    assert_eq!(report.groups_merged, 0);
    assert!(report.clashes.is_empty());
}

#[test]
fn a_merge_that_would_pass_the_group_limit_changes_nothing() {
    let mut mine = ModOrganization::new();
    for index in 0..OrganizationLimits::MAX_GROUPS - 1 {
        group(&mut mine, &format!("g{index}"), None, &[], None);
    }
    let mut source = ModOrganization::new();
    group(&mut source, "new one", None, &["a"], None);
    group(&mut source, "new two", None, &["a"], None);
    let mut session = session_with(mine.clone());
    let store = store_with_source(source);

    let result =
        CopyOrganization::new(&store).execute(&mut session, &source_dir(), CopyMode::Merge);

    assert_eq!(
        result,
        Err(CopyOrganizationError::Refused(CopyRefusal::Limit(
            OrganizationError::TooManyGroups {
                limit: OrganizationLimits::MAX_GROUPS
            }
        )))
    );
    assert_eq!(loaded(&session), &mine, "the session is unchanged");
    assert_eq!(store.save_count(), 0, "nothing was saved");
}

#[test]
fn a_merge_that_would_pass_the_members_limit_of_a_group_is_refused_whole() {
    let mut mine = ModOrganization::new();
    let full = group(&mut mine, "Full", None, &[], None);
    let filler: BTreeSet<OrganizedModId> = (0..OrganizationLimits::MAX_MEMBERS_PER_GROUP)
        .map(|index| organized(&format!("filler.{index}")))
        .collect();
    mine.add_members(full, &filler).expect("fill");
    let mut source = ModOrganization::new();
    group(&mut source, "Elsewhere", None, &["a"], None);
    group(&mut source, "Full", None, &["one.more"], None);
    let mut session = session_with(mine.clone());
    let store = store_with_source(source);

    let result =
        CopyOrganization::new(&store).execute(&mut session, &source_dir(), CopyMode::Merge);

    assert!(matches!(
        result,
        Err(CopyOrganizationError::Refused(CopyRefusal::Limit(
            OrganizationError::TooManyMembers { .. }
        )))
    ));
    assert_eq!(
        loaded(&session),
        &mine,
        "the group that fit was not copied either"
    );
    assert_eq!(store.save_count(), 0);
}

#[test]
fn replace_makes_the_source_the_whole_organization_with_fresh_ids() {
    let mut mine = ModOrganization::new();
    group(&mut mine, "Old", None, &["a", "b"], Some(ColorToken::Red));
    mine.set_mod_colors(&mods(&["a"]), Some(ColorToken::Blue))
        .expect("colour");
    let next_before = mine.next_group_id();
    let mut source = ModOrganization::new();
    let top = group(&mut source, "Old", None, &["c"], Some(ColorToken::Green));
    group(&mut source, "Kid", Some(top), &[], None);
    let mut session = session_with(mine);
    let store = store_with_source(source);

    let report = CopyOrganization::new(&store)
        .execute(&mut session, &source_dir(), CopyMode::Replace)
        .expect("copy");

    let after = loaded(&session);
    let tree = after.tree();
    assert_eq!(tree.len(), 2);
    assert_eq!(
        members_of(after, tree[0].id),
        mods(&["c"]),
        "nothing of the old members stays"
    );
    assert_eq!(tree[0].color, Some(ColorToken::Green));
    assert_eq!(
        after.color_of(&organized("a")),
        None,
        "the old mod colours are gone"
    );
    assert!(
        tree.iter().all(|node| node.id.get() >= next_before),
        "no id this profile handed out is reused"
    );
    assert_eq!(report.discarded.groups, 1);
    assert_eq!(report.discarded.memberships, 2);
    assert_eq!(report.discarded.mod_colors, 1);
    assert_eq!(report.groups_merged, 0, "replace merges nothing");
    assert_eq!(report.groups_added, 2);
    assert_eq!(store.save_count(), 1);
}

#[test]
fn a_replace_with_no_group_ids_left_is_refused_and_changes_nothing() {
    let exhausted = ModOrganization::from_parts(
        Vec::new(),
        BTreeMap::new(),
        OrganizationLimits::MAX_GROUP_ID + 1,
    )
    .expect("the last next id is allowed");
    let mut source = ModOrganization::new();
    group(&mut source, "G", None, &[], None);
    let mut session = session_with(exhausted.clone());
    let store = store_with_source(source);

    let result =
        CopyOrganization::new(&store).execute(&mut session, &source_dir(), CopyMode::Replace);

    assert_eq!(
        result,
        Err(CopyOrganizationError::Refused(CopyRefusal::Limit(
            OrganizationError::GroupIdsExhausted
        )))
    );
    assert_eq!(loaded(&session), &exhausted);
    assert_eq!(store.save_count(), 0);
}

#[test]
fn a_source_that_cannot_be_read_is_refused_with_its_reason_in_either_mode() {
    let cases = [
        (OrganizationLoad::Missing, CopyRefusal::SourceMissing),
        (
            OrganizationLoad::Unusable(OrganizationIssue::Malformed),
            CopyRefusal::SourceUnusable(OrganizationIssue::Malformed),
        ),
        (
            OrganizationLoad::Unusable(OrganizationIssue::TooLarge { limit_bytes: 4 }),
            CopyRefusal::SourceUnusable(OrganizationIssue::TooLarge { limit_bytes: 4 }),
        ),
        (
            OrganizationLoad::Loaded(ModOrganization::new()),
            CopyRefusal::SourceEmpty,
        ),
    ];
    for mode in [CopyMode::Merge, CopyMode::Replace] {
        for (load, refusal) in &cases {
            let mut mine = ModOrganization::new();
            group(&mut mine, "Mine", None, &["a"], None);
            let mut session = session_with(mine.clone());
            let store = InMemoryOrganizationStore::new();
            store.seed_dir(source_dir(), load.clone());

            let result = CopyOrganization::new(&store).execute(&mut session, &source_dir(), mode);

            assert_eq!(
                result,
                Err(CopyOrganizationError::Refused(refusal.clone())),
                "{mode:?} {load:?}"
            );
            assert_eq!(loaded(&session), &mine);
            assert_eq!(store.save_count(), 0);
        }
    }
}

#[test]
fn an_unavailable_organization_refuses_the_copy_and_never_reads_the_source() {
    for state in [
        OrganizationState::NotLoaded,
        OrganizationState::Recovered {
            issue: OrganizationIssue::Malformed,
        },
    ] {
        let mut session = session_with(ModOrganization::new());
        session.set_organization_state(state.clone());
        let mut source = ModOrganization::new();
        group(&mut source, "G", None, &[], None);
        let store = store_with_source(source);

        let use_case = CopyOrganization::new(&store);

        let copied = use_case.execute(&mut session, &source_dir(), CopyMode::Merge);
        let previewed = use_case.preview(&session, &source_dir(), CopyMode::Merge);

        assert!(
            matches!(copied, Err(CopyOrganizationError::Unavailable(_))),
            "{state:?}"
        );
        assert!(
            matches!(previewed, Err(CopyOrganizationError::Unavailable(_))),
            "{state:?}"
        );
        assert_eq!(session.organization(), &state);
        assert_eq!(store.save_count(), 0);
        assert_eq!(store.load_count(), 0, "the source was read for nothing");
    }
}

#[test]
fn a_failed_save_leaves_the_session_as_it_was() {
    let mut source = ModOrganization::new();
    group(&mut source, "G", None, &["a"], None);
    let mut session = session_with(ModOrganization::new());
    let store = store_with_source(source);
    store.fail_next_save();

    let result =
        CopyOrganization::new(&store).execute(&mut session, &source_dir(), CopyMode::Merge);

    assert!(matches!(result, Err(CopyOrganizationError::Save(_))));
    assert_eq!(loaded(&session), &ModOrganization::new());
}

#[test]
fn a_result_too_big_for_the_file_is_a_refusal_and_the_session_is_unchanged() {
    let mut source = ModOrganization::new();
    group(&mut source, "G", None, &["a"], None);
    let mut session = session_with(ModOrganization::new());
    let store = store_with_source(source);
    store.fail_next_save_too_large();

    let result =
        CopyOrganization::new(&store).execute(&mut session, &source_dir(), CopyMode::Merge);

    assert!(matches!(
        result,
        Err(CopyOrganizationError::Refused(
            CopyRefusal::TooLargeToSave { .. }
        ))
    ));
    assert_eq!(loaded(&session), &ModOrganization::new());
}

#[test]
fn copying_an_organization_identical_to_this_one_is_refused_in_either_mode() {
    for mode in [CopyMode::Merge, CopyMode::Replace] {
        let mut mine = ModOrganization::new();
        group(&mut mine, "G", None, &["a"], Some(ColorToken::Red));
        let mut session = session_with(mine.clone());
        let store = store_with_source(mine.clone());
        let use_case = CopyOrganization::new(&store);

        let previewed = use_case.preview(&session, &source_dir(), mode);
        let copied = use_case.execute(&mut session, &source_dir(), mode);

        let refusal = Err(CopyOrganizationError::Refused(CopyRefusal::NothingToCopy));
        assert_eq!(previewed, refusal, "{mode:?}");
        assert_eq!(copied, refusal, "{mode:?}");
        assert_eq!(store.save_count(), 0);
        assert_eq!(loaded(&session), &mine);
    }
}

#[test]
fn a_merge_that_adds_nothing_new_writes_nothing() {
    let mut mine = ModOrganization::new();
    group(&mut mine, "G", None, &["a", "b"], Some(ColorToken::Red));
    let mut source = ModOrganization::new();
    group(&mut source, "g", None, &["a"], Some(ColorToken::Red));
    let mut session = session_with(mine);
    let store = store_with_source(source);

    let report = CopyOrganization::new(&store)
        .execute(&mut session, &source_dir(), CopyMode::Merge)
        .expect("copy");

    assert_eq!(store.save_count(), 0);
    assert_eq!(report.groups_merged, 1);
    assert_eq!(report.memberships_added, 0);
}

#[test]
fn a_result_with_too_many_coloured_mods_is_refused_whole() {
    let mut mine = ModOrganization::new();
    let filler: BTreeSet<OrganizedModId> = (0..OrganizationLimits::MAX_MOD_COLORS)
        .map(|index| organized(&format!("filler.{index}")))
        .collect();
    mine.set_mod_colors(&filler, Some(ColorToken::Red))
        .expect("fill");
    let mut source = ModOrganization::new();
    group(&mut source, "Elsewhere", None, &["a"], None);
    source
        .set_mod_colors(&mods(&["one.more"]), Some(ColorToken::Teal))
        .expect("colour");
    let mut session = session_with(mine.clone());
    let store = store_with_source(source);

    let result =
        CopyOrganization::new(&store).execute(&mut session, &source_dir(), CopyMode::Merge);

    assert!(matches!(
        result,
        Err(CopyOrganizationError::Refused(CopyRefusal::Limit(
            OrganizationError::TooManyColors { .. }
        )))
    ));
    assert_eq!(loaded(&session), &mine, "the group was not copied either");
    assert_eq!(store.save_count(), 0);
}

#[test]
fn execute_plans_from_the_source_as_it_is_now_not_as_it_was_previewed() {
    let mut first = ModOrganization::new();
    group(&mut first, "Before", None, &["a"], None);
    let mut second = ModOrganization::new();
    group(&mut second, "After", None, &["b"], None);
    let mut session = session_with(ModOrganization::new());
    let store = store_with_source(first);
    let use_case = CopyOrganization::new(&store);
    let previewed = use_case
        .preview(&session, &source_dir(), CopyMode::Merge)
        .expect("preview");
    store.seed_dir(source_dir(), OrganizationLoad::Loaded(second));

    let done = use_case
        .execute(&mut session, &source_dir(), CopyMode::Merge)
        .expect("copy");

    assert_eq!(previewed.groups_added, 1);
    assert_eq!(done.groups_added, 1);
    assert_eq!(
        names_of(loaded(&session)),
        BTreeSet::from(["After".to_string()]),
        "the changed source is what was copied"
    );
}

#[test]
fn a_source_read_beforehand_is_what_is_planned_not_what_the_store_holds_now() {
    let mut on_disk = ModOrganization::new();
    group(&mut on_disk, "OnDisk", None, &["a"], None);
    let mut read_earlier = ModOrganization::new();
    group(&mut read_earlier, "ReadEarlier", None, &["b"], None);
    let mut session = session_with(ModOrganization::new());
    let store = store_with_source(on_disk);
    let use_case = CopyOrganization::new(&store);

    let previewed = use_case
        .preview_loaded(
            &session,
            OrganizationLoad::Loaded(read_earlier.clone()),
            CopyMode::Merge,
        )
        .expect("preview");
    use_case
        .execute_loaded(
            &mut session,
            OrganizationLoad::Loaded(read_earlier),
            CopyMode::Merge,
        )
        .expect("copy");

    assert_eq!(previewed.groups_added, 1);
    assert_eq!(
        names_of(loaded(&session)),
        BTreeSet::from(["ReadEarlier".to_string()])
    );
}

#[test]
fn a_source_read_beforehand_is_planned_against_this_profile_as_it_is_at_the_commit() {
    let mut source = ModOrganization::new();
    group(&mut source, "Theirs", None, &["a"], None);
    let mut session = session_with(ModOrganization::new());
    let store = store_with_source(source);
    let use_case = CopyOrganization::new(&store);
    let read = use_case.read_source(&source_dir());
    let mut mine = ModOrganization::new();
    group(&mut mine, "Mine", None, &["b"], None);
    session.set_organization_state(OrganizationState::Loaded(mine));

    use_case
        .execute_loaded(&mut session, read, CopyMode::Merge)
        .expect("copy");

    assert_eq!(
        names_of(loaded(&session)),
        BTreeSet::from(["Mine".to_string(), "Theirs".to_string()]),
        "an edit made after the read is kept"
    );
}

#[test]
fn a_preview_reports_what_execute_does_and_changes_nothing() {
    let mut mine = ModOrganization::new();
    let gear = group(&mut mine, "Gear", None, &["a"], Some(ColorToken::Blue));
    let mut source = ModOrganization::new();
    group(
        &mut source,
        "Gear",
        None,
        &["a", "b"],
        Some(ColorToken::Red),
    );
    group(&mut source, "Extra", None, &["c"], None);
    let mut session = session_with(mine.clone());
    let store = store_with_source(source);
    let use_case = CopyOrganization::new(&store);

    let previewed = use_case
        .preview(&session, &source_dir(), CopyMode::Merge)
        .expect("preview");

    assert_eq!(loaded(&session), &mine, "a preview is a read");
    assert_eq!(store.save_count(), 0);
    let done = use_case
        .execute(&mut session, &source_dir(), CopyMode::Merge)
        .expect("copy");
    assert_eq!(previewed, done);
    assert_eq!(members_of(loaded(&session), gear), mods(&["a", "b"]));
}

/// Everything a copy must never touch, as comparable text.
fn world(session: &mut Session) -> Vec<String> {
    let ledger = format!("{:?}", session.ledger(OrderSource::Current));
    vec![
        format!("{:?}", session.working().ids()),
        format!(
            "{:?} {:?}",
            session.orders().current.as_slice(),
            session.orders().suggested.as_slice()
        ),
        format!("{:?}", session.file_active_mods()),
        format!("{:?}", session.selected()),
        format!("{:?}", session.pending_changes()),
        format!("{:?}", session.sort_outcome().order.as_slice()),
        format!("{:?}", session.rules()),
        format!("{:?}", session.decisions()),
        ledger,
    ]
}

#[test]
fn a_copy_in_either_mode_leaves_the_order_the_active_mods_and_the_rules_alone() {
    for mode in [CopyMode::Merge, CopyMode::Replace] {
        let mut mine = ModOrganization::new();
        group(&mut mine, "Mine", None, &["a"], None);
        let mut source = ModOrganization::new();
        group(
            &mut source,
            "Theirs",
            None,
            &["a", "idle", "b"],
            Some(ColorToken::Pink),
        );
        source
            .set_mod_colors(&mods(&["b"]), Some(ColorToken::Teal))
            .expect("colour");
        let mut session = session_with(mine);
        let before = world(&mut session);
        let store = store_with_source(source);

        CopyOrganization::new(&store)
            .execute(&mut session, &source_dir(), mode)
            .expect("copy");

        assert_eq!(
            world(&mut session),
            before,
            "{mode:?} changed something else"
        );
    }
}
