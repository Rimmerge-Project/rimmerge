//! [`scoped`]: derives a compat patch's ledger from the profile's. Pure
//! and `O(entries)`: no XML,
//! no re-running the sorter, no re-extracting findings — every entry it
//! keeps is a filtered, rewritten copy of one already in `profile`.

use crate::domain::{
    Action, Alternative, Confidence, DecisionSet, Ledger, LedgerStats, PatchScope, Rationale,
    Resolution, ResolutionStatus, ScopeMembership, Suggestion,
};

/// Everything [`scoped`] needs: the profile ledger to derive from, the
/// patch's scope and its own decisions, and the confidence threshold
/// separating `Auto` from `NeedsInput` in the derived ledger.
///
/// Every field is a reference or a `Copy` value, so this itself is `Copy`.
#[derive(Clone, Copy)]
pub struct ScopedLedgerInput<'a> {
    /// The profile ledger, built for the same
    /// [`crate::domain::OrderSource`] the patch's preview should use.
    pub profile: &'a Ledger,
    /// The patch's scope.
    pub scope: &'a PatchScope,
    /// The patch's own decisions — never the profile's.
    pub decisions: &'a DecisionSet,
    /// The confidence threshold separating `Auto` from `NeedsInput`.
    pub threshold: Confidence,
}

/// Only `Merge`/`ShipAsset` alternatives survive rewriting — the only two
/// actions a compat patch can actually publish.
fn is_patchable(action: &Action) -> bool {
    matches!(action, Action::Merge { .. } | Action::ShipAsset { .. })
}

/// Rewrites a profile suggestion into the patch's vocabulary: `Ignore`
/// ("not addressed by this patch") at the same confidence (so the derived
/// ledger's own ordering still surfaces the least certain conflicts
/// first), offering only the alternatives a patch could actually carry
/// out, in the order the profile offered them (`Merge` already leads for
/// a contested collision). `Ignore` is always the rewritten action: a
/// compat patch never auto-applies a `Merge`/`ShipAsset` (those write a
/// mod folder), and every other action (`Accept`, `PreferWinner`,
/// `Reorder`, ...) is meaningless in a publishable patch, which only ever
/// *ships* field-level operations or stays silent on a finding.
fn rewrite_suggestion(profile_suggestion: &Suggestion) -> Suggestion {
    Suggestion {
        action: Action::Ignore,
        confidence: profile_suggestion.confidence,
        rationale: Rationale::NotAddressedByPatch,
        alternatives: profile_suggestion
            .alternatives
            .iter()
            .filter(|alternative| is_patchable(&alternative.action))
            .cloned()
            .collect::<Vec<Alternative>>(),
    }
}

/// Derives a patch's ledger from the profile's: keeps entries the scope
/// admits ([`ScopeMembership::Full`]/[`ScopeMembership::Partial`], never
/// [`ScopeMembership::Outside`]), overlays the patch's own decisions in
/// place of the profile's, rewrites each suggestion into the patch's
/// vocabulary, and re-derives status and stats from the patch's decisions
/// alone — the profile's own status on a kept key is never consulted.
#[must_use]
pub fn scoped(input: &ScopedLedgerInput<'_>) -> Ledger {
    let ScopedLedgerInput {
        profile,
        scope,
        decisions,
        threshold,
    } = *input;

    let mut entries = Vec::new();
    let mut stats = LedgerStats::default();

    for entry in &profile.entries {
        let membership = scope.membership(&entry.key);
        if matches!(membership, ScopeMembership::Outside) {
            continue;
        }

        let suggestion = rewrite_suggestion(&entry.suggestion);
        let decision = decisions.get(&entry.key).cloned();
        let status = match &decision {
            Some(_) => ResolutionStatus::UserOverridden,
            None if suggestion.confidence.meets(threshold) => ResolutionStatus::Auto,
            None => ResolutionStatus::NeedsInput,
        };
        let effective = decision
            .as_ref()
            .map_or_else(|| suggestion.action.clone(), |d| d.action.clone());

        match status {
            ResolutionStatus::Auto => stats.auto += 1,
            ResolutionStatus::NeedsInput => stats.needs_input += 1,
            ResolutionStatus::UserOverridden => stats.overridden += 1,
        }
        if entry.resolved_by_suggested == Some(true) {
            stats.resolved_by_suggested += 1;
        }

        entries.push(Resolution {
            key: entry.key.clone(),
            finding: entry.finding.clone(),
            suggestion,
            status,
            effective,
            decision,
            resolved_by_suggested: entry.resolved_by_suggested,
            // `rim-session` overlays this from the patch's own preview
            // slot, exactly as it does for the profile ledger — this crate
            // has no XML to build a preview from either way.
            merge: None,
            structural_guard_field: None,
            scope: Some(membership),
        });
    }

    Ledger {
        source: profile.source,
        entries,
        stats,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use jiff::Timestamp;
    use rim_analyzer::domain::{LoadOrder, Mod, ModId};

    use super::*;
    use crate::domain::{
        Decision, DefKey, FindingKey, OrderSource, PatchScope, RuleSet, SorterOverrides, Tagging,
    };
    use crate::ledger::build::{BuildLedgerInput, build};
    use crate::sort::{EnforcedLayers, SortInput, TieBreak, sort};

    fn decision(key: FindingKey, action: Action) -> Decision {
        Decision {
            key,
            action,
            note: None,
            decided_at: Timestamp::UNIX_EPOCH,
        }
    }

    fn wall_key() -> FindingKey {
        FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
            },
            owners: [ModId::new("a"), ModId::new("b"), ModId::new("c")]
                .into_iter()
                .collect(),
        }
    }

    fn door_key() -> FindingKey {
        FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Door".to_string(),
            },
            owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
        }
    }

    fn ghost_key() -> FindingKey {
        FindingKey::MissingMod {
            mod_id: ModId::new("ghost"),
        }
    }

    fn threshold(percent: u8) -> Confidence {
        Confidence::new(percent).unwrap_or_else(|_| unreachable!())
    }

    /// A three-mod-owned `DefOverride` (`Wall`, scope only covers two of its
    /// three owners -> `Partial`), a two-mod `DefOverride` fully inside
    /// scope (`Door` -> `Full`), and a `MissingMod` (always `Outside`) —
    /// exactly the shapes a scoped ledger must handle. The *profile*'s own
    /// decision set carries an `Accept` on `Door` (so its ledger entry
    /// shows `UserOverridden`), deliberately different from whatever the
    /// patch's own decisions say, to prove `scoped` derives status from the
    /// patch's decisions alone.
    fn profile_ledger() -> Ledger {
        let report = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .mod_("b")
            .mod_("c")
            .def_override("ThingDef", "Wall", &["a", "b", "c"])
            .def_override("ThingDef", "Door", &["a", "b"])
            .missing_mod("ghost")
            .build();
        let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("b"), ModId::new("c")]);
        let sort_outcome = sort(&SortInput {
            report: &report,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            overrides: &SorterOverrides::default(),
            current: &current,
            enforce: EnforcedLayers::default(),
            tie_break: TieBreak::PreserveCurrent,
        });
        let mods_by_id: BTreeMap<ModId, &Mod> =
            report.mods.iter().map(|m| (m.id.clone(), m)).collect();

        let mut profile_decisions = DecisionSet::new();
        profile_decisions
            .insert(decision(door_key(), Action::Accept))
            .unwrap_or_else(|never| match never {});

        build(&BuildLedgerInput {
            report: &report,
            sort_outcome: &sort_outcome,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            decisions: &profile_decisions,
            threshold: threshold(80),
            source: OrderSource::Current,
            current: &current,
            suggested: &sort_outcome.order,
            mods_by_id: &mods_by_id,
            show_dangling_def_references: false,
        })
    }

    fn ab_scope() -> PatchScope {
        PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap_or_else(|_| unreachable!())
    }

    fn entry<'a>(ledger: &'a Ledger, key: &FindingKey) -> &'a Resolution {
        ledger
            .entries
            .iter()
            .find(|e| &e.key == key)
            .unwrap_or_else(|| panic!("expected {key} in the scoped ledger"))
    }

    #[test]
    fn outside_entries_are_dropped() {
        let profile = profile_ledger();
        let scope = ab_scope();
        let derived = scoped(&ScopedLedgerInput {
            profile: &profile,
            scope: &scope,
            decisions: &DecisionSet::new(),
            threshold: threshold(80),
        });

        assert!(!derived.entries.iter().any(|e| e.key == ghost_key()));
    }

    #[test]
    fn kept_entries_carry_their_scope_membership() {
        let profile = profile_ledger();
        let scope = ab_scope();
        let derived = scoped(&ScopedLedgerInput {
            profile: &profile,
            scope: &scope,
            decisions: &DecisionSet::new(),
            threshold: threshold(80),
        });

        assert_eq!(
            entry(&derived, &wall_key()).scope,
            Some(ScopeMembership::Partial {
                outside: [ModId::new("c")].into_iter().collect()
            })
        );
        assert_eq!(
            entry(&derived, &door_key()).scope,
            Some(ScopeMembership::Full)
        );
    }

    #[test]
    fn suggestion_is_rewritten_to_ignore_with_only_patchable_alternatives() {
        let profile = profile_ledger();
        let scope = ab_scope();
        let derived = scoped(&ScopedLedgerInput {
            profile: &profile,
            scope: &scope,
            decisions: &DecisionSet::new(),
            threshold: threshold(80),
        });

        let wall = entry(&derived, &wall_key());
        assert_eq!(wall.suggestion.action, Action::Ignore);
        assert_eq!(wall.suggestion.rationale, Rationale::NotAddressedByPatch);
        // The profile's own suggestion for an unexplained def override
        // offers three `PreferWinner`s plus one `Merge` — only the `Merge`
        // survives.
        assert_eq!(wall.suggestion.alternatives.len(), 1);
        assert!(matches!(
            wall.suggestion.alternatives[0].action,
            Action::Merge { .. }
        ));
    }

    #[test]
    fn confidence_is_preserved_from_the_profile_suggestion() {
        let profile = profile_ledger();
        let scope = ab_scope();
        let derived = scoped(&ScopedLedgerInput {
            profile: &profile,
            scope: &scope,
            decisions: &DecisionSet::new(),
            threshold: threshold(80),
        });

        let profile_wall = entry(&profile, &wall_key());
        let scoped_wall = entry(&derived, &wall_key());
        assert_eq!(
            scoped_wall.suggestion.confidence,
            profile_wall.suggestion.confidence
        );
    }

    #[test]
    fn status_follows_the_patchs_own_decisions_not_the_profiles() {
        let profile = profile_ledger();
        // The profile ledger has an `Accept` decision on `door_key` — its
        // own status is `UserOverridden`.
        assert_eq!(
            entry(&profile, &door_key()).status,
            ResolutionStatus::UserOverridden
        );

        let scope = ab_scope();
        let mut patch_decisions = DecisionSet::new();
        patch_decisions
            .insert(decision(wall_key(), Action::Ignore))
            .unwrap_or_else(|never| match never {});
        // No decision on `door_key` in the patch's own set.

        let derived = scoped(&ScopedLedgerInput {
            profile: &profile,
            scope: &scope,
            decisions: &patch_decisions,
            threshold: threshold(80),
        });

        assert_eq!(
            entry(&derived, &wall_key()).status,
            ResolutionStatus::UserOverridden
        );
        assert_eq!(
            entry(&derived, &wall_key())
                .decision
                .as_ref()
                .map(|d| &d.action),
            Some(&Action::Ignore)
        );
        // `Door`'s rewritten suggestion is `Ignore` at the def override's
        // 60% confidence — below the 80% threshold, so `NeedsInput`,
        // *not* the profile's `UserOverridden`.
        assert_eq!(
            entry(&derived, &door_key()).status,
            ResolutionStatus::NeedsInput
        );
    }

    #[test]
    fn stats_are_recomputed_over_the_scoped_entries() {
        let profile = profile_ledger();
        let scope = ab_scope();
        let mut patch_decisions = DecisionSet::new();
        patch_decisions
            .insert(decision(wall_key(), Action::Ignore))
            .unwrap_or_else(|never| match never {});

        let derived = scoped(&ScopedLedgerInput {
            profile: &profile,
            scope: &scope,
            decisions: &patch_decisions,
            threshold: threshold(80),
        });

        assert_eq!(derived.entries.len(), 2, "only Wall and Door are admitted");
        assert_eq!(derived.stats.overridden, 1);
        assert_eq!(derived.stats.needs_input, 1);
        assert_eq!(derived.stats.auto, 0);
    }

    #[test]
    fn source_is_carried_over_from_the_profile_ledger() {
        let profile = profile_ledger();
        let scope = ab_scope();
        let derived = scoped(&ScopedLedgerInput {
            profile: &profile,
            scope: &scope,
            decisions: &DecisionSet::new(),
            threshold: threshold(80),
        });

        assert_eq!(derived.source, profile.source);
    }

    // -- proptests: idempotence and scope-monotonicity ---------------------
    //
    // A fixed, five-mod report (`wide_profile_ledger`) with `DefOverride`s
    // spread across different owner subsets — `Wall` (a,b,c), `Door`
    // (a,b), `Roof` (c,d,e) — a `PatchCollision` (`Lamp`, owned by `a` and
    // a `_steam` copy of `b`, exercising the owner-set rule's `ModId::base`
    // normalization), a pair-kind finding (`IncompatiblePair` between `d`
    // and `e`), plus the always-`Outside` `MissingMod` `"ghost"` — so a
    // randomly chosen scope mask actually produces a mix of
    // `Full`/`Partial`/`Outside` memberships across every finding shape
    // [`PatchScope::membership`] handles differently, not just
    // `DefOverride`, to exercise.
    //
    // `scoping_is_idempotent_prop` below needs no hand-written unit-test
    // twin over the narrower `profile_ledger()` fixture: the property holds
    // for every mask the proptest tries, including that fixture's, so both
    // would be pure duplication.

    fn wide_profile_ledger() -> Ledger {
        let report = crate::test_support::ReportBuilder::new()
            .mod_("a")
            .mod_("b")
            .mod_("c")
            .mod_("d")
            .mod_("e")
            .def_override("ThingDef", "Wall", &["a", "b", "c"])
            .def_override("ThingDef", "Door", &["a", "b"])
            .def_override("ThingDef", "Roof", &["c", "d", "e"])
            .patch_collision("ThingDef", "Lamp", &["a", "b_steam"])
            .incompatible_pair("d", "e")
            .missing_mod("ghost")
            .build();
        let current = LoadOrder::new(vec![
            ModId::new("a"),
            ModId::new("b"),
            ModId::new("c"),
            ModId::new("d"),
            ModId::new("e"),
        ]);
        let sort_outcome = sort(&SortInput {
            report: &report,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            overrides: &SorterOverrides::default(),
            current: &current,
            enforce: EnforcedLayers::default(),
            tie_break: TieBreak::PreserveCurrent,
        });
        let mods_by_id: BTreeMap<ModId, &Mod> =
            report.mods.iter().map(|m| (m.id.clone(), m)).collect();
        build(&BuildLedgerInput {
            report: &report,
            sort_outcome: &sort_outcome,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            decisions: &DecisionSet::new(),
            threshold: threshold(80),
            source: OrderSource::Current,
            current: &current,
            suggested: &sort_outcome.order,
            mods_by_id: &mods_by_id,
            show_dangling_def_references: false,
        })
    }

    const MOD_IDS: [&str; 5] = ["a", "b", "c", "d", "e"];

    fn scope_from_mask(mask: &[bool; 5]) -> Option<PatchScope> {
        let members: Vec<ModId> = MOD_IDS
            .iter()
            .zip(mask.iter())
            .filter(|(_, on)| **on)
            .map(|(id, _)| ModId::new(*id))
            .collect();
        PatchScope::new(members).ok()
    }

    fn arb_mask() -> impl proptest::strategy::Strategy<Value = [bool; 5]> {
        proptest::array::uniform5(proptest::bool::ANY)
    }

    proptest::proptest! {
        /// Re-scoping an already-scoped ledger (same scope, same decisions)
        /// changes nothing further: every entry `scoped` kept is
        /// `Full`/`Partial` for that same scope, and the suggestion
        /// rewrite/status derivation are pure functions of the
        /// (already-rewritten) suggestion and the decision set alone.
        #[test]
        fn scoping_is_idempotent_prop(mask in arb_mask()) {
            proptest::prop_assume!(scope_from_mask(&mask).is_some());
            let scope = scope_from_mask(&mask).unwrap_or_else(|| unreachable!());
            let profile = wide_profile_ledger();
            let decisions = DecisionSet::new();

            let once = scoped(&ScopedLedgerInput {
                profile: &profile,
                scope: &scope,
                decisions: &decisions,
                threshold: threshold(80),
            });
            let twice = scoped(&ScopedLedgerInput {
                profile: &once,
                scope: &scope,
                decisions: &decisions,
                threshold: threshold(80),
            });

            proptest::prop_assert_eq!(once.entries.len(), twice.entries.len());
            for entry in &once.entries {
                let again = twice.entries.iter().find(|e| e.key == entry.key);
                proptest::prop_assert!(again.is_some(), "{} must survive re-scoping", entry.key);
                let again = again.unwrap_or_else(|| unreachable!());
                proptest::prop_assert_eq!(&again.suggestion, &entry.suggestion);
                proptest::prop_assert_eq!(again.status, entry.status);
                proptest::prop_assert_eq!(&again.scope, &entry.scope);
            }
        }

        /// scope ⊆ scope' implies scoped(scope) findings ⊆ scoped(scope')
        /// findings: adding members to a scope never drops a key the
        /// smaller scope already admitted, since `PatchScope::membership`
        /// only ever turns `Outside` into `Partial`/`Full` or `Partial`
        /// into `Full` as members are added, never the reverse.
        #[test]
        fn scope_growth_never_drops_an_admitted_finding(base_mask in arb_mask(), extra_mask in arb_mask()) {
            proptest::prop_assume!(scope_from_mask(&base_mask).is_some());
            let base_scope = scope_from_mask(&base_mask).unwrap_or_else(|| unreachable!());
            let grown_mask: [bool; 5] = std::array::from_fn(|i| base_mask[i] || extra_mask[i]);
            // `grown_mask` is a superset of `base_mask`, which already has
            // >= 2 members, so this can never fail `PatchScope::new`.
            let grown_scope = scope_from_mask(&grown_mask).unwrap_or_else(|| unreachable!());

            let profile = wide_profile_ledger();
            let decisions = DecisionSet::new();
            let base_ledger = scoped(&ScopedLedgerInput {
                profile: &profile,
                scope: &base_scope,
                decisions: &decisions,
                threshold: threshold(80),
            });
            let grown_ledger = scoped(&ScopedLedgerInput {
                profile: &profile,
                scope: &grown_scope,
                decisions: &decisions,
                threshold: threshold(80),
            });

            let grown_keys: std::collections::BTreeSet<_> =
                grown_ledger.entries.iter().map(|e| e.key.clone()).collect();
            for entry in &base_ledger.entries {
                proptest::prop_assert!(grown_keys.contains(&entry.key),
                    "{} admitted by the smaller scope must survive growing it",
                    entry.key
                );
            }
        }
    }
}
