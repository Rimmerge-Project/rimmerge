use rim_analyzer::domain::{MissingDependency, ModCost, ModId};
use rim_resolve::domain::{DecisionSet, OrderSource};

use crate::mod_info::{HomepageLink, ModInfo, PendingChange};
use crate::ports::{ModsConfigFile, StoredRules};
use crate::test_support::{report_fixture_with_inactive, session_fixture};
use crate::use_cases::ActivateMods;
use crate::{ProjectPaths, Session};

fn session_over(report: rim_analyzer::domain::Report, active_mods: Vec<ModId>) -> Session {
    Session::new(
        ProjectPaths {
            game_dir: "game".into(),
            workshop_dir: "workshop".into(),
            mods_config: "ModsConfig.xml".into(),
            profile_dir: "profile".into(),
        },
        report,
        Vec::new(),
        rim_analyzer::analysis::SourceIndex::default(),
        StoredRules::default(),
        DecisionSet::new(),
        ModsConfigFile {
            version: "1.6".to_string(),
            active_mods,
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    )
}

#[test]
fn an_active_mod_resolves_to_active_with_position_tier_and_cost() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .with_mod_costs(vec![ModCost {
            mod_id: ModId::new("a"),
            patch_ops: 3,
            slow_xpath_ops: 0,
            texture_files: 0,
            texture_bytes: 0,
            dds_files: 0,
            assembly_count: 0,
            assembly_bytes: 0,
            def_count: 5,
            content_only: false,
            overridden_texture_bytes: 0,
            nameless_def_count: 0,
        }])
        .build();
    let mut session = session_over(report, vec![ModId::new("a"), ModId::new("b")]);

    let info = session
        .mod_info(&ModId::new("a"), OrderSource::Current)
        .expect("a is active");

    let ModInfo::Active(active) = info else {
        panic!("expected Active, got {info:?}");
    };
    assert_eq!(active.mod_id, ModId::new("a"));
    assert_eq!(active.position, 0);
    assert_eq!(
        active.cost.as_ref().map(|c| c.def_count),
        Some(5),
        "cost must come from report.mod_costs"
    );
}

#[test]
fn a_steam_suffixed_active_mod_resolves_by_its_base_id() {
    let mut session = session_fixture(&["a_steam", "b"]);

    let info = session
        .mod_info(&ModId::new("a"), OrderSource::Current)
        .expect("a_steam must resolve by its base id");

    let ModInfo::Active(active) = info else {
        panic!("expected Active");
    };
    assert_eq!(active.mod_id, ModId::new("a_steam"));
}

#[test]
fn an_inactive_mod_resolves_to_inactive() {
    let mut session = session_over(
        report_fixture_with_inactive(&["a"], &["inactive.mod"]),
        vec![ModId::new("a")],
    );

    let info = session
        .mod_info(&ModId::new("inactive.mod"), OrderSource::Current)
        .expect("must resolve");

    assert!(matches!(info, ModInfo::Inactive(_)));
}

#[test]
fn a_missing_mod_resolves_to_missing_with_required_by() {
    let mut report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a")
        .missing_mod("ghost.mod")
        .dependency("a", "ghost.mod")
        .build();
    let declaring_id = ModId::new("a");
    let missing_id = ModId::new("ghost.mod");
    // `dependency()` only records the declared dependency for
    // round-tripping through `DeclaredOrder`; `missing_dependencies`
    // (what `required_by` reads) is a separate, analyzer-computed report
    // field this builder doesn't derive automatically — set it directly.
    let declared = report
        .mods
        .iter()
        .find(|m| m.id == declaring_id)
        .expect("mod a exists")
        .declared
        .dependencies
        .first()
        .cloned()
        .expect("dependency() recorded it");
    report.missing_dependencies.push(MissingDependency {
        mod_id: declaring_id.clone(),
        dependency: declared,
    });
    let mut session = session_over(report, vec![declaring_id.clone(), missing_id.clone()]);

    let info = session
        .mod_info(&missing_id, OrderSource::Current)
        .expect("must resolve");

    let ModInfo::Missing(missing) = info else {
        panic!("expected Missing");
    };
    assert_eq!(missing.mod_id, missing_id);
    assert!(missing.required_by.contains(&declaring_id));
}

#[test]
fn an_unknown_id_is_an_error() {
    let mut session = session_fixture(&["a"]);

    let result = session.mod_info(&ModId::new("nobody"), OrderSource::Current);

    assert!(result.is_err());
}

#[test]
fn a_pending_activation_shows_up_on_an_inactive_mod() {
    let mut session = session_over(
        report_fixture_with_inactive(&["a"], &["inactive.mod"]),
        vec![ModId::new("a")],
    );
    let plan = ActivateMods::plan(&session, &[ModId::new("inactive.mod")], false);
    ActivateMods::execute(&mut session, &plan).expect("activation must succeed");

    let info = session
        .mod_info(&ModId::new("inactive.mod"), OrderSource::Current)
        .expect("must resolve");

    let ModInfo::Inactive(inactive) = info else {
        panic!("expected Inactive");
    };
    assert_eq!(inactive.pending, Some(PendingChange::ActivationPending));
}

#[test]
fn homepage_url_is_classified_openable_for_https() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_with("a", |m| m.url = Some("https://example.com".to_string()))
        .build();
    let mut session = session_over(report, vec![ModId::new("a")]);

    let info = session
        .mod_info(&ModId::new("a"), OrderSource::Current)
        .expect("must resolve");

    let ModInfo::Active(active) = info else {
        panic!("expected Active");
    };
    assert!(matches!(active.homepage, Some(HomepageLink::Openable(_))));
}

#[test]
fn findings_total_reflects_a_live_finding_naming_the_mod() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .def_override("ThingDef", "Foo", &["a", "b"])
        .build();
    let mut session = session_over(report, vec![ModId::new("a"), ModId::new("b")]);

    let info = session
        .mod_info(&ModId::new("b"), OrderSource::Current)
        .expect("must resolve");

    let ModInfo::Active(active) = info else {
        panic!("expected Active");
    };
    assert!(
        active.findings_total >= 1,
        "the DefOverride conflict must surface as a live finding naming b"
    );
}
