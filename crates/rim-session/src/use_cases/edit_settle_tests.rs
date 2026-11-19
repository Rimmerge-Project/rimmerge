//! The edit-direction settle across the Mods-page use cases: a pending
//! membership edit, a move that stays in `working`, then the undo of the
//! membership edit must land the sequence in Current, whichever use case
//! made the undo. Each call site of `settle_current_onto_working` has its
//! own scenario so removing one is caught.

use proptest::prelude::*;
use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::ModId;
use rim_resolve::domain::OrderSource;

use crate::Session;
use crate::active_set::Anchor;
use crate::test_support::{
    session_fixture, session_fixture_with_inactive, session_with_sources_and_mods,
};
use crate::use_cases::{
    ActivateMods, DeactivateMods, DiscardPendingChanges, MoveMods, RevertOrderToFile,
};

fn mods(ids: &[&str]) -> Vec<ModId> {
    ids.iter().map(|id| ModId::new(*id)).collect()
}

fn same_multiset(left: &[ModId], right: &[ModId]) -> bool {
    let mut left = left.to_vec();
    let mut right = right.to_vec();
    left.sort();
    right.sort();
    left == right
}

#[test]
fn activating_back_after_a_pending_move_settles_the_sequence_into_current() {
    let mut session = session_fixture(&["a", "b", "c", "d"]);
    let removal = DeactivateMods::plan(&session, &mods(&["b"]));
    DeactivateMods::execute(&mut session, &removal);
    MoveMods::execute(&mut session, &mods(&["d"]), &Anchor::Start).expect("valid move");
    assert_eq!(
        session.orders().current.as_slice(),
        mods(&["a", "b", "c", "d"]),
        "the move stays in working while a membership edit is pending"
    );

    let addition = ActivateMods::plan(&session, &mods(&["b"]), false);
    ActivateMods::execute(&mut session, &addition).expect("b is known");

    assert_eq!(session.working().ids(), mods(&["d", "a", "b", "c"]));
    assert_eq!(
        session.orders().current.as_slice(),
        mods(&["d", "a", "b", "c"]),
        "the undo makes the ids equal again, so the staged move reaches Current"
    );
}

#[test]
fn deactivating_back_after_a_pending_move_settles_the_sequence_into_current() {
    let mut session = session_fixture_with_inactive(&["a", "b", "c", "d"], &["x"]);
    let addition = ActivateMods::plan(&session, &mods(&["x"]), false);
    ActivateMods::execute(&mut session, &addition).expect("x is known");
    MoveMods::execute(&mut session, &mods(&["d"]), &Anchor::Start).expect("valid move");
    assert_eq!(
        session.orders().current.as_slice(),
        mods(&["a", "b", "c", "d"]),
        "the move stays in working while a membership edit is pending"
    );

    let removal = DeactivateMods::plan(&session, &mods(&["x"]));
    DeactivateMods::execute(&mut session, &removal);

    assert_eq!(session.working().ids(), mods(&["d", "a", "b", "c"]));
    assert_eq!(
        session.orders().current.as_slice(),
        mods(&["d", "a", "b", "c"]),
        "the undo makes the ids equal again, so the staged move reaches Current"
    );
}

#[test]
fn reactivating_a_dependent_lands_behind_a_dependency_that_was_moved_after_its_old_slot() {
    // Current is [dependency, a, dependent]; the dependent goes inactive and
    // the dependency moves to the end, so working is [a, dependency]. The
    // dependent's old slot (after `a`) would put it before its dependency.
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("dependency")
        .mod_("a")
        .mod_("dependent")
        .dependency("dependent", "dependency")
        .build();
    let mut session = session_with_sources_and_mods(
        SourceIndex::default(),
        report,
        &["dependency", "a", "dependent"],
    );
    let removal = DeactivateMods::plan(&session, &mods(&["dependent"]));
    DeactivateMods::execute(&mut session, &removal);
    MoveMods::execute(&mut session, &mods(&["dependency"]), &Anchor::End).expect("valid move");
    assert_eq!(session.working().ids(), mods(&["a", "dependency"]));

    let addition = ActivateMods::plan(&session, &mods(&["dependent"]), false);
    ActivateMods::execute(&mut session, &addition).expect("dependent is known");

    assert_eq!(
        session.working().ids(),
        mods(&["a", "dependency", "dependent"])
    );
}

#[test]
fn an_anchored_activation_switches_a_selected_suggested_to_current() {
    let mut session = session_fixture_with_inactive(&["a", "b"], &["x"]);
    session.select(OrderSource::Suggested);
    let plan = ActivateMods::plan(&session, &mods(&["x"]), false);

    let outcome =
        ActivateMods::execute_at(&mut session, &plan, &Anchor::Start).expect("valid drop");

    assert!(outcome.did_switch_to_current);
    assert_eq!(outcome.activated, mods(&["x"]));
    assert_eq!(session.selected(), OrderSource::Current);
}

#[test]
fn an_unanchored_activation_keeps_a_selected_suggested() {
    let mut session = session_fixture_with_inactive(&["a", "b"], &["x"]);
    session.select(OrderSource::Suggested);
    let plan = ActivateMods::plan(&session, &mods(&["x"]), false);

    ActivateMods::execute(&mut session, &plan).expect("x is known");

    assert_eq!(session.selected(), OrderSource::Suggested);
}

#[test]
fn an_anchored_activation_that_adds_nothing_keeps_a_selected_suggested() {
    let mut session = session_fixture_with_inactive(&["a", "b"], &["x"]);
    session.select(OrderSource::Suggested);
    let plan = ActivateMods::plan(&session, &mods(&["a"]), false);
    assert!(plan.to_add.is_empty(), "a is already active");

    let outcome = ActivateMods::execute_at(&mut session, &plan, &Anchor::End).expect("valid drop");

    assert!(!outcome.did_switch_to_current);
    assert_eq!(session.selected(), OrderSource::Suggested);
}

#[test]
fn a_refused_anchored_activation_keeps_a_selected_suggested() {
    let mut session = session_fixture_with_inactive(&["a"], &["x"]);
    session.select(OrderSource::Suggested);
    let plan = ActivateMods::plan(&session, &mods(&["x"]), false);

    let result = ActivateMods::execute_at(&mut session, &plan, &Anchor::After(ModId::new("gone")));

    assert!(result.is_err());
    assert_eq!(session.selected(), OrderSource::Suggested);
}

#[test]
fn a_settled_move_makes_the_order_differ_from_the_file_while_the_diffs_stay_empty() {
    let mut session = session_fixture(&["a", "b", "c"]);
    assert!(!session.pending_changes().order_differs_from_file);

    MoveMods::execute(&mut session, &mods(&["c"]), &Anchor::Start).expect("valid move");
    let moved = session.pending_changes();
    RevertOrderToFile::execute(&mut session).expect("same mods as the file");

    assert!(moved.order_differs_from_file);
    assert!(moved.unscanned.is_empty() && moved.unapplied.is_empty());
    assert!(!session.pending_changes().order_differs_from_file);
}

#[derive(Debug, Clone)]
enum EditOp {
    Move(Vec<usize>, usize, u8),
    Activate(Vec<usize>),
    ActivateAt(Vec<usize>, usize, u8),
    Deactivate(Vec<usize>),
    Discard,
    Revert,
}

const POOL: [&str; 7] = ["a", "b", "c", "d", "e", "x", "y"];

fn anchor_of(index: usize, kind: u8) -> Anchor {
    match kind % 4 {
        0 => Anchor::Start,
        1 => Anchor::End,
        2 => Anchor::Before(ModId::new(POOL[index])),
        _ => Anchor::After(ModId::new(POOL[index])),
    }
}

fn pool_ids(indices: &[usize]) -> Vec<ModId> {
    indices
        .iter()
        .map(|index| ModId::new(POOL[*index]))
        .collect()
}

fn edit_op() -> impl Strategy<Value = EditOp> {
    let ids = proptest::collection::vec(0..POOL.len(), 0..3);
    prop_oneof![
        (ids.clone(), 0..POOL.len(), any::<u8>())
            .prop_map(|(ids, at, kind)| EditOp::Move(ids, at, kind)),
        ids.clone().prop_map(EditOp::Activate),
        (ids.clone(), 0..POOL.len(), any::<u8>())
            .prop_map(|(ids, at, kind)| EditOp::ActivateAt(ids, at, kind)),
        ids.prop_map(EditOp::Deactivate),
        Just(EditOp::Discard),
        Just(EditOp::Revert),
    ]
}

/// Applies one op; a refused one (a stale anchor, an inactive mover, a
/// refused revert) is a legal outcome and leaves the session untouched.
fn apply_op(session: &mut Session, op: &EditOp) {
    match op {
        EditOp::Move(ids, at, kind) => {
            let _ = MoveMods::execute(session, &pool_ids(ids), &anchor_of(*at, *kind));
        }
        EditOp::Activate(ids) => {
            let plan = ActivateMods::plan(session, &pool_ids(ids), true);
            let _ = ActivateMods::execute(session, &plan);
        }
        EditOp::ActivateAt(ids, at, kind) => {
            let plan = ActivateMods::plan(session, &pool_ids(ids), true);
            let _ = ActivateMods::execute_at(session, &plan, &anchor_of(*at, *kind));
        }
        EditOp::Deactivate(ids) => {
            let plan = DeactivateMods::plan(session, &pool_ids(ids));
            DeactivateMods::execute(session, &plan);
        }
        EditOp::Discard => DiscardPendingChanges::execute(session),
        EditOp::Revert => {
            let _ = RevertOrderToFile::execute(session);
        }
    }
}

proptest! {
    #[test]
    fn any_edit_sequence_keeps_working_and_current_settled(
        ops in proptest::collection::vec(edit_op(), 0..12)
    ) {
        let mut session =
            session_fixture_with_inactive(&["a", "b", "c", "d", "e"], &["x", "y"]);
        for op in ops {
            apply_op(&mut session, &op);

            let working = session.working().ids().to_vec();
            let mut unique = working.clone();
            unique.sort();
            unique.dedup();
            prop_assert_eq!(unique.len(), working.len(), "duplicate after {:?}", op);
            let current = session.orders().current.as_slice().to_vec();
            if same_multiset(&working, &current) {
                prop_assert_eq!(&working, &current, "hidden pending change after {:?}", op);
            }
        }
    }
}
