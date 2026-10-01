//! [`DeactivateMods`]: plans and commits a deactivation onto a session's
//! working active-mod set. Pure, the
//! same shape as [`crate::use_cases::ActivateMods`]; `deactivate` itself
//! is infallible (`ActiveSetError` has no case a plan-then-commit pair can
//! reach on removal — an unknown id is simply not in the set to remove),
//! so unlike `ActivateMods::execute` this returns no `Result`.

use rim_analyzer::domain::ModId;

use crate::Session;
use crate::active_set::{DeactivatePlan, plan_deactivate};
use crate::mod_inventory::ModInventory;

/// Plans and commits deactivating one or more mods from
/// [`Session::working`].
pub struct DeactivateMods;

impl DeactivateMods {
    /// Plans what deactivating `ids` would do: which would actually be
    /// removed (Core refused), and which other currently-active mods
    /// still declare a dependency on — or carry a `Hard` edge to — each
    /// removed one (a warning, never a block).
    #[must_use]
    pub fn plan(session: &Session, ids: &[ModId]) -> DeactivatePlan {
        let inventory = ModInventory::from_report(session.report());
        plan_deactivate(session.working(), &inventory, ids)
    }

    /// Commits `plan.to_remove` onto `session`'s working set. Returns
    /// what was actually removed.
    pub fn execute(session: &mut Session, plan: &DeactivatePlan) -> Vec<ModId> {
        session.working_mut().deactivate(&plan.to_remove)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::session_fixture;

    #[test]
    fn plan_and_execute_remove_the_requested_mod() {
        let mut session = session_fixture(&["a", "b"]);

        let plan = DeactivateMods::plan(&session, &[ModId::new("b")]);
        assert_eq!(plan.to_remove, [ModId::new("b")]);
        assert!(plan.dependents_still_active.is_empty());
        assert!(plan.refused.is_empty());

        let removed = DeactivateMods::execute(&mut session, &plan);

        assert_eq!(removed, [ModId::new("b")]);
        assert!(!session.working().contains(&ModId::new("b")));
        assert_eq!(
            session.orders().current.as_slice(),
            [ModId::new("a"), ModId::new("b")],
            "deactivating never touches orders.current"
        );
    }

    #[test]
    fn plan_refuses_core_and_reports_dependents() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("ludeon.rimworld")
            .mod_("target.mod")
            .mod_("dependent.mod")
            .dependency("dependent.mod", "target.mod")
            .build();
        let session = crate::Session::new(
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
            crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![
                    ModId::new("ludeon.rimworld"),
                    ModId::new("target.mod"),
                    ModId::new("dependent.mod"),
                ],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );

        let plan = DeactivateMods::plan(
            &session,
            &[ModId::new("ludeon.rimworld"), ModId::new("target.mod")],
        );

        assert_eq!(plan.refused, [ModId::new("ludeon.rimworld")]);
        assert_eq!(plan.to_remove, [ModId::new("target.mod")]);
        assert_eq!(
            plan.dependents_still_active.get(&ModId::new("target.mod")),
            Some(&vec![ModId::new("dependent.mod")])
        );
    }

    #[test]
    fn a_missing_mod_is_removable() {
        // `missing.mod` is active (`ModsConfig.xml` named it) but was
        // never found on disk — deactivating it must still work.
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("a")
            .missing_mod("missing.mod")
            .build();
        let session = crate::Session::new(
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
            crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a"), ModId::new("missing.mod")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let mut session = session;

        let plan = DeactivateMods::plan(&session, &[ModId::new("missing.mod")]);
        assert_eq!(plan.to_remove, [ModId::new("missing.mod")]);

        let removed = DeactivateMods::execute(&mut session, &plan);

        assert_eq!(removed, [ModId::new("missing.mod")]);
        assert!(!session.working().contains(&ModId::new("missing.mod")));
    }
}
