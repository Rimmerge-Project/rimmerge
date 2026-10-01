//! [`ActivateMods`]: plans and commits an activation onto a session's
//! working active-mod set. Pure: no
//! port, no persistence — only [`crate::use_cases::Rescan`] and
//! [`crate::use_cases::Apply`] touch disk.

use rim_analyzer::domain::ModId;

use crate::Session;
use crate::active_set::{ActivatePlan, ActiveSetError, plan_activate};
use crate::mod_inventory::ModInventory;

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

    /// Commits `plan.to_add` onto `session`'s working set. Validates every
    /// id against the current report before mutating anything (via
    /// [`crate::ActiveSet::activate`]), so an id that has gone unknown
    /// since [`Self::plan`] built this plan (a stale plan reused after a
    /// rescan, say) leaves the working set untouched rather than partially
    /// applied.
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
        session
            .working_mut()
            .activate(plan.to_add.clone(), &inventory)
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
