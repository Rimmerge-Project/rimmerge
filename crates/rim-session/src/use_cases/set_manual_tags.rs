//! [`SetManualTags`]: sets (or removes) one manual tag on a whole set of mods with a single save.

use std::collections::BTreeSet;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{Tag, TagMode};

use crate::ports::{RuleStore, StoreError};
use crate::{ManualTagsChange, Session};

/// Applies one manual tag override to every mod of a set, then saves the rules file once through
/// [`RuleStore`]. The bulk sibling of [`crate::use_cases::SetManualTag`]: it never re-sorts (see
/// [`Session::set_manual_tags`]), and a request that changes nothing writes nothing.
pub struct SetManualTags<Rules> {
    rule_store: Rules,
}

impl<Rules: RuleStore> SetManualTags<Rules> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(rule_store: Rules) -> Self {
        Self { rule_store }
    }

    /// Adds `tag` to, or removes it from, every mod in `mods` (each once: the set dedupes).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when saving fails, in which case every override is rolled back
    /// (without a re-sort) so the session never runs ahead of disk.
    pub fn execute(
        &self,
        session: &mut Session,
        mods: &BTreeSet<ModId>,
        tag: &Tag,
        mode: TagMode,
    ) -> Result<ManualTagsChange, StoreError> {
        let snapshot = session.manual_tags_snapshot();
        let change = session.set_manual_tags(mods, tag, mode);
        if change.changed == 0 {
            return Ok(change);
        }
        if let Err(error) = self
            .rule_store
            .save(&session.paths().profile_dir, session.rules())
        {
            // The tag-only restore: `restore_rules` would re-sort and could replace Suggested.
            session.restore_manual_tags(snapshot);
            return Err(error);
        }
        Ok(change)
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::{LoadOrder, Source};
    use rim_resolve::domain::{
        Action, Decision, DecisionSet, FindingKey, ManualTag, OrderSource, TagEvidence, TagRule,
        TagSignal,
    };
    use rim_resolve::sort::TieBreak;

    use super::*;
    use crate::ProjectPaths;
    use crate::ports::{ModsConfigFile, StoredRules};
    use crate::settings::Settings;
    use crate::test_support::{InMemoryRuleStore, report_fixture, session_fixture};

    fn ids(raw: &[&str]) -> BTreeSet<ModId> {
        raw.iter().map(|id| ModId::new(*id)).collect()
    }

    fn framework() -> Tag {
        Tag::new("framework").expect("valid tag")
    }

    fn has_tag(session: &Session, id: &str) -> bool {
        session
            .tagging()
            .tags_of(&ModId::new(id))
            .contains(&framework())
    }

    fn paths() -> ProjectPaths {
        ProjectPaths {
            game_dir: "game".into(),
            workshop_dir: "workshop".into(),
            mods_config: "ModsConfig.xml".into(),
            profile_dir: "profile".into(),
        }
    }

    fn mods_config(ids: &[&str]) -> ModsConfigFile {
        ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: ids.iter().map(|id| ModId::new(*id)).collect(),
            known_expansions: Vec::new(),
        }
    }

    /// A session over `ids` where every id in `inferred` earns `framework` from the scan evidence.
    fn tagged_session(ids: &[&str], inferred: &[&str], rules: StoredRules) -> Session {
        decided_session(ids, inferred, &[], rules)
    }

    /// [`tagged_session`] with an `AddTag` decision on file for every id of `decided`.
    fn decided_session(
        ids: &[&str],
        inferred: &[&str],
        decided: &[&str],
        rules: StoredRules,
    ) -> Session {
        let evidence = inferred
            .iter()
            .map(|id| TagEvidence {
                mod_id: ModId::new(*id),
                url: Some("https://mods.example/framework".to_string()),
                source: Source::Workshop,
                def_type_namespaces: BTreeSet::new(),
                def_name_prefixes: BTreeSet::new(),
                assembly_refs_to: BTreeSet::new(),
                declares_after: BTreeSet::new(),
            })
            .collect();
        let rules = StoredRules {
            tag_rules: vec![TagRule {
                tag: framework(),
                any_of: vec![TagSignal::UrlContains("mods.example/framework".to_string())],
            }],
            ..rules
        };
        Session::new(
            paths(),
            report_fixture(ids),
            evidence,
            SourceIndex::default(),
            rules,
            add_tag_decisions(decided),
            mods_config(ids),
            Vec::new(),
            Vec::new(),
        )
    }

    /// A ledger decision `AddTag(id, framework)` for every id of `decided`.
    fn add_tag_decisions(decided: &[&str]) -> DecisionSet {
        let mut decisions = DecisionSet::new();
        for id in decided {
            decisions
                .insert(Decision {
                    key: FindingKey::TagInferred {
                        mod_id: ModId::new(*id),
                        tag: framework(),
                    },
                    action: Action::AddTag {
                        mod_id: ModId::new(*id),
                        tag: framework(),
                    },
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                })
                .expect("valid decision");
        }
        decisions
    }

    fn overrides(session: &Session) -> Vec<(String, TagMode)> {
        let mut found: Vec<(String, TagMode)> = session
            .rules_snapshot()
            .manual_tags
            .iter()
            .map(|m| (m.mod_id.as_str().to_string(), m.mode))
            .collect();
        found.sort_by(|left, right| left.0.cmp(&right.0));
        found
    }

    fn manual(id: &str, mode: TagMode) -> ManualTag {
        ManualTag {
            mod_id: ModId::new(id),
            tag: framework(),
            mode,
        }
    }

    #[test]
    fn adds_the_tag_to_every_mod_with_one_save() {
        let mut session = session_fixture(&["a", "b", "c"]);
        let use_case = SetManualTags::new(InMemoryRuleStore::new());

        let change = use_case
            .execute(&mut session, &ids(&["a", "b"]), &framework(), TagMode::Add)
            .expect("bulk tag must succeed");

        assert_eq!(
            change,
            ManualTagsChange {
                changed: 2,
                kept_by_decision: 0,
                unchanged: 0
            }
        );
        assert!(has_tag(&session, "a") && has_tag(&session, "b"));
        assert!(!has_tag(&session, "c"));
        assert_eq!(use_case.rule_store.save_count(), 1);
    }

    #[test]
    fn a_repeated_add_changes_nothing_and_writes_nothing() {
        let mut session = session_fixture(&["a"]);
        let use_case = SetManualTags::new(InMemoryRuleStore::new());
        use_case
            .execute(&mut session, &ids(&["a"]), &framework(), TagMode::Add)
            .expect("first add");

        let change = use_case
            .execute(&mut session, &ids(&["a"]), &framework(), TagMode::Add)
            .expect("second add");

        assert_eq!(
            change,
            ManualTagsChange {
                changed: 0,
                kept_by_decision: 0,
                unchanged: 1
            }
        );
        assert_eq!(use_case.rule_store.save_count(), 1);
    }

    #[test]
    fn an_add_replaces_a_recorded_remove() {
        let mut session = tagged_session(
            &["a"],
            &["a"],
            StoredRules {
                manual_tags: vec![manual("a", TagMode::Remove)],
                ..StoredRules::default()
            },
        );
        let use_case = SetManualTags::new(InMemoryRuleStore::new());

        use_case
            .execute(&mut session, &ids(&["a"]), &framework(), TagMode::Add)
            .expect("add");

        assert_eq!(overrides(&session), vec![("a".to_string(), TagMode::Add)]);
        assert!(has_tag(&session, "a"));
    }

    #[test]
    fn removing_over_a_mixed_selection_leaves_only_the_overrides_that_hide_a_shown_tag() {
        // m: manual only; i: inferred only; b: manual and inferred; x: not shown at all.
        let mut session = tagged_session(
            &["m", "i", "b", "x"],
            &["i", "b"],
            StoredRules {
                manual_tags: vec![manual("m", TagMode::Add), manual("b", TagMode::Add)],
                ..StoredRules::default()
            },
        );
        let use_case = SetManualTags::new(InMemoryRuleStore::new());

        let change = use_case
            .execute(
                &mut session,
                &ids(&["m", "i", "b", "x"]),
                &framework(),
                TagMode::Remove,
            )
            .expect("remove");

        assert_eq!(
            change,
            ManualTagsChange {
                changed: 3,
                kept_by_decision: 0,
                unchanged: 1
            }
        );
        assert_eq!(
            overrides(&session),
            vec![
                ("b".to_string(), TagMode::Remove),
                ("i".to_string(), TagMode::Remove)
            ],
            "the Add on m is deleted with nothing left to hide, and x gets no dead Remove"
        );
        assert!(["m", "i", "b", "x"].iter().all(|id| !has_tag(&session, id)));
        assert_eq!(use_case.rule_store.save_count(), 1);
    }

    #[test]
    fn removing_a_tag_nobody_shows_changes_nothing_and_writes_nothing() {
        let mut session = session_fixture(&["a", "b"]);
        let use_case = SetManualTags::new(InMemoryRuleStore::new());

        let change = use_case
            .execute(
                &mut session,
                &ids(&["a", "b"]),
                &framework(),
                TagMode::Remove,
            )
            .expect("remove");

        assert_eq!(
            change,
            ManualTagsChange {
                changed: 0,
                kept_by_decision: 0,
                unchanged: 2
            }
        );
        assert!(session.rules_snapshot().manual_tags.is_empty());
        assert_eq!(use_case.rule_store.save_count(), 0);
    }

    #[test]
    fn a_repeated_remove_is_unchanged() {
        let mut session = tagged_session(&["a"], &["a"], StoredRules::default());
        let use_case = SetManualTags::new(InMemoryRuleStore::new());
        use_case
            .execute(&mut session, &ids(&["a"]), &framework(), TagMode::Remove)
            .expect("first remove");

        let change = use_case
            .execute(&mut session, &ids(&["a"]), &framework(), TagMode::Remove)
            .expect("second remove");

        assert_eq!(change.unchanged, 1);
        assert_eq!(use_case.rule_store.save_count(), 1);
    }

    #[test]
    fn removing_a_tag_a_decision_adds_changes_nothing_and_writes_nothing() {
        let mut session = decided_session(&["d"], &[], &["d"], StoredRules::default());
        assert!(has_tag(&session, "d"), "the decision shows the tag");
        let use_case = SetManualTags::new(InMemoryRuleStore::new());

        let change = use_case
            .execute(&mut session, &ids(&["d"]), &framework(), TagMode::Remove)
            .expect("remove");

        assert_eq!(
            change,
            ManualTagsChange {
                changed: 0,
                kept_by_decision: 1,
                unchanged: 0
            }
        );
        assert!(session.rules_snapshot().manual_tags.is_empty());
        assert_eq!(use_case.rule_store.save_count(), 0);
        assert!(has_tag(&session, "d"));
    }

    #[test]
    fn repeated_removes_of_a_decision_tag_never_add_overrides() {
        let mut session = decided_session(&["d"], &["d"], &["d"], StoredRules::default());
        let use_case = SetManualTags::new(InMemoryRuleStore::new());

        for _ in 0..3 {
            use_case
                .execute(&mut session, &ids(&["d"]), &framework(), TagMode::Remove)
                .expect("remove");
        }

        assert!(session.rules_snapshot().manual_tags.is_empty());
        assert_eq!(use_case.rule_store.save_count(), 0);
    }

    #[test]
    fn a_decision_kept_mod_keeps_its_own_add_override_in_a_mixed_selection() {
        // d: decision only; e: decision plus its own Add; i: inferred only; m: manual Add only.
        let mut session = decided_session(
            &["d", "e", "i", "m"],
            &["i"],
            &["d", "e"],
            StoredRules {
                manual_tags: vec![manual("e", TagMode::Add), manual("m", TagMode::Add)],
                ..StoredRules::default()
            },
        );
        let use_case = SetManualTags::new(InMemoryRuleStore::new());

        let change = use_case
            .execute(
                &mut session,
                &ids(&["d", "e", "i", "m"]),
                &framework(),
                TagMode::Remove,
            )
            .expect("remove");

        assert_eq!(
            change,
            ManualTagsChange {
                changed: 2,
                kept_by_decision: 2,
                unchanged: 0
            }
        );
        assert_eq!(
            overrides(&session),
            vec![
                ("e".to_string(), TagMode::Add),
                ("i".to_string(), TagMode::Remove)
            ]
        );
        assert_eq!(use_case.rule_store.save_count(), 1);
    }

    #[test]
    fn a_failed_save_rolls_every_override_back() {
        let mut session = session_fixture(&["a", "b"]);
        let store = InMemoryRuleStore::new();
        store.fail_next_save();
        let use_case = SetManualTags::new(store);

        let result = use_case.execute(&mut session, &ids(&["a", "b"]), &framework(), TagMode::Add);

        assert!(result.is_err());
        assert!(!has_tag(&session, "a") && !has_tag(&session, "b"));
        assert!(session.rules_snapshot().manual_tags.is_empty());
    }

    #[test]
    fn a_failed_save_does_not_re_sort_after_current_was_replaced() {
        // `d` needs one of x or y; which one the sorter picks follows the Current order, and a
        // reorder of Current under Rebuild leaves Suggested stale on purpose. A rollback that
        // re-sorted would pick up the new Current and move Suggested.
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("x")
            .mod_("y")
            .mod_("d")
            .any_of("d", "Asm", &["x", "y"], false)
            .build();
        let mut session = Session::new(
            paths(),
            report,
            Vec::new(),
            SourceIndex::default(),
            StoredRules {
                settings: Settings {
                    tie_break: TieBreak::Rebuild,
                    ..Settings::default()
                },
                ..StoredRules::default()
            },
            DecisionSet::new(),
            mods_config(&["x", "y", "d"]),
            Vec::new(),
            Vec::new(),
        );
        session.set_current_order(LoadOrder::new(
            ["y", "x", "d"].iter().map(|id| ModId::new(*id)).collect(),
        ));
        let suggested_before = session.orders().suggested.as_slice().to_vec();
        let store = InMemoryRuleStore::new();
        store.fail_next_save();

        let result = SetManualTags::new(store).execute(
            &mut session,
            &ids(&["x"]),
            &framework(),
            TagMode::Add,
        );

        assert!(result.is_err());
        assert_eq!(session.orders().suggested.as_slice(), suggested_before);
    }

    #[test]
    fn a_tag_only_change_leaves_the_sort_and_both_orders_unchanged() {
        let mut session = session_fixture(&["a", "b", "c"]);
        let sorted_before = session.sort_outcome().order.as_slice().to_vec();
        let current_before = session.orders().current.as_slice().to_vec();
        let suggested_before = session.orders().suggested.as_slice().to_vec();
        let use_case = SetManualTags::new(InMemoryRuleStore::new());

        use_case
            .execute(
                &mut session,
                &ids(&["a", "b", "c"]),
                &framework(),
                TagMode::Add,
            )
            .expect("bulk tag");

        assert_eq!(session.sort_outcome().order.as_slice(), sorted_before);
        assert_eq!(session.orders().current.as_slice(), current_before);
        assert_eq!(session.orders().suggested.as_slice(), suggested_before);
    }

    #[test]
    fn a_bulk_tag_refreshes_the_ledger_that_was_already_built() {
        let mut session = tagged_session(&["a", "b"], &["a"], StoredRules::default());
        let has_finding_for_a = |session: &mut Session| {
            session
                .ledger(OrderSource::Suggested)
                .entries
                .iter()
                .any(|entry| {
                    matches!(
                        &entry.key,
                        FindingKey::TagInferred { mod_id, .. } if *mod_id == ModId::new("a")
                    )
                })
        };
        assert!(
            has_finding_for_a(&mut session),
            "the ledger starts with the inferred tag"
        );

        SetManualTags::new(InMemoryRuleStore::new())
            .execute(&mut session, &ids(&["a"]), &framework(), TagMode::Remove)
            .expect("remove");

        assert!(
            !has_finding_for_a(&mut session),
            "a ledger kept from before the removal would still report the tag"
        );
    }

    #[test]
    fn the_single_mod_use_case_still_replaces_an_override() {
        let mut session = session_fixture(&["a"]);
        let bulk = SetManualTags::new(InMemoryRuleStore::new());
        bulk.execute(&mut session, &ids(&["a"]), &framework(), TagMode::Add)
            .expect("add");

        crate::use_cases::SetManualTag::new(InMemoryRuleStore::new())
            .execute(&mut session, ModId::new("a"), framework(), TagMode::Remove)
            .expect("single remove");

        assert!(!has_tag(&session, "a"));
    }
}
