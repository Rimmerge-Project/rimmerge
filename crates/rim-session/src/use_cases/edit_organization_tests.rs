//! `EditOrganization`: persistence, rollback, refusal without a loaded
//! organization, and the proof that no organization edit can touch the
//! working set, the orders, the sort or the ledger.

use std::collections::BTreeSet;

use rim_resolve::domain::OrderSource;

use super::*;
use crate::organization::{
    ColorToken, DeleteMode, GroupId, GroupName, ModOrganization, OrganizationIssue,
    OrganizationState, OrganizedModId,
};
use crate::ports::OrganizationSaveError;
use crate::test_support::{InMemoryOrganizationStore, organized, session_fixture_with_inactive};

fn name(text: &str) -> GroupName {
    GroupName::new(text).expect("fixture name is valid")
}

fn mods(ids: &[&str]) -> BTreeSet<OrganizedModId> {
    ids.iter().map(|id| organized(id)).collect()
}

/// A session over active `a`, `b` and inactive `idle`, with a loaded
/// organization holding group `1` ("Weapons") and group `2` ("Armor").
fn session_with_groups() -> (Session, GroupId, GroupId) {
    let mut session = session_fixture_with_inactive(&["a", "b"], &["idle"]);
    let mut organization = ModOrganization::new();
    let weapons = organization
        .create_group(name("Weapons"), None)
        .expect("create");
    let armor = organization
        .create_group(name("Armor"), None)
        .expect("create");
    organization
        .add_members(weapons, &mods(&["a", "idle"]))
        .expect("seed members");
    session.set_organization_state(OrganizationState::Loaded(organization));
    (session, weapons, armor)
}

/// One edit of every kind, valid against [`session_with_groups`].
fn every_edit(weapons: GroupId, armor: GroupId) -> Vec<OrganizationEdit> {
    vec![
        OrganizationEdit::CreateGroup {
            name: name("New"),
            parent: Some(weapons),
            mods: BTreeSet::new(),
        },
        OrganizationEdit::RenameGroup {
            id: armor,
            name: name("Plates"),
        },
        OrganizationEdit::ReparentGroup {
            id: armor,
            parent: Some(weapons),
        },
        OrganizationEdit::DeleteGroup {
            id: armor,
            mode: DeleteMode::LiftChildren,
        },
        OrganizationEdit::SetGroupColor {
            id: weapons,
            color: Some(ColorToken::Teal),
        },
        OrganizationEdit::AddMembers {
            id: armor,
            mods: mods(&["b"]),
        },
        OrganizationEdit::RemoveMembers {
            id: weapons,
            mods: mods(&["a"]),
        },
        OrganizationEdit::RemoveMembersFromGroups {
            groups: [weapons, armor].into(),
            mods: mods(&["idle"]),
        },
        OrganizationEdit::MoveMembers {
            from: weapons,
            to: armor,
            mods: mods(&["a"]),
        },
        OrganizationEdit::SetModColors {
            mods: mods(&["a", "idle"]),
            color: Some(ColorToken::Red),
        },
    ]
}

#[test]
fn each_edit_is_applied_and_saved_exactly_once() {
    for edit in every_edit(
        GroupId::new(1).expect("non-zero"),
        GroupId::new(2).expect("non-zero"),
    ) {
        let (mut session, _, _) = session_with_groups();
        let store = InMemoryOrganizationStore::new();

        EditOrganization::new(&store)
            .execute(&mut session, edit.clone())
            .unwrap_or_else(|error| panic!("{edit:?} must succeed: {error}"));

        assert_eq!(store.save_count(), 1, "{edit:?} saves once");
        assert_eq!(
            store.last_saved().as_ref(),
            session.organization().loaded().ok(),
            "{edit:?}: what was saved is what the session holds"
        );
    }
}

#[test]
fn a_failed_save_rolls_every_edit_back() {
    for edit in every_edit(
        GroupId::new(1).expect("non-zero"),
        GroupId::new(2).expect("non-zero"),
    ) {
        let (mut session, _, _) = session_with_groups();
        let before = session.organization().clone();
        let store = InMemoryOrganizationStore::new();
        store.fail_next_save();

        let result = EditOrganization::new(&store).execute(&mut session, edit.clone());

        assert!(
            matches!(
                result,
                Err(EditOrganizationError::Save(OrganizationSaveError::Failed(
                    _
                )))
            ),
            "{edit:?} reports the save failure"
        );
        assert_eq!(session.organization(), &before, "{edit:?} is rolled back");
    }
}

#[test]
fn a_document_over_the_file_limit_is_a_typed_refusal_and_rolls_back() {
    let (mut session, weapons, _) = session_with_groups();
    let before = session.organization().clone();
    let store = InMemoryOrganizationStore::new();
    store.fail_next_save_too_large();

    let result = EditOrganization::new(&store).execute(
        &mut session,
        OrganizationEdit::AddMembers {
            id: weapons,
            mods: mods(&["b"]),
        },
    );

    assert!(matches!(
        result,
        Err(EditOrganizationError::Save(
            OrganizationSaveError::TooLarge { .. }
        ))
    ));
    assert_eq!(session.organization(), &before);
}

#[test]
fn an_edit_that_changes_nothing_does_not_write_the_file() {
    let (mut session, weapons, _) = session_with_groups();
    let store = InMemoryOrganizationStore::new();

    let outcome = EditOrganization::new(&store)
        .execute(
            &mut session,
            OrganizationEdit::AddMembers {
                id: weapons,
                mods: mods(&["a"]),
            },
        )
        .expect("adding an existing member is fine");

    assert!(matches!(
        outcome,
        EditOutcome::Added(added) if added.added == 0 && added.already == 1
    ));
    assert_eq!(store.save_count(), 0);
}

#[test]
fn a_rejected_edit_reports_the_rule_and_writes_nothing() {
    let (mut session, weapons, _) = session_with_groups();
    let before = session.organization().clone();
    let store = InMemoryOrganizationStore::new();

    let result = EditOrganization::new(&store).execute(
        &mut session,
        OrganizationEdit::ReparentGroup {
            id: weapons,
            parent: Some(weapons),
        },
    );

    assert!(matches!(
        result,
        Err(EditOrganizationError::Rejected(
            OrganizationError::WouldCreateCycle
        ))
    ));
    assert_eq!(session.organization(), &before);
    assert_eq!(store.save_count(), 0);
}

#[test]
fn a_session_without_organization_refuses_every_edit_and_saves_nothing() {
    for state in [
        OrganizationState::NotLoaded,
        OrganizationState::Recovered {
            issue: OrganizationIssue::Malformed,
        },
    ] {
        for edit in every_edit(
            GroupId::new(1).expect("non-zero"),
            GroupId::new(2).expect("non-zero"),
        ) {
            let (mut session, _, _) = session_with_groups();
            session.set_organization_state(state.clone());
            let store = InMemoryOrganizationStore::new();

            let result = EditOrganization::new(&store).execute(&mut session, edit.clone());

            assert!(
                matches!(result, Err(EditOrganizationError::Unavailable(_))),
                "{state:?} refuses {edit:?}"
            );
            assert_eq!(store.save_count(), 0, "{state:?} never saves");
            assert_eq!(session.organization(), &state);
        }
    }
}

/// Everything organization must never touch, as comparable text.
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
        // `LoadOrder` holds a `HashMap` index whose `Debug` order varies, so
        // compare the sort's own ordered parts.
        format!(
            "{:?} {:?} {:?} {:?} {:?} {:?}",
            session.sort_outcome().order.as_slice(),
            session.sort_outcome().placements,
            session.sort_outcome().dropped,
            session.sort_outcome().any_of_choices,
            session.sort_outcome().warnings,
            session.sort_outcome().stats
        ),
        format!("{:?}", session.rules()),
        format!("{:?}", session.decisions()),
        ledger,
    ]
}

#[test]
fn no_organization_edit_touches_the_working_set_orders_sort_or_ledger() {
    for edit in every_edit(
        GroupId::new(1).expect("non-zero"),
        GroupId::new(2).expect("non-zero"),
    ) {
        let (mut session, _, _) = session_with_groups();
        let before = world(&mut session);
        let store = InMemoryOrganizationStore::new();

        EditOrganization::new(&store)
            .execute(&mut session, edit.clone())
            .expect("the edit succeeds");

        assert_eq!(
            world(&mut session),
            before,
            "{edit:?} changed something else"
        );
    }
}

/// A session whose sort and ledger have something to say: `a` must load
/// after `c`, so the sorted order differs from the scanned one and the
/// ledger holds the edge. An organization that leaked into either would
/// change what [`world`] prints.
fn session_with_edges() -> Session {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .mod_("c")
        .declares_load_after("a", "c")
        .force_load_after_edge("a", "c")
        .inactive("idle")
        .build();
    crate::test_support::session_with_sources_and_mods(
        rim_analyzer::analysis::SourceIndex::default(),
        report,
        &["a", "b", "c"],
    )
}

#[test]
fn the_sort_and_the_ledger_are_identical_with_an_empty_and_a_large_organization() {
    let mut plain = session_with_edges();
    plain.set_organization_state(OrganizationState::Loaded(ModOrganization::new()));
    let mut organized_session = session_with_edges();
    let mut big = ModOrganization::new();
    for index in 0..200 {
        let group = big
            .create_group(name(&format!("group {index}")), None)
            .expect("create");
        big.add_members(group, &mods(&["a", "b", "c", "idle"]))
            .expect("add");
    }
    big.set_mod_colors(&mods(&["a", "b", "c"]), Some(ColorToken::Pink))
        .expect("colour");
    organized_session.set_organization_state(OrganizationState::Loaded(big));
    assert_ne!(
        plain.sort_outcome().order.as_slice(),
        plain.orders().current.as_slice(),
        "the fixture's edge must move something, or this proves nothing"
    );

    assert_eq!(world(&mut plain), world(&mut organized_session));
}
