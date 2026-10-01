//! `Report` and `Session` fixture builders.

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{GeneratedMarker, ModId, Report};
use rim_resolve::domain::DecisionSet;

use crate::ports::{ModsConfigFile, StoredRules};
use crate::{ProjectPaths, Session};

/// Builds a minimal [`Report`] with one plain mod per id, in the order
/// given — a convenience for tests that don't need real scan data.
#[must_use]
pub fn report_fixture(ids: &[&str]) -> Report {
    let mut builder = rim_resolve::test_support::ReportBuilder::new();
    for id in ids {
        builder = builder.mod_(id);
    }
    builder.build()
}

/// [`report_fixture`], but also seeding `report.inactive_mods` for each id
/// in `inactive` — a
/// convenience so `ActiveSet`/`ModInventory` tests don't each need their
/// own `ReportBuilder` chain for the common "some active, some inactive"
/// shape. A test that also needs declared dependencies, a missing mod, or
/// a report edge builds on `ReportBuilder` directly instead.
#[must_use]
pub fn report_fixture_with_inactive(active: &[&str], inactive: &[&str]) -> Report {
    let mut builder = rim_resolve::test_support::ReportBuilder::new();
    for id in active {
        builder = builder.mod_(id);
    }
    for id in inactive {
        builder = builder.inactive(id);
    }
    builder.build()
}

/// Builds a [`Session`] over [`report_fixture`]'s mods, with an empty
/// [`SourceIndex`], default rules/decisions, and a `ModsConfig.xml`
/// active list matching `ids` exactly — a convenience for use-case tests
/// that don't care about the details of loading.
#[must_use]
pub fn session_fixture(ids: &[&str]) -> Session {
    Session::new(
        ProjectPaths {
            game_dir: "game".into(),
            workshop_dir: "workshop".into(),
            mods_config: "ModsConfig.xml".into(),
            profile_dir: "profile".into(),
        },
        report_fixture(ids),
        Vec::new(),
        SourceIndex::default(),
        StoredRules::default(),
        DecisionSet::new(),
        ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: ids.iter().map(|id| ModId::new(*id)).collect(),
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    )
}

/// [`session_fixture`], but `generated_id` carries a [`GeneratedMarker`]
/// (as the profile merge mod, or another patch's export, would) —
/// [`crate::Session::reserved_package_ids`]'s marker-based half, for
/// tests proving a generated mod can never be picked as a new patch's
/// scope member or package id.
#[must_use]
pub fn session_fixture_with_generated(ids: &[&str], generated_id: &str) -> Session {
    let mut builder = rim_resolve::test_support::ReportBuilder::new();
    for id in ids {
        if *id == generated_id {
            builder = builder.mod_with(id, |m| {
                m.generated = Some(GeneratedMarker {
                    kind: rim_analyzer::domain::GeneratedKind::Merge,
                    patch_id: None,
                    scope: None,
                });
            });
        } else {
            builder = builder.mod_(id);
        }
    }
    Session::new(
        ProjectPaths {
            game_dir: "game".into(),
            workshop_dir: "workshop".into(),
            mods_config: "ModsConfig.xml".into(),
            profile_dir: "profile".into(),
        },
        builder.build(),
        Vec::new(),
        SourceIndex::default(),
        StoredRules::default(),
        DecisionSet::new(),
        ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: ids.iter().map(|id| ModId::new(*id)).collect(),
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    )
}

/// [`session_fixture_with_generated`], but with a caller-supplied
/// [`SourceIndex`] instead of an empty one — for a test that needs
/// `InspectDef`/`PlanMerge` to see a *real* owner/patcher on a generated
/// mod: the
/// generated marker alone (a bare `Report.mods[*].generated`) has nothing
/// for those use cases to inspect without a matching `SourceIndex` entry
/// too.
#[must_use]
pub fn session_with_sources_and_generated(
    sources: SourceIndex,
    ids: &[&str],
    generated_id: &str,
) -> Session {
    let mut builder = rim_resolve::test_support::ReportBuilder::new();
    for id in ids {
        if *id == generated_id {
            builder = builder.mod_with(id, |m| {
                m.generated = Some(GeneratedMarker {
                    kind: rim_analyzer::domain::GeneratedKind::Merge,
                    patch_id: None,
                    scope: None,
                });
            });
        } else {
            builder = builder.mod_(id);
        }
    }
    Session::new(
        ProjectPaths {
            game_dir: "game".into(),
            workshop_dir: "workshop".into(),
            mods_config: "ModsConfig.xml".into(),
            profile_dir: "profile".into(),
        },
        builder.build(),
        Vec::new(),
        sources,
        StoredRules::default(),
        DecisionSet::new(),
        ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: ids.iter().map(|id| ModId::new(*id)).collect(),
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    )
}

/// Builds a [`Session`] with a real [`Report`] (from [`report_fixture`],
/// mods `ludeon.rimworld` and `example.bionicsfork`, in that order) and the
/// given [`SourceIndex`] — for tests that exercise
/// [`crate::use_cases::PlanMerge`]/[`crate::use_cases::DecideMerge`]
/// against real source data, e.g. [`bionic_heart_fixture`].
#[must_use]
pub fn session_with_sources(sources: SourceIndex, report: Report) -> Session {
    session_with_sources_and_mods(sources, report, &["ludeon.rimworld", "example.bionicsfork"])
}

/// [`session_with_sources`], but for an arbitrary active-mod list (in
/// order) — needed wherever a fixture's owners aren't exactly
/// `ludeon.rimworld`/`example.bionicsfork`, e.g. [`conflicting_wall_fixture`],
/// which needs three.
#[must_use]
pub fn session_with_sources_and_mods(
    sources: SourceIndex,
    report: Report,
    ids: &[&str],
) -> Session {
    Session::new(
        ProjectPaths {
            game_dir: "game".into(),
            workshop_dir: "workshop".into(),
            mods_config: "ModsConfig.xml".into(),
            profile_dir: "profile".into(),
        },
        report,
        Vec::new(),
        sources,
        StoredRules::default(),
        DecisionSet::new(),
        ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: ids.iter().map(|id| ModId::new(*id)).collect(),
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    )
}
