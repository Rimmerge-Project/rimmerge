//! [`ActivateMods`]: plans and commits an activation onto a session's
//! working active-mod set. Pure: no
//! port, no persistence — only [`crate::use_cases::Rescan`] and
//! [`crate::use_cases::Apply`] touch disk.

use rim_analyzer::domain::ModId;

use crate::Session;
use crate::active_set::{ActivatePlan, ActiveSetError, Anchor, MoveError, plan_activate};
use crate::mod_inventory::ModInventory;

/// What a successful [`ActivateMods::execute_at`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivateOutcome {
    /// The ids inserted, dependencies first.
    pub activated: Vec<ModId>,
    /// The drop changed the working sequence while Suggested was selected,
    /// so the selection is now Current, exactly as for a
    /// [`crate::use_cases::MoveMods`]: an explicit drop position is the
    /// user's own order. The caller tells the user.
    pub did_switch_to_current: bool,
}

/// Plans and commits activating one or more mods onto
/// [`Session::working`].
pub struct ActivateMods;

impl ActivateMods {
    /// Plans what activating `ids` would do: the closure of their own
    /// inactive dependencies when `with_dependencies` is set (dependencies
    /// first), unresolvable dependencies, and which requested ids are
    /// already active. Computes nothing new against disk — `session.report()`
    /// is already in memory.
    #[must_use]
    pub fn plan(session: &Session, ids: &[ModId], with_dependencies: bool) -> ActivatePlan {
        let inventory = ModInventory::from_report(session.report());
        plan_activate(session.working(), &inventory, ids, with_dependencies)
    }

    /// Commits `plan.to_add` onto `session`'s working set with no anchor: a
    /// mod that has a slot in Current goes back to it (right after its
    /// nearest Current predecessor that is active, or at the start), any
    /// other mod goes to the end. Deactivating a mod and activating it again
    /// therefore leaves Current as it was. Validates every id against the
    /// current report before mutating anything, so an id that has gone
    /// unknown since [`Self::plan`] built this plan (a stale plan reused
    /// after a rescan, say) leaves the working set untouched rather than
    /// partially applied. Then settles Current onto `working` when their
    /// ids are equal (`Session::settle_current_onto_working`).
    ///
    /// # Errors
    ///
    /// Returns [`ActiveSetError::Unknown`] when `plan.to_add` names an id
    /// the current report's inventory doesn't recognize.
    pub fn execute(
        session: &mut Session,
        plan: &ActivatePlan,
    ) -> Result<Vec<ModId>, ActiveSetError> {
        let inventory = ModInventory::from_report(session.report());
        let reference = session.orders().current.as_slice().to_vec();
        let activated = session.working_mut().activate_at_reference_slots(
            plan.to_add.clone(),
            &reference,
            &inventory,
        )?;
        session.settle_current_onto_working();
        Ok(activated)
    }

    /// [`Self::execute`] with an anchor: `plan.to_add` goes in as one block
    /// (dependencies first) at `anchor`. Validated before anything changes.
    /// Unlike [`Self::execute`], an explicit position also switches a
    /// selected Suggested to Current when the working sequence changed (see
    /// [`ActivateOutcome::did_switch_to_current`]).
    ///
    /// # Errors
    ///
    /// [`MoveError::Unknown`] for an id the report does not know,
    /// [`MoveError::AnchorNotActive`] for a stale anchor.
    pub fn execute_at(
        session: &mut Session,
        plan: &ActivatePlan,
        anchor: &Anchor,
    ) -> Result<ActivateOutcome, MoveError> {
        let inventory = ModInventory::from_report(session.report());
        let activated =
            session
                .working_mut()
                .activate_at(plan.to_add.clone(), anchor, &inventory)?;
        session.settle_current_onto_working();
        let did_switch_to_current =
            !activated.is_empty() && session.switch_to_current_after_reorder();
        Ok(ActivateOutcome {
            activated,
            did_switch_to_current,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use crate::ports::ModsConfigFile;
    use crate::test_support::session_fixture;

    /// A [`Session`] over `active` (all active) plus `inactive` (all
    /// inactive) — [`session_fixture`]'s own shape, but with an inactive
    /// mod list this use case needs to exercise `ActivateMods` against.
    fn session_with(active: &[&str], inactive: &[&str]) -> Session {
        let report = {
            let mut builder = rim_resolve::test_support::ReportBuilder::new();
            for id in active {
                builder = builder.mod_(id);
            }
            for id in inactive {
                builder = builder.inactive(id);
            }
            builder.build()
        };
        Session::new(
            crate::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: active.iter().map(|id| ModId::new(*id)).collect(),
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn plan_and_execute_add_the_requested_mods() {
        let mut session = session_with(&["a"], &["b"]);

        let plan = ActivateMods::plan(&session, &[ModId::new("b")], false);
        assert_eq!(plan.to_add, [ModId::new("b")]);

        let added = ActivateMods::execute(&mut session, &plan).expect("b is known");

        assert_eq!(added, [ModId::new("b")]);
        assert!(session.working().contains(&ModId::new("b")));
        assert_eq!(
            session.orders().current.as_slice(),
            [ModId::new("a")],
            "activating never touches orders.current"
        );
    }

    #[test]
    fn execute_rejects_an_unknown_id_and_leaves_working_untouched() {
        let mut session = session_fixture(&["a"]);
        let before = session.working().clone();

        let plan = ActivatePlan {
            to_add: vec![ModId::new("ghost")],
            ..ActivatePlan::default()
        };

        let error = ActivateMods::execute(&mut session, &plan).expect_err("ghost is unknown");

        assert_eq!(error, ActiveSetError::Unknown(ModId::new("ghost")));
        assert_eq!(session.working(), &before);
    }
}
