//! Tests for [`Session`].

use rim_resolve::domain::{Action, Placement, PlacementRule, RuleOrigin};

use super::*;
use crate::finding_index::FindingFilter;
use crate::game_launch::{OrderOnDisk, UnappliedReason};
use crate::merge_workspace::MergePreview;
use crate::test_support::report_fixture;
use crate::use_cases::MergeContext;
use rim_resolve::domain::{Decision, FindingKey, PairRule, PromotedRuleKey};

fn mods_config(ids: &[&str]) -> ModsConfigFile {
    ModsConfigFile {
        version: "1.6.4871 rev590".to_string(),
        active_mods: ids.iter().map(|id| ModId::new(*id)).collect(),
        known_expansions: vec![ModId::new("ludeon.rimworld.royalty")],
    }
}

fn session(ids: &[&str]) -> Session {
    let report = report_fixture(ids);
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
        mods_config(ids),
        Vec::new(),
        Vec::new(),
    )
}

#[test]
fn new_session_builds_both_orders_and_defaults_to_current() {
    let session = session(&["a", "b", "c"]);
    assert_eq!(session.selected(), OrderSource::Current);
    assert_eq!(
        session.orders().current.as_slice(),
        [ModId::new("a"), ModId::new("b"), ModId::new("c")]
    );
}

#[test]
fn mods_config_file_for_preserves_version_and_known_expansions() {
    let session = session(&["a", "b"]);
    let file = session.mods_config_file_for(&session.orders().suggested);
    assert_eq!(file.version, "1.6.4871 rev590");
    assert_eq!(
        file.known_expansions,
        vec![ModId::new("ludeon.rimworld.royalty")]
    );
}

#[test]
fn decide_with_accept_does_not_change_the_suggested_order() {
    let mut session = session(&["a", "b"]);
    let before = session.orders().suggested.as_slice().to_vec();

    let resorted = session
        .decide(Decision {
            key: FindingKey::UnsupportedVersion {
                mod_id: ModId::new("a"),
            },
            action: Action::Accept,
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("accept is always a valid action");

    assert!(!resorted);
    assert_eq!(session.orders().suggested.as_slice(), before.as_slice());
    assert_eq!(
        session
            .decisions()
            .get(&FindingKey::UnsupportedVersion {
                mod_id: ModId::new("a"),
            })
            .map(|d| &d.action),
        Some(&Action::Accept)
    );
}

#[test]
fn decide_with_reorder_resorts_and_persists_the_decision() {
    let mut session = session(&["a", "b"]);

    let resorted = session
        .decide(Decision {
            key: FindingKey::UndeclaredHardDependency {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
            action: Action::Reorder {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("reorder is always a valid action");

    assert!(resorted);
    // "a" must load after "b" now that the Reorder decision is a
    // sorter override.
    let order = session.orders().suggested.as_slice();
    let a_pos = order
        .iter()
        .position(|id| *id == ModId::new("a"))
        .unwrap_or_else(|| unreachable!());
    let b_pos = order
        .iter()
        .position(|id| *id == ModId::new("b"))
        .unwrap_or_else(|| unreachable!());
    assert!(b_pos < a_pos);
}

/// The "Promote" alternative: a `PromoteRule` decision must actually call
/// `Session::promote_imported_rule`, not just get recorded inertly —
/// `DecisionSet::sorter_overrides` has no entry for this action (see
/// its own doc comment), so the generic before/after comparison in
/// `decide` would otherwise never notice the rule set changed.
#[test]
fn decide_with_promote_rule_actually_promotes_the_imported_rule() {
    let mut session = session(&["a", "b"]);
    session.upsert_rule(Rule::Pair(PairRule {
        after: ModId::new("a"),
        before: ModId::new("b"),
        origin: RuleOrigin::RimSortCommunity,
        comment: None,
        overrides_declared: false,
    }));

    let resorted = session
        .decide(Decision {
            key: FindingKey::RuleOverruled {
                after: ModId::new("a"),
                before: ModId::new("b"),
                origin: RuleOrigin::RimSortCommunity,
            },
            action: Action::PromoteRule {
                rule: PromotedRuleKey::Pair {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("promote is always a valid action");

    assert!(resorted, "a genuine promotion must report true");
    let pairs = &session.rules().pairs;
    assert_eq!(
        pairs.len(),
        2,
        "the imported rule and its promoted copy must both be listed: {pairs:?}"
    );
    assert!(pairs.iter().any(|r| r.origin == RuleOrigin::UserDecision));
}

/// A `PromoteRule` decision that promotes nothing (nothing imported at
/// that key) must still record the decision and report `false`,
/// rather than erroring.
#[test]
fn decide_with_promote_rule_on_a_no_op_key_still_records_the_decision() {
    let mut session = session(&["a", "b"]);
    let key = FindingKey::RuleOverruled {
        after: ModId::new("a"),
        before: ModId::new("b"),
        origin: RuleOrigin::RimSortCommunity,
    };

    let resorted = session
        .decide(Decision {
            key: key.clone(),
            action: Action::PromoteRule {
                rule: PromotedRuleKey::Pair {
                    after: ModId::new("a"),
                    before: ModId::new("b"),
                },
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("promote is always a valid action");

    assert!(!resorted);
    assert!(session.decisions().get(&key).is_some());
}

#[test]
fn revert_decision_removes_it_and_resorts_when_it_was_a_sorter_override() {
    let mut session = session(&["a", "b"]);
    session
        .decide(Decision {
            key: FindingKey::UndeclaredHardDependency {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
            action: Action::Reorder {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("reorder is always a valid action");

    let removed = session.revert_decision(&FindingKey::UndeclaredHardDependency {
        after: ModId::new("a"),
        before: ModId::new("b"),
    });

    assert!(removed.is_some());
    assert!(
        session
            .decisions()
            .get(&FindingKey::UndeclaredHardDependency {
                after: ModId::new("a"),
                before: ModId::new("b"),
            })
            .is_none()
    );
}

#[test]
fn upsert_rule_replaces_an_existing_rule_with_the_same_key() {
    let mut session = session(&["a", "b"]);
    session.upsert_rule(Rule::Placement(PlacementRule {
        mod_id: ModId::new("a"),
        placement: Placement::Top,
        origin: RuleOrigin::UserDecision,
        comment: None,
    }));
    session.upsert_rule(Rule::Placement(PlacementRule {
        mod_id: ModId::new("a"),
        placement: Placement::Bottom,
        origin: RuleOrigin::UserDecision,
        comment: None,
    }));

    assert_eq!(session.rules().placements.len(), 1);
    assert_eq!(session.rules().placements[0].placement, Placement::Bottom);
}

/// Upserting a promoted
/// (`UserDecision`-origin) pair rule's own comment must never also
/// remove the imported original sharing its key.
#[test]
fn upsert_rule_of_a_promoted_copy_leaves_the_import_intact() {
    let mut session = session(&["a", "b"]);
    session.upsert_rule(Rule::Pair(PairRule {
        after: ModId::new("a"),
        before: ModId::new("b"),
        origin: RuleOrigin::RimSortCommunity,
        comment: None,
        overrides_declared: false,
    }));
    let key = RuleKey::Pair {
        after: ModId::new("a"),
        before: ModId::new("b"),
    };
    assert!(session.promote_imported_rule(&key));

    session.upsert_rule(Rule::Pair(PairRule {
        after: ModId::new("a"),
        before: ModId::new("b"),
        origin: RuleOrigin::UserDecision,
        comment: Some("edited".to_string()),
        overrides_declared: false,
    }));

    let pairs = &session.rules().pairs;
    assert_eq!(
        pairs.len(),
        2,
        "the imported original must survive the promoted copy's own upsert: {pairs:?}"
    );
    assert!(
        pairs
            .iter()
            .any(|r| r.origin == RuleOrigin::RimSortCommunity)
    );
    assert!(pairs.iter().any(
            |r| r.origin == RuleOrigin::UserDecision && r.comment.as_deref() == Some("edited")
        ));
}

#[test]
fn delete_rule_removes_the_matching_rule() {
    let mut session = session(&["a", "b"]);
    session.upsert_rule(Rule::Placement(PlacementRule {
        mod_id: ModId::new("a"),
        placement: Placement::Top,
        origin: RuleOrigin::UserDecision,
        comment: None,
    }));

    session.delete_rule(
        &RuleKey::Placement {
            mod_id: ModId::new("a"),
        },
        None,
    );

    assert!(session.rules().placements.is_empty());
}

/// **Written failing-first**: `userRules.json` is read live off a RimSort
/// install rather than copied into Rimmerge's own storage, so a
/// refresh-then-import-from-cache run (`rimmerge db refresh &&
/// rimmerge import --from-cache`, the most natural command pair)
/// legitimately has no user-rules path at all — and an `apply_import`
/// that cleared *every* imported-origin rule regardless of which sources
/// this import actually carried would delete the user's own hand-added
/// `RimSortUser` rules at exactly that moment. A future reader who
/// sees `ImportedRules.user_rules: Option<Vec<Rule>>` and thinks
/// `None`/`Some(vec![])` "look the same enough to collapse" needs to
/// find this reason here.
#[test]
fn apply_import_with_no_user_rules_path_leaves_existing_rimsort_user_rules_intact() {
    let mut session = session(&["a", "b"]);
    session.apply_import(crate::ports::ImportedRules {
        user_rules: Some(vec![Rule::Pair(PairRule {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::RimSortUser,
            comment: None,
            overrides_declared: false,
        })]),
        community_rules: Some(Vec::new()),
        steam_dependencies: Some(Vec::new()),
        skipped_inactive_rules: 0,
        skipped_inactive_steam: 0,
        provenance: std::collections::BTreeMap::new(),
    });
    assert_eq!(session.rules().pairs.len(), 1);

    // Only the community database was part of this import —
    // `userRules.json` legitimately wasn't (no RimSort install, or
    // `--from-cache` with no `--user-rules`).
    session.apply_import(crate::ports::ImportedRules {
        user_rules: None,
        community_rules: Some(vec![Rule::Placement(PlacementRule {
            mod_id: ModId::new("b"),
            placement: Placement::Top,
            origin: RuleOrigin::RimSortCommunity,
            comment: None,
        })]),
        steam_dependencies: None,
        skipped_inactive_rules: 0,
        skipped_inactive_steam: 0,
        provenance: std::collections::BTreeMap::new(),
    });

    assert_eq!(
        session.rules().pairs.len(),
        1,
        "user_rules: None must leave the existing RimSortUser pair rule untouched"
    );
    assert_eq!(session.rules().pairs[0].origin, RuleOrigin::RimSortUser);
    assert_eq!(
        session.rules().placements.len(),
        1,
        "community_rules: Some(_) must still replace the RimSortCommunity origin"
    );
}

#[test]
fn apply_import_replaces_previously_imported_rules_on_reimport() {
    let mut session = session(&["a", "b"]);
    session.apply_import(crate::ports::ImportedRules {
        user_rules: Some(vec![Rule::Pair(rim_resolve::domain::PairRule {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::RimSortUser,
            comment: None,
            overrides_declared: false,
        })]),
        community_rules: Some(Vec::new()),
        steam_dependencies: Some(Vec::new()),
        skipped_inactive_rules: 0,
        skipped_inactive_steam: 0,
        provenance: std::collections::BTreeMap::new(),
    });
    assert_eq!(session.rules().pairs.len(), 1);

    // Re-importing with an explicit, empty `Some` result for the same
    // origin must clear the earlier imported pair, not accumulate
    // alongside it — an import that legitimately found nothing is a
    // real outcome, distinct from an import that didn't run at all
    // (see `imported_rules_default_means_nothing_was_imported`,
    // below, for the `None`-everywhere case).
    session.apply_import(crate::ports::ImportedRules {
        user_rules: Some(Vec::new()),
        community_rules: Some(Vec::new()),
        steam_dependencies: Some(Vec::new()),
        skipped_inactive_rules: 0,
        skipped_inactive_steam: 0,
        provenance: std::collections::BTreeMap::new(),
    });

    assert!(session.rules().pairs.is_empty());
}

/// `ImportedRules::default()` (every field `None`) means "nothing was
/// imported this call" — a no-op on every existing rule, never "clear
/// everything imported."
#[test]
fn imported_rules_default_means_nothing_was_imported() {
    let mut session = session(&["a", "b"]);
    session.apply_import(crate::ports::ImportedRules {
        user_rules: Some(vec![Rule::Pair(PairRule {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::RimSortUser,
            comment: None,
            overrides_declared: false,
        })]),
        community_rules: Some(Vec::new()),
        steam_dependencies: Some(Vec::new()),
        skipped_inactive_rules: 0,
        skipped_inactive_steam: 0,
        provenance: std::collections::BTreeMap::new(),
    });

    session.apply_import(crate::ports::ImportedRules::default());

    assert_eq!(
        session.rules().pairs.len(),
        1,
        "ImportedRules::default() must leave every existing imported rule untouched"
    );
}

/// The `debug_assert!` documented on `replace_origin_rules` itself:
/// a rule whose own `origin` doesn't match the arm it's passed under
/// trips it in a debug build (every test in this workspace runs
/// under one). No real caller can produce this today — see that
/// method's own doc comment for why a `debug_assert!` was judged
/// worth adding anyway.
#[test]
#[should_panic(expected = "must tag every rule in this arm's own Vec")]
fn apply_import_debug_asserts_a_rule_tagged_with_the_wrong_origin() {
    let mut session = session(&["a", "b"]);
    session.apply_import(crate::ports::ImportedRules {
        user_rules: Some(vec![Rule::Pair(PairRule {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::RimSortCommunity,
            comment: None,
            overrides_declared: false,
        })]),
        community_rules: None,
        steam_dependencies: None,
        skipped_inactive_rules: 0,
        skipped_inactive_steam: 0,
        provenance: std::collections::BTreeMap::new(),
    });
}

/// A promoted rule must
/// survive a re-import that would otherwise wipe every imported-origin
/// rule — `promote_imported_rule`'s copy is `UserDecision`-origin, so
/// `apply_import`'s per-origin retain-then-replace never touches it.
#[test]
fn a_promoted_pair_survives_a_reimport_that_clears_the_original() {
    let mut session = session(&["a", "b"]);
    session.apply_import(crate::ports::ImportedRules {
        user_rules: Some(vec![Rule::Pair(rim_resolve::domain::PairRule {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::RimSortUser,
            comment: None,
            overrides_declared: false,
        })]),
        community_rules: Some(Vec::new()),
        steam_dependencies: Some(Vec::new()),
        skipped_inactive_rules: 0,
        skipped_inactive_steam: 0,
        provenance: std::collections::BTreeMap::new(),
    });
    let promoted = session.promote_imported_rule(&RuleKey::Pair {
        after: ModId::new("a"),
        before: ModId::new("b"),
    });
    assert!(promoted);
    assert_eq!(session.rules().pairs.len(), 2);

    // A re-import that explicitly re-imports `RimSortUser` with
    // nothing this time clears the original but must leave the
    // promoted copy alone.
    session.apply_import(crate::ports::ImportedRules {
        user_rules: Some(Vec::new()),
        community_rules: Some(Vec::new()),
        steam_dependencies: Some(Vec::new()),
        skipped_inactive_rules: 0,
        skipped_inactive_steam: 0,
        provenance: std::collections::BTreeMap::new(),
    });

    assert_eq!(
        session.rules().pairs.len(),
        1,
        "the promoted UserDecision copy must survive the reimport"
    );
    assert_eq!(session.rules().pairs[0].origin, RuleOrigin::UserDecision);
}

#[test]
fn active_base_ids_strips_the_steam_suffix() {
    let session = session(&["a.mod_steam", "b"]);
    assert!(session.active_base_ids().contains(&ModId::new("a.mod")));
}

/// A test that would still pass even if `ensure_ledger` rebuilt on
/// every call is not actually testing caching. This one mutates the
/// threshold *without* going through the public API (which would
/// correctly invalidate the cache) — only a genuinely cached ledger
/// survives that unseen.
#[test]
fn ledger_is_cached_across_calls_for_the_same_source() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a")
        .missing_mod("gone")
        .build();
    let mut session = Session::new(
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
            active_mods: vec![ModId::new("a"), ModId::new("gone")],
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    );

    // `MissingMod`'s suggested confidence is 85 (see the confidence
    // table), so it's `Auto` at the default threshold of 80.
    let first_auto = session.ledger(OrderSource::Current).stats.auto;
    assert_eq!(
        first_auto, 1,
        "sanity check: MissingMod must be Auto at the default threshold"
    );

    // Bypass `update_settings` (which would correctly invalidate the
    // cache) to raise the threshold above 85 directly on the private
    // field — if `ensure_ledger` rebuilt instead of reusing the
    // cached ledger, the next call would reclassify the entry as
    // NeedsInput and this would fail.
    session.rules.settings.threshold =
        rim_resolve::domain::Confidence::new(90).unwrap_or_else(|_| unreachable!());

    let second_auto = session.ledger(OrderSource::Current).stats.auto;
    assert_eq!(
        second_auto, 1,
        "the cached ledger must be reused, not rebuilt from the mutated threshold"
    );
}

#[test]
fn findings_pages_the_current_ledger_and_respects_the_limit() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a")
        .missing_mod("gone1")
        .missing_mod("gone2")
        .build();
    let mut session = Session::new(
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
            active_mods: vec![ModId::new("a"), ModId::new("gone1"), ModId::new("gone2")],
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    );

    let page = session.findings(
        OrderSource::Current,
        &FindingFilter {
            limit: 1,
            ..FindingFilter::default()
        },
    );

    assert_eq!(page.items.len(), 1, "limit must actually cap the page");
    assert_eq!(
        page.total, 2,
        "total must reflect every matching finding, not just the page"
    );
}

#[test]
fn replacing_a_reorder_with_accept_on_the_same_key_restores_the_unforced_order() {
    let mut session = session(&["a", "b"]);
    let key = FindingKey::UndeclaredHardDependency {
        after: ModId::new("a"),
        before: ModId::new("b"),
    };
    session
        .decide(Decision {
            key: key.clone(),
            action: Action::Reorder {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("reorder is always a valid action");
    assert_ne!(
        session.orders().suggested.as_slice(),
        [ModId::new("a"), ModId::new("b")],
        "sanity check: the reorder must have actually moved something"
    );

    let resorted = session
        .decide(Decision {
            key,
            action: Action::Accept,
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("accept is always a valid action");

    assert!(
        resorted,
        "replacing a sorter-affecting decision with a non-affecting one on the same key must still resort"
    );
    assert_eq!(
        session.orders().suggested.as_slice(),
        [ModId::new("a"), ModId::new("b")],
        "the unforced order must be restored once the Reorder override is gone"
    );
}

#[test]
fn reverting_a_missing_mod_ignore_re_sorts() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a")
        .missing_mod("gone")
        .build();
    let mut session = Session::new(
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
            active_mods: vec![ModId::new("a"), ModId::new("gone")],
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    );
    assert!(
        !session
            .orders()
            .suggested
            .as_slice()
            .contains(&ModId::new("gone")),
        "sanity check: a missing mod is omitted by default"
    );

    let key = FindingKey::MissingMod {
        mod_id: ModId::new("gone"),
    };
    session
        .decide(Decision {
            key: key.clone(),
            action: Action::Ignore,
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("ignore is always a valid action");
    assert!(
        session
            .orders()
            .suggested
            .as_slice()
            .contains(&ModId::new("gone")),
        "Ignore on MissingMod must reinsert it"
    );

    let removed = session.revert_decision(&key);

    assert!(removed.is_some());
    assert!(
        !session
            .orders()
            .suggested
            .as_slice()
            .contains(&ModId::new("gone")),
        "reverting the Ignore must re-sort and drop the missing mod again"
    );
}

/// [`crate::Settings::enforce`] must reach [`SortInput::enforce`], and
/// toggling it must re-sort: an `Awareness`-strength edge is advisory
/// (ignored) by default, but becomes a real ordering constraint once
/// `enforce.awareness` is turned on.
#[test]
fn toggling_enforce_awareness_reaches_the_sorter_and_re_sorts() {
    use rim_resolve::sort::EnforcedLayers;

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .awareness_edge("a", "b") // "a" must load after "b".
        .build();
    let mut session = Session::new(
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
            active_mods: vec![ModId::new("a"), ModId::new("b")],
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    );

    // Insertion order [a, b] violates "a after b" — advisory by
    // default, so the sorter leaves it alone.
    assert_eq!(
        session.orders().suggested.as_slice(),
        [ModId::new("a"), ModId::new("b")]
    );

    session.update_settings(Settings {
        threshold: session.settings().threshold,
        enforce: EnforcedLayers {
            soft: false,
            awareness: true,
            inferred: true,
        },
        ..session.settings()
    });

    assert_eq!(
        session.orders().suggested.as_slice(),
        [ModId::new("b"), ModId::new("a")],
        "enforcing awareness must re-sort so b loads before a"
    );
}

/// `Settings.use_imported_pairs`
/// must reach the sorter and re-sort. An imported pair rule ("a after
/// b") is inert while the toggle is off and takes effect once it's
/// turned on — the toggle itself defaults *on*, so this test sets it off
/// explicitly first rather than relying on `Settings::default`.
#[test]
fn toggling_use_imported_pairs_reaches_the_sorter_and_re_sorts() {
    let mut session = session(&["a", "b"]);
    session.upsert_rule(Rule::Pair(PairRule {
        after: ModId::new("a"),
        before: ModId::new("b"),
        origin: RuleOrigin::RimSortCommunity,
        comment: None,
        overrides_declared: false,
    }));
    session.update_settings(Settings {
        use_imported_pairs: false,
        ..session.settings()
    });

    assert_eq!(
        session.orders().suggested.as_slice(),
        [ModId::new("a"), ModId::new("b")],
        "an imported pair rule must be inert while use_imported_pairs is off"
    );

    session.update_settings(Settings {
        use_imported_pairs: true,
        ..session.settings()
    });

    assert_eq!(
        session.orders().suggested.as_slice(),
        [ModId::new("b"), ModId::new("a")],
        "enabling use_imported_pairs must re-sort so b loads before a"
    );
}

/// `Settings.use_imported_placements` must reach the sorter and
/// re-sort. An imported placement rule (Bottom) holds while the
/// toggle is on (default) and stops applying once it's turned off.
#[test]
fn toggling_use_imported_placements_reaches_the_sorter_and_re_sorts() {
    let mut session = session(&["pinned", "other"]);
    session.upsert_rule(Rule::Placement(PlacementRule {
        mod_id: ModId::new("pinned"),
        placement: Placement::Bottom,
        origin: RuleOrigin::RimSortCommunity,
        comment: None,
    }));

    assert_eq!(
        session.orders().suggested.as_slice(),
        [ModId::new("other"), ModId::new("pinned")],
        "the imported Bottom placement must hold while the toggle is on"
    );

    session.update_settings(Settings {
        use_imported_placements: false,
        ..session.settings()
    });

    assert_eq!(
        session.orders().suggested.as_slice(),
        [ModId::new("other"), ModId::new("pinned")],
        "with no ordering fact between them, Rebuild's alphabetical tie-break happens to \
             leave this pair unchanged — the toggle's own effect is asserted via the tier below"
    );
    assert_eq!(
        session.sort_outcome().placements[&ModId::new("pinned")].tier,
        rim_resolve::sort::Tier::Body,
        "disabling use_imported_placements must stop pinning it to Bottom"
    );
}

/// `Settings.tie_break` must reach the sorter and re-sort.
/// `Rebuild` (the default) ranks unconstrained mods by display name;
/// `PreserveCurrent` keeps the current order instead.
#[test]
fn toggling_tie_break_reaches_the_sorter_and_re_sorts() {
    let mut session = session(&["z", "a"]);

    assert_eq!(
        session.orders().suggested.as_slice(),
        [ModId::new("a"), ModId::new("z")],
        "Rebuild (the default) must rank these unconstrained mods alphabetically"
    );

    session.update_settings(Settings {
        tie_break: rim_resolve::sort::TieBreak::PreserveCurrent,
        ..session.settings()
    });

    assert_eq!(
        session.orders().suggested.as_slice(),
        [ModId::new("z"), ModId::new("a")],
        "PreserveCurrent must keep the current order's own position instead"
    );
}

/// The merge mod's placement rule: no placement at all
/// when the merge mod neither exists nor is about to; a placement the
/// moment either becomes true.
#[test]
fn merge_mod_placement_exists_only_when_the_mod_is_present_or_pending() {
    let current = LoadOrder::new(vec![ModId::new("a")]);
    assert_eq!(
        merge_mod_placement("abc123", &current, &DecisionSet::new()),
        None,
        "no merge activity at all -> no placement rule"
    );

    let current_with_merge_mod =
        LoadOrder::new(vec![ModId::new("a"), ModId::new("rimmerge.merge.abc123")]);
    let placement = merge_mod_placement("abc123", &current_with_merge_mod, &DecisionSet::new())
        .expect("the id already being in the current order must yield a placement");
    assert_eq!(placement.mod_id, ModId::new("rimmerge.merge.abc123"));
    assert_eq!(placement.placement, Placement::Bottom);
    assert_eq!(placement.origin, RuleOrigin::UserDecision);

    let mut decisions = DecisionSet::new();
    decisions
        .insert(Decision {
            key: FindingKey::DefOverride {
                key: rim_resolve::domain::DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Wall".to_string(),
                },
                owners: [ModId::new("a")].into_iter().collect(),
            },
            action: Action::Merge {
                key: rim_resolve::domain::DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Wall".to_string(),
                },
                choices: BTreeMap::new(),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("merge is always a valid action");
    assert!(
        merge_mod_placement("abc123", &current, &decisions).is_some(),
        "a pending Merge decision must also yield a placement, even before the id exists"
    );
}

/// Merge-first suggestions for clean previews, `DefOverride` side: a
/// `DefOverride` finding with no decision, whose real merge preview
/// turns out `Complete` with a real, non-empty plan (a nonzero op count
/// — see [`crate::test_support::clean_override_fixture`]'s own doc
/// comment for why a fixture needs a field the winner never touches to
/// produce one), does not get its ledger suggestion promoted to `Merge`
/// outright — `redecide_for_clean_merge`'s own non-zero-op `Complete`
/// arm only leads `Merge` in the alternatives for a `DefOverride`,
/// exactly like a genuine `NeedsFieldInput` conflict, since entering the
/// merge mod on an undecided `DefOverride` would have the ledger promise
/// something `RenderMergeMod` never carries out. `resolution.merge`
/// therefore stays `None` too — it's only ever populated when the
/// redecided action actually becomes `Merge`.
#[test]
fn a_clean_def_override_preview_only_leads_with_merge_never_promotes_it() {
    use crate::test_support::clean_override_fixture;

    let fixture = clean_override_fixture("Wall", "contributor.mod", "winner.mod");
    let def_key = rim_resolve::domain::DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
    };
    let key = FindingKey::DefOverride {
        key: def_key.clone(),
        owners: [
            ModId::new("core.mod"),
            ModId::new("contributor.mod"),
            ModId::new("winner.mod"),
        ]
        .into_iter()
        .collect(),
    };
    let mut session = crate::test_support::session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "contributor.mod", "winner.mod"],
    );
    session.set_def_source_reader(Arc::new(fixture.reader));
    assert!(session.settings().suggest_merge_when_clean, "sanity check");

    let resolution = session
        .resolution(OrderSource::Current, &key)
        .expect("the def override finding must be live");

    assert!(
        !matches!(resolution.suggestion.action, Action::Merge { .. }),
        "a DefOverride must never be promoted to Merge outright: {:?}",
        resolution.suggestion.action
    );
    assert_eq!(
        resolution.suggestion.alternatives.first(),
        Some(&rim_resolve::domain::Alternative {
            action: Action::Merge {
                key: def_key,
                choices: BTreeMap::new(),
            },
            rationale: rim_resolve::domain::Rationale::MergeLeadDefOverrideCombine,
        }),
        "Merge must still lead the alternatives, one click away: {:#?}",
        resolution.suggestion.alternatives
    );
    assert_eq!(resolution.effective, resolution.suggestion.action);
    assert!(resolution.decision.is_none(), "still no explicit decision");
    assert_eq!(
        resolution.merge, None,
        "merge is only populated once the redecided action actually becomes Merge: {:?}",
        resolution.merge
    );
}

/// Comparing the copies before calling a def override contested: an
/// "unexplained" `DefOverride`
/// (`rim_resolve::ledger::DefOverrideDirection::Unknown` — no
/// `Conflict::DefOverride` flag applies) whose every active owner's
/// copy is byte-identical promotes to `Accept` 99, no alternatives,
/// `Auto` at the default threshold.
#[test]
fn identical_copies_promote_an_unexplained_def_override_to_accept_99() {
    use crate::test_support::identical_def_override_fixture;

    let fixture = identical_def_override_fixture("Wall", &["core.mod", "a.mod", "b.mod"]);
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [
            ModId::new("core.mod"),
            ModId::new("a.mod"),
            ModId::new("b.mod"),
        ]
        .into_iter()
        .collect(),
    };
    let mut session = crate::test_support::session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    session.set_def_source_reader(Arc::new(fixture.reader));

    let resolution = session
        .resolution(OrderSource::Current, &key)
        .expect("the def override finding must be live");

    assert_eq!(resolution.suggestion.action, Action::Accept);
    assert_eq!(resolution.suggestion.confidence.percent(), 99);
    assert_eq!(
        resolution.suggestion.rationale.to_string(),
        "identical copies; order cannot change the outcome"
    );
    assert!(resolution.suggestion.alternatives.is_empty());
    assert_eq!(resolution.status, ResolutionStatus::Auto);
    assert_eq!(resolution.effective, Action::Accept);
}

/// The identical-copies check's other eligible bucket: `same_author`
/// (confidence 90 without it, indistinguishable from
/// `LoneNonVanillaOwner` by shape alone — see `DefOverrideDirection`'s
/// own doc comment) still promotes to `Accept` 99 once the copies are
/// confirmed identical.
#[test]
fn identical_copies_promote_a_same_author_def_override_to_accept_99() {
    use crate::test_support::identical_def_override_fixture;

    let mut fixture = identical_def_override_fixture("Wall", &["core.mod", "a.mod", "b.mod"]);
    match &mut fixture.report.conflicts[0] {
        rim_analyzer::domain::Conflict::DefOverride(d) => d.same_author = true,
        other => unreachable!("fixture's only conflict is its own DefOverride: {other:?}"),
    }
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [
            ModId::new("core.mod"),
            ModId::new("a.mod"),
            ModId::new("b.mod"),
        ]
        .into_iter()
        .collect(),
    };
    let mut session = crate::test_support::session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    session.set_def_source_reader(Arc::new(fixture.reader));

    let resolution = session
        .resolution(OrderSource::Current, &key)
        .expect("the def override finding must be live");

    assert_eq!(resolution.suggestion.action, Action::Accept);
    assert_eq!(resolution.suggestion.confidence.percent(), 99);
    assert!(resolution.suggestion.alternatives.is_empty());
}

/// The identical-copies check must never promote a `DefOverride` the confidence table
/// already explains with a stronger signal, even when the copies do
/// happen to agree — `LoneNonVanillaOwner` renders identically to
/// `SameAuthor` from the outside (`Accept` 90, `PreferWinner`
/// alternatives only), which is exactly why the eligibility check has
/// to be the shared classifier, not the suggestion's own shape.
#[test]
fn identical_copies_do_not_promote_a_lone_non_vanilla_owner_override() {
    use crate::test_support::identical_def_override_fixture;

    let mut fixture = identical_def_override_fixture("Wall", &["core.mod", "a.mod"]);
    fixture.report.mods[0].source = rim_analyzer::domain::Source::Core;
    match &mut fixture.report.conflicts[0] {
        rim_analyzer::domain::Conflict::DefOverride(d) => d.overrides_vanilla = true,
        other => unreachable!("fixture's only conflict is its own DefOverride: {other:?}"),
    }
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [ModId::new("core.mod"), ModId::new("a.mod")]
            .into_iter()
            .collect(),
    };
    let mut session = crate::test_support::session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod"],
    );
    session.set_def_source_reader(Arc::new(fixture.reader));

    let resolution = session
        .resolution(OrderSource::Current, &key)
        .expect("the def override finding must be live");

    assert_eq!(resolution.suggestion.confidence.percent(), 90);
    assert_eq!(
        resolution.suggestion.rationale.to_string(),
        "Exactly one active mod overrides this vanilla def; nothing else contests it."
    );
}

/// The identical-copies content check must never mistake genuinely
/// differing copies for identical ones: `conflicting_wall_fixture`'s three
/// owners each set `<label>` to a different value, so even though the
/// finding is `Unknown`-direction (eligible), the check itself must
/// decline — confidence stays at the unexplained-override's own 60 (the
/// merge-first `NeedsFieldInput` redecision leaves
/// `suggestion.action`/`confidence` untouched too, only reordering
/// alternatives — see `redecide_for_clean_merge`'s own doc comment).
#[test]
fn genuinely_differing_copies_are_never_promoted() {
    use crate::test_support::conflicting_wall_fixture;

    let fixture = conflicting_wall_fixture();
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [
            ModId::new("core.mod"),
            ModId::new("a.mod"),
            ModId::new("b.mod"),
        ]
        .into_iter()
        .collect(),
    };
    let mut session = crate::test_support::session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    session.set_def_source_reader(Arc::new(fixture.reader));

    let resolution = session
        .resolution(OrderSource::Current, &key)
        .expect("the def override finding must be live");

    assert_eq!(resolution.suggestion.action, Action::Accept);
    assert_eq!(resolution.suggestion.confidence.percent(), 60);
}

/// Key-representability: an owner the source index has no entry for at
/// all (a stale scan gap, or — the real case — a def that exists only as
/// a patch injection, with no
/// `Defs/`-inline source to read) must fail the content check for
/// *every* owner, not just be silently skipped — never mistaken for
/// "identical" on the strength of the readable owners alone.
#[test]
fn a_missing_source_entry_for_one_owner_is_never_mistaken_for_identical() {
    use crate::test_support::identical_def_override_fixture;

    let mut fixture = identical_def_override_fixture("Wall", &["core.mod", "a.mod", "b.mod"]);
    fixture
        .sources
        .defs
        .remove(&(
            ModId::new("a.mod"),
            ("ThingDef".to_string(), "Wall".to_string()),
        ))
        .expect("the fixture seeds a.mod's own entry");
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [
            ModId::new("core.mod"),
            ModId::new("a.mod"),
            ModId::new("b.mod"),
        ]
        .into_iter()
        .collect(),
    };
    let mut session = crate::test_support::session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    session.set_def_source_reader(Arc::new(fixture.reader));

    let resolution = session
        .resolution(OrderSource::Current, &key)
        .expect("the def override finding must be live");

    assert_eq!(resolution.suggestion.action, Action::Accept);
    assert_eq!(
        resolution.suggestion.confidence.percent(),
        60,
        "today's unexplained-override behaviour must survive unchanged, not become 99"
    );
}

/// Ground-truthed against the decompiled
/// `Verse.XmlInheritance.GetBestParentFor`: deciding `PreferWinner` on a
/// `DuplicateTemplateName` finding must actually move mods — a real
/// end-to-end proof that `Session::decide` correctly builds
/// `duplicate_template_children` from `session.sources()` and feeds it
/// to `sorter_overrides`, not just the pure `rim-resolve`-level unit
/// tests of the expansion logic itself. Every mod name is deliberately
/// chosen so the *natural* (alphabetical, `TieBreak::Rebuild`) order
/// violates both constraints the decision adds — the test only passes
/// because the decision actually reorders things, never by coincidence
/// of name order.
#[test]
fn deciding_prefer_winner_on_a_duplicate_template_name_reorders_the_winner_before_its_child_and_after_the_other_registrant()
 {
    use crate::test_support::duplicate_template_name_with_child_fixture;

    let fixture = duplicate_template_name_with_child_fixture();
    let mut session = crate::test_support::session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &[
            "core.mod",
            "zzz_other.mod",
            "mmm_winner.mod",
            "aaa_child.mod",
        ],
    );

    // Before deciding anything: natural (name-ranked) order is the
    // exact reverse of what the decision below must produce — the
    // fixture's own doc comment explains why.
    let before = &session.orders().suggested;
    assert!(
        before.position(&ModId::new("aaa_child.mod"))
            < before.position(&ModId::new("mmm_winner.mod"))
    );

    let decided = session
        .decide(Decision {
            key: FindingKey::DuplicateTemplateName {
                name: "Base".to_string(),
                owners: [ModId::new("zzz_other.mod"), ModId::new("mmm_winner.mod")]
                    .into_iter()
                    .collect(),
            },
            action: Action::PreferWinner {
                key: rim_resolve::domain::DefKey::synthesize_for_template_name("Base"),
                winner: ModId::new("mmm_winner.mod"),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("PreferWinner is always a valid action");
    assert!(
        decided,
        "the new registrant- and children-side Reorder pairs must actually change sorter_overrides"
    );

    let after = &session.orders().suggested;
    let other = after
        .position(&ModId::new("zzz_other.mod"))
        .expect("zzz_other.mod is active");
    let winner = after
        .position(&ModId::new("mmm_winner.mod"))
        .expect("mmm_winner.mod is active");
    let child = after
        .position(&ModId::new("aaa_child.mod"))
        .expect("aaa_child.mod is active");
    assert!(
        other < winner,
        "winner must load after the other registrant: other={other} winner={winner}"
    );
    assert!(
        winner < child,
        "winner must load before its own child: winner={winner} child={child}"
    );
}

/// Storing a `Merge`/`ShipAsset` decision invalidates only the decided
/// finding's cached merge preview, not every preview wholesale
/// (`Session::invalidate_ledgers`'s own `self.merges.clear_all()`): a
/// decision on one finding can never change a *different* finding's own
/// diff — its target's raw sources and its own stored choices are
/// untouched by a decision naming some other key. Two previews are
/// seeded directly (this test is about `Session::decide`'s own
/// invalidation scope, not about building two real overlapping fixtures
/// — the same technique `render_merge_mod.rs`'s own tests use). The
/// decided key's own stale entry is expected to be gone too: the real
/// caller (`DecideMerge`) always rebuilds and re-caches it right after
/// `decide` returns, so a bare `decide` call leaving it cleared, rather
/// than stale, is correct.
#[test]
fn a_merge_decision_leaves_other_findings_own_cached_previews_alone() {
    let decided_key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
        },
        owners: [
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ]
        .into_iter()
        .collect(),
    };
    let other_key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [ModId::new("a.mod"), ModId::new("b.mod")]
            .into_iter()
            .collect(),
    };
    let fixture = crate::test_support::bionic_heart_fixture();
    let mut session = crate::test_support::session_with_sources(fixture.sources, fixture.report);
    let slot = PreviewSlot::profile(OrderSource::Current);
    let synthetic_preview = |key: &FindingKey, def_key: rim_resolve::domain::DefKey| MergePreview {
        key: key.clone(),
        owners: vec![ModId::new("a.mod"), ModId::new("b.mod")],
        base: ModId::new("a.mod"),
        winner: ModId::new("b.mod"),
        diff: rim_merge::diff::ThreeWayDiff {
            base: ModId::new("a.mod"),
            fields: Vec::new(),
        },
        plan: rim_merge::plan::MergePlan {
            key: def_key,
            selector: rim_analyzer::domain::Selector::DefName,
            winner: ModId::new("b.mod"),
            owners: vec![ModId::new("a.mod"), ModId::new("b.mod")],
            ops: Vec::new(),
            unresolved: Vec::new(),
            caveats: Vec::new(),
        },
        state: MergeState::Complete { op_count: 0 },
        structural_change: None,
        final_values: BTreeMap::new(),
    };
    let empty_ctx = || MergeContext {
        slot: slot.clone(),
        choices: BTreeMap::new(),
        scope: None,
    };
    session.cache_merge_preview(
        empty_ctx(),
        synthetic_preview(
            &decided_key,
            rim_resolve::domain::DefKey {
                def_type: "HediffDef".to_string(),
                def_name: "BionicHeart".to_string(),
            },
        ),
    );
    session.cache_merge_preview(
        empty_ctx(),
        synthetic_preview(
            &other_key,
            rim_resolve::domain::DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
            },
        ),
    );
    assert!(
        session.merge_preview(&slot, &decided_key).is_some(),
        "sanity check"
    );
    assert!(
        session.merge_preview(&slot, &other_key).is_some(),
        "sanity check"
    );

    session
        .decide(Decision {
            key: decided_key.clone(),
            action: Action::Merge {
                key: rim_resolve::domain::DefKey {
                    def_type: "HediffDef".to_string(),
                    def_name: "BionicHeart".to_string(),
                },
                choices: BTreeMap::new(),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("merge is always a valid action");

    assert!(
        session.merge_preview(&slot, &other_key).is_some(),
        "an unrelated finding's own cached preview must survive a merge decision on a different key"
    );
    assert!(
        session.merge_preview(&slot, &decided_key).is_none(),
        "the decided finding's own stale entry must not survive either — its caller (DecideMerge) rebuilds and re-caches it right after decide() returns"
    );
}

/// A patch's scoped ledger must not be derived from
/// [`Session::ensure_ledger`]'s own cached profile ledger — the very
/// one [`Session::redecide_clean_merge_at`] mutates in place once
/// something (the profile inbox, [`Session::resolution`]) views the
/// same finding and its clean-merge preview resolves. Building the
/// scoped ledger *before* that promotion would bake in the pristine
/// suggestion; the exact same scoped ledger, rebuilt later (its own
/// cache invalidated by an unrelated patch decision) after the
/// profile promoted the finding, would bake in the promoted suggestion
/// instead — two different answers for the same patch, same key, same
/// patch decisions, differing only by incidental call order elsewhere
/// in the session. [`Session::patch_ledger`] must derive from a
/// pristine, unredecided copy of the profile ledger so both builds
/// agree.
#[test]
fn scoped_ledger_suggestion_is_stable_across_a_profile_clean_merge_promotion() {
    use crate::test_support::clean_override_fixture;

    let fixture = clean_override_fixture("Wall", "contributor.mod", "winner.mod");
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [
            ModId::new("core.mod"),
            ModId::new("contributor.mod"),
            ModId::new("winner.mod"),
        ]
        .into_iter()
        .collect(),
    };
    let mut session = crate::test_support::session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "contributor.mod", "winner.mod"],
    );
    session.set_def_source_reader(Arc::new(fixture.reader));

    let patch_id: PatchId = "abcdef012345".parse().expect("valid patch id");
    let project = PatchProject::new(
        patch_id.clone(),
        "Contributor+Winner compat".to_string(),
        rim_resolve::domain::PatchModIdentity::new("test.cwcompat", "CW Compat")
            .expect("valid identity"),
        rim_resolve::domain::PatchScope::new([
            ModId::new("contributor.mod"),
            ModId::new("winner.mod"),
        ])
        .expect("two distinct members"),
        jiff::Timestamp::UNIX_EPOCH,
    );
    session.upsert_patch(project);

    // Build the scoped ledger once, before anything promotes the
    // profile's own suggestion.
    let before = session
        .patch_ledger(&patch_id, OrderSource::Current)
        .expect("patch is loaded")
        .entries
        .iter()
        .find(|e| e.key == key)
        .expect("the wall key is Partial-admitted by this scope")
        .suggestion
        .clone();

    // Viewing the finding through the profile inbox triggers the
    // clean-merge promotion, mutating the cached profile ledger in
    // place.
    let profile_resolution = session
        .resolution(OrderSource::Current, &key)
        .expect("the def override finding must be live");
    // A `DefOverride`'s `Complete { op_count > 0 }` preview never
    // promotes `action` to `Merge` outright —
    // `redecide_for_clean_merge` only leads with `Merge` in the
    // alternatives (`action`/`rationale` untouched). The redecision still
    // demonstrably touches this suggestion (this test's own point: the
    // profile view mutates the cached ledger entry in place), just via
    // the alternatives, not the action.
    assert_eq!(
        profile_resolution.suggestion.action,
        Action::Accept,
        "sanity check: the ledger's own original action is untouched by the redecision here"
    );
    assert_eq!(
        profile_resolution.suggestion.alternatives.first(),
        Some(&rim_resolve::domain::Alternative {
            action: Action::Merge {
                key: rim_resolve::domain::DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Wall".to_string(),
                },
                choices: BTreeMap::new(),
            },
            rationale: rim_resolve::domain::Rationale::MergeLeadDefOverrideCombine,
        }),
        "sanity check: the profile suggestion actually got clean-merge-redecided \
             (Merge now leads the alternatives)"
    );

    // Invalidate the patch's own cache via a patch decision — decide
    // then revert leaves no decision on file, isolating this test to
    // the cache-invalidation path alone.
    session
        .patch_decide(
            &patch_id,
            Decision {
                key: key.clone(),
                action: Action::Ignore,
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            },
        )
        .expect("Ignore is patchable and the wall key is in scope");
    session
        .patch_revert(&patch_id, &key)
        .expect("patch is loaded");

    let after = session
        .patch_ledger(&patch_id, OrderSource::Current)
        .expect("patch is loaded")
        .entries
        .iter()
        .find(|e| e.key == key)
        .expect("the wall key is Partial-admitted by this scope")
        .suggestion
        .clone();

    assert_eq!(
        before, after,
        "a patch's scoped suggestion must not depend on whether the profile viewed \
             (and clean-merge-redecided) the same finding in between the two builds"
    );
}

/// `patch_findings`/`patch_resolution`/`patch_ledger` are three views
/// over the same cache and must agree with each other; building any
/// one of them populates `Session::scoped`, and both
/// `invalidate_ledgers` (a profile-wide change) and
/// `invalidate_patch_caches` (that patch's own change) must clear it.
#[test]
fn patch_findings_and_patch_resolution_agree_and_the_scoped_cache_invalidates_correctly() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .def_override("ThingDef", "Wall", &["a", "b"])
        .build();
    let mut session = Session::new(
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
            active_mods: vec![ModId::new("a"), ModId::new("b")],
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    );
    let scope = rim_resolve::domain::PatchScope::new([ModId::new("a"), ModId::new("b")])
        .expect("two distinct members");
    let identity = rim_resolve::domain::PatchModIdentity::new("test.abcompat", "AB Compat")
        .expect("valid identity");
    let id = rim_resolve::domain::PatchId::derive(
        "profile",
        identity.package_id(),
        jiff::Timestamp::UNIX_EPOCH,
    );
    let project = PatchProject::new(
        id.clone(),
        "AB Compat Project".to_string(),
        identity,
        scope,
        jiff::Timestamp::UNIX_EPOCH,
    );
    session.upsert_patch(project);
    let source = OrderSource::Current;
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
    };

    assert!(session.scoped.is_empty(), "sanity check: nothing built yet");

    let resolution = session
        .patch_resolution(&id, source, &key)
        .expect("patch is loaded")
        .cloned();
    let page = session
        .patch_findings(
            &id,
            source,
            &FindingFilter {
                limit: 10,
                ..FindingFilter::default()
            },
        )
        .expect("patch is loaded");
    let ledger = session
        .patch_ledger(&id, source)
        .expect("patch is loaded")
        .clone();

    assert!(
        !session.scoped.is_empty(),
        "building any of the three views must populate the scoped cache"
    );
    assert!(
        page.items.contains(&key),
        "patch_findings must list the same key"
    );
    let from_ledger = ledger
        .entries
        .iter()
        .find(|entry| entry.key == key)
        .cloned();
    assert_eq!(
        resolution, from_ledger,
        "patch_resolution must agree with patch_ledger's own entry"
    );

    // A profile-wide change clears the whole scoped cache.
    session.invalidate_ledgers();
    assert!(
        session.scoped.is_empty(),
        "invalidate_ledgers must clear every patch's scoped cache"
    );

    // Rebuild, then prove a per-patch invalidation clears it too.
    session.patch_ledger(&id, source).expect("patch is loaded");
    assert!(!session.scoped.is_empty(), "sanity check: rebuilt");
    session.invalidate_patch_caches(&id);
    assert!(
        session.scoped.is_empty(),
        "invalidate_patch_caches must clear this patch's own scoped cache"
    );
}

/// `Session::patch_orphaned`
/// derives `live` from the patch's own scoped ledger, sparing every
/// caller (`apps/cli`/`apps/desktop/src-tauri`'s `commands/patch.rs`)
/// from rebuilding that set itself.
#[test]
fn patch_orphaned_reports_a_decision_the_current_scope_no_longer_admits() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .mod_("c")
        .def_override("ThingDef", "Wall", &["a", "b"])
        .build();
    let mut session = Session::new(
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
            active_mods: vec![ModId::new("a"), ModId::new("b"), ModId::new("c")],
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    );
    let scope = rim_resolve::domain::PatchScope::new([ModId::new("a"), ModId::new("b")])
        .expect("two distinct members");
    let identity = rim_resolve::domain::PatchModIdentity::new("test.abcompat", "AB Compat")
        .expect("valid identity");
    let id = rim_resolve::domain::PatchId::derive(
        "profile",
        identity.package_id(),
        jiff::Timestamp::UNIX_EPOCH,
    );
    let mut project = PatchProject::new(
        id.clone(),
        "AB Compat Project".to_string(),
        identity,
        scope,
        jiff::Timestamp::UNIX_EPOCH,
    );
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
    };
    project
        .decide(Decision {
            key: key.clone(),
            action: Action::Ignore,
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("key is admitted by the patch's own scope");
    session.upsert_patch(project);
    let source = OrderSource::Current;

    assert_eq!(
        session
            .patch_orphaned(&id, source)
            .expect("patch is loaded"),
        Vec::new(),
        "an admitted, still-live decision must not be orphaned"
    );

    let shrunk_scope = rim_resolve::domain::PatchScope::new([ModId::new("a"), ModId::new("c")])
        .expect("two distinct members");
    let mut project = session.remove_patch(&id).expect("still loaded");
    project.set_scope(shrunk_scope);
    session.upsert_patch(project);

    assert_eq!(
        session
            .patch_orphaned(&id, source)
            .expect("patch is loaded"),
        vec![key],
        "a decision the shrunk scope no longer admits must be reported as orphaned"
    );
}

#[test]
fn patch_orphaned_returns_unknown_patch_for_an_unloaded_id() {
    let mut session = session(&["a", "b"]);
    let id = rim_resolve::domain::PatchId::derive(
        "profile",
        &rim_analyzer::domain::ModId::new("test.unloaded"),
        jiff::Timestamp::UNIX_EPOCH,
    );

    let error = session
        .patch_orphaned(&id, OrderSource::Current)
        .expect_err("no such patch is loaded");

    assert_eq!(error, UnknownPatch(id));
}

/// A `DefOverride`
/// finding whose clean preview is `MergeState::Complete { op_count: 0
/// }` (every field already resolves to the load-order winner's own
/// value — [`bionic_heart_fixture_flat_with_conflict`]'s own shape)
/// must never be promoted to `Action::Merge`: the "merge" would write
/// nothing, so promoting it would be a false "the mods change
/// different fields" claim over an empty patch. Instead the finding
/// becomes a plain `Accept` at confidence 95, `Auto` (95 clears the
/// default 80 threshold), with `Merge` removed from the alternatives
/// entirely and the `PreferWinner` alternatives kept.
///
/// Uses [`bionic_heart_fixture_flat_with_conflict`], not
/// [`bionic_heart_fixture_with_conflict`]: that fixture's `ParentName`
/// difference fires the structural guard, so its preview is never
/// `Complete` at all — see
/// [`a_guarded_preview_never_promotes_even_though_the_ledger_still_offers_merge`]
/// below for that def's own outcome, and
/// [`crate::test_support::bionic_heart_fixture_flat`]'s own doc
/// comment for why this test needs a fixture the guard doesn't touch.
#[test]
fn a_zero_op_clean_preview_never_promotes_the_suggestion() {
    use crate::test_support::bionic_heart_fixture_flat_with_conflict;

    let fixture = bionic_heart_fixture_flat_with_conflict();
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
        },
        owners: [
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ]
        .into_iter()
        .collect(),
    };
    let mut session = crate::test_support::session_with_sources(fixture.sources, fixture.report);
    session.set_def_source_reader(Arc::new(fixture.reader));
    assert!(session.settings().suggest_merge_when_clean, "sanity check");

    let resolution = session
        .resolution(OrderSource::Current, &key)
        .expect("the def override finding must be live");

    assert_eq!(
        resolution.suggestion.action,
        Action::Accept,
        "a no-op merge must never become the suggestion"
    );
    assert_eq!(
        resolution.suggestion.confidence.percent(),
        95,
        "a zero-op clean preview is Accept 95 (80 only when the preview assumed \
             default mod settings)"
    );
    assert_eq!(resolution.status, ResolutionStatus::Auto);
    assert!(
        resolution
            .suggestion
            .alternatives
            .iter()
            .all(|alt| !matches!(alt.action, Action::Merge { .. })),
        "Merge must be gone from the alternatives entirely: {:?}",
        resolution.suggestion.alternatives
    );
    assert!(
        resolution
            .suggestion
            .alternatives
            .iter()
            .any(|alt| matches!(alt.action, Action::PreferWinner { .. })),
        "the PreferWinner alternatives must survive: {:?}",
        resolution.suggestion.alternatives
    );
    assert!(
        resolution.merge.is_none(),
        "the state pill is for an actual Merge, not the zero-op Accept: {:?}",
        resolution.merge
    );
}

/// The same finding, but with `suggestMergeWhenClean` off: the merge-first
/// redecision never runs, so the ledger's plain suggestion (`Accept` 60,
/// `Merge` only ever offered as an alternative) survives unchanged.
#[test]
fn the_setting_off_leaves_the_ledger_suggestion_untouched() {
    use crate::test_support::bionic_heart_fixture_with_conflict;

    let fixture = bionic_heart_fixture_with_conflict();
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
        },
        owners: [
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ]
        .into_iter()
        .collect(),
    };
    let mut session = crate::test_support::session_with_sources(fixture.sources, fixture.report);
    session.set_def_source_reader(Arc::new(fixture.reader));
    let mut settings = session.settings();
    settings.suggest_merge_when_clean = false;
    session.update_settings(settings);

    let resolution = session
        .resolution(OrderSource::Current, &key)
        .expect("the def override finding must be live");

    assert_eq!(resolution.suggestion.action, Action::Accept);
    assert_eq!(resolution.suggestion.confidence.percent(), 60);
    assert_eq!(resolution.status, ResolutionStatus::NeedsInput);
}

/// The structural guard, at the ledger level: the real
/// `bionic_heart_fixture_with_conflict` — BIONICS's own `ParentName`
/// genuinely differs from Core's — never reaches `MergeState::Complete`,
/// so it can never take
/// [`a_zero_op_clean_preview_never_promotes_the_suggestion`]'s own path
/// through `redecide_for_clean_merge`. Its preview is `NeedsFieldInput`
/// instead (guard-forced, `unresolved == total`, not a genuine
/// per-field conflict — see `state_from_plan`'s own doc comment), so
/// `action`/`confidence`/`status` stay exactly the ledger's own
/// `Accept` 60/`NeedsInput` (identical to the setting-off case above)
/// — the guard forces a confirmation, it does not change who wins.
/// `Merge` is not offered as an alternative at all here: reordering it
/// to the front with a rationale naming a field count ("Merge, 9 fields
/// need a choice.") would advertise a merge that can never complete, so
/// `redecide_for_clean_merge`'s own guarded `NeedsFieldInput` arm strips
/// it outright and replaces the rationale with one naming the guard's
/// own field instead.
#[test]
fn a_guarded_preview_never_promotes_and_no_longer_offers_merge() {
    use crate::test_support::bionic_heart_fixture_with_conflict;

    let fixture = bionic_heart_fixture_with_conflict();
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
        },
        owners: [
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ]
        .into_iter()
        .collect(),
    };
    let mut session = crate::test_support::session_with_sources(fixture.sources, fixture.report);
    session.set_def_source_reader(Arc::new(fixture.reader));
    assert!(session.settings().suggest_merge_when_clean, "sanity check");

    let resolution = session
        .resolution(OrderSource::Current, &key)
        .expect("the def override finding must be live");

    assert_eq!(
        resolution.suggestion.action,
        Action::Accept,
        "a guarded def must never be promoted to Merge"
    );
    assert_eq!(resolution.suggestion.confidence.percent(), 60);
    assert_eq!(resolution.status, ResolutionStatus::NeedsInput);
    assert!(
        resolution
            .suggestion
            .alternatives
            .iter()
            .all(|alt| !matches!(alt.action, Action::Merge { .. })),
        "a guarded def must not offer Merge at all — it can never complete: {:?}",
        resolution.suggestion.alternatives
    );
    assert_eq!(
        resolution.suggestion.rationale.to_string(),
        "ParentName differs between owners — field-level merging is unsafe; confirm the \
             load-order winner",
        "the rationale must name the guard, not a field count"
    );
}

/// A finding whose preview needs a field choice
/// (`MergeState::NeedsFieldInput`) keeps the ledger's own suggestion
/// (still `Accept` 60, still `NeedsInput`), but `Merge` moves to the
/// front of the alternatives with the remaining-conflict count named
/// in its rationale.
#[test]
fn a_needs_field_input_preview_leads_the_alternatives_with_merge() {
    use crate::test_support::conflicting_wall_fixture;

    let fixture = conflicting_wall_fixture();
    let key = FindingKey::DefOverride {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [
            ModId::new("core.mod"),
            ModId::new("a.mod"),
            ModId::new("b.mod"),
        ]
        .into_iter()
        .collect(),
    };
    let mut session = crate::test_support::session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    session.set_def_source_reader(Arc::new(fixture.reader));

    let resolution = session
        .resolution(OrderSource::Current, &key)
        .expect("the def override finding must be live");

    assert_eq!(
        resolution.suggestion.action,
        Action::Accept,
        "the ledger's own suggestion must survive unchanged"
    );
    assert_eq!(resolution.status, ResolutionStatus::NeedsInput);
    let first_alternative = resolution
        .suggestion
        .alternatives
        .first()
        .expect("at least one alternative");
    assert_eq!(
        first_alternative.action,
        Action::Merge {
            key: rim_resolve::domain::DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
            },
            choices: BTreeMap::new(),
        }
    );
    assert_eq!(
        first_alternative.rationale.to_string(),
        "Merge, 1 field needs a choice."
    );
}

/// `redecide_clean_merge_at` mutates a ledger entry in place, but the
/// `FindingIndex` built alongside it caches `sorted`/`needs_input_by_mod`
/// from *before* any redecision runs. If `Session::findings` never
/// redecided, a page fetched once, then resolved per item (exactly what
/// `list_findings_inner` does to build each row's DTO — see
/// `apps/desktop/src-tauri/src/commands/findings.rs`), would promote
/// entries out from under a page whose `total`/`items` were already
/// computed and returned — so the *next*, otherwise-identical
/// `list_findings` call would disagree with the first, since
/// `FindingIndex::page`'s status filter reads live status while its
/// `sorted` order does not. `Session::findings` therefore redecides
/// every item on the page it's about to return, rebuilding the index
/// and re-paging until nothing more changes, so it settles *before*
/// returning — no caller-visible drift is possible afterward.
#[test]
fn findings_paging_stays_consistent_once_two_findings_are_lazily_promoted() {
    use crate::test_support::two_zero_op_overrides_fixture;

    // `two_zero_op_overrides_fixture`, not `two_clean_overrides_fixture`:
    // a non-zero-op `DefOverride` is un-promotable, so
    // `two_clean_overrides_fixture`'s own `WallA`/`WallB` never
    // lazily promote at all — see that fixture's own doc comment.
    let fixture = two_zero_op_overrides_fixture();
    let active_mods: Vec<&str> = fixture.active_mods.clone();
    let mut session = crate::test_support::session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &active_mods,
    );
    session.set_def_source_reader(Arc::new(fixture.reader));
    assert!(session.settings().suggest_merge_when_clean, "sanity check");

    let filter = FindingFilter {
        status: Some(ResolutionStatus::NeedsInput),
        limit: 1,
        ..FindingFilter::default()
    };

    // Mirrors `list_findings_inner`: page, then fetch each returned
    // item's full resolution (as the desktop command does to build
    // the DTO) — the step that could silently promote entries the
    // just-returned page never accounted for.
    let first_page = session.findings(OrderSource::Current, &filter);
    for key in &first_page.items {
        session
            .resolution(OrderSource::Current, key)
            .expect("an item the page just returned must still be live");
    }

    let second_page = session.findings(OrderSource::Current, &filter);

    assert_eq!(
        first_page.total, second_page.total,
        "two otherwise-identical list_findings calls must agree on the total"
    );
    assert_eq!(
        first_page.items, second_page.items,
        "two otherwise-identical list_findings calls must return the same page"
    );
    for key in &second_page.items {
        let resolution = session
            .resolution(OrderSource::Current, key)
            .expect("must still be live");
        assert_ne!(
            resolution.status,
            ResolutionStatus::Auto,
            "an item on a NeedsInput-filtered page must never actually be Auto: {key:?}"
        );
    }
    // The three-owner `WallC` conflict never promotes (see
    // `two_zero_op_overrides_fixture`'s own doc comment), so it must
    // still be the one control item left on both pages once the two
    // zero-op overrides have settled to `Auto`.
    assert_eq!(
        second_page.total, 1,
        "only the never-promoting WallC conflict should remain NeedsInput"
    );
}

#[test]
fn file_matches_is_true_when_the_file_holds_the_orders_sequence() {
    let session = session(&["a", "b"]);

    assert!(session.file_matches(OrderSource::Current));
}

#[test]
fn file_matches_is_false_when_the_file_order_differs() {
    let mut session = session(&["a", "b"]);
    session.set_file_active_mods(vec![ModId::new("b"), ModId::new("a")]);

    assert!(!session.file_matches(OrderSource::Current));
}

#[test]
fn file_matches_is_false_when_the_file_lists_a_different_set_of_mods() {
    let mut session = session(&["a", "b"]);
    session.set_file_active_mods(vec![ModId::new("a")]);

    assert!(!session.file_matches(OrderSource::Current));
}

#[test]
fn file_matches_ignores_the_generated_merge_mod_id() {
    let mut session = session(&["a", "b"]);
    let merge_mod =
        rim_resolve::domain::GeneratedModIdentity::for_profile(session.paths().profile_hash())
            .package_id;
    session.set_file_active_mods(vec![ModId::new("a"), ModId::new("b"), merge_mod]);

    assert!(session.file_matches(OrderSource::Current));
}

fn reorder_a_after_b(session: &mut Session) {
    session
        .decide(Decision {
            key: FindingKey::UndeclaredHardDependency {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
            action: Action::Reorder {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("reorder is always a valid action");
}

#[test]
fn order_on_disk_is_applied_when_the_file_holds_the_selected_order() {
    let session = session(&["a", "b"]);

    assert_eq!(session.order_on_disk(), OrderOnDisk::Applied);
}

#[test]
fn order_on_disk_reports_unscanned_activation_changes_before_a_differing_order() {
    let mut session = session(&["a", "b"]);
    session.set_file_active_mods(vec![ModId::new("b"), ModId::new("a")]);
    session.working_mut().deactivate(&[ModId::new("b")]);
    assert!(
        !session.file_matches(OrderSource::Current),
        "sanity: order differs"
    );
    assert!(session.is_stale(), "sanity: working set is unscanned");

    assert_eq!(
        session.order_on_disk(),
        OrderOnDisk::NotApplied(UnappliedReason::ActivationChangesNotScanned)
    );
}

#[test]
fn order_on_disk_reports_a_differing_order_when_nothing_is_unscanned() {
    let mut session = session(&["a", "b"]);
    session.set_file_active_mods(vec![ModId::new("b"), ModId::new("a")]);

    assert_eq!(
        session.order_on_disk(),
        OrderOnDisk::NotApplied(UnappliedReason::OrderDiffers)
    );
}

#[test]
fn order_on_disk_follows_the_selected_order_not_suggested() {
    let mut session = session(&["a", "b"]);
    reorder_a_after_b(&mut session);
    assert!(
        !session.file_matches(OrderSource::Suggested),
        "sanity: the file holds the current order, not the suggested one"
    );

    assert_eq!(session.selected(), OrderSource::Current);
    assert_eq!(session.order_on_disk(), OrderOnDisk::Applied);

    session.select(OrderSource::Suggested);
    assert_eq!(
        session.order_on_disk(),
        OrderOnDisk::NotApplied(UnappliedReason::OrderDiffers)
    );
}

#[test]
fn order_on_disk_ignores_the_generated_merge_mod() {
    let mut session = session(&["a", "b"]);
    let merge_mod =
        rim_resolve::domain::GeneratedModIdentity::for_profile(session.paths().profile_hash())
            .package_id;
    session.set_file_active_mods(vec![ModId::new("a"), ModId::new("b"), merge_mod]);

    assert_eq!(session.order_on_disk(), OrderOnDisk::Applied);
}
