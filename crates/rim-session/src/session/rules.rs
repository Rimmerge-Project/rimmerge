//! [`Session`]'s rule editing: upsert/delete, promotion of imported rules, manual tags, profile
//! import, and the rules snapshot/restore pair.

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    ManualTag, PairRule, PlacementRule, PromotedRuleKey, Rule, RuleOrigin, Tag, TagMode,
};

use super::Session;
use crate::ports::StoredRules;
use crate::settings::{Settings, is_imported_origin};

/// A rule's natural key, used by [`Session::upsert_rule`]/
/// [`Session::delete_rule`] since none of [`Rule`]'s shapes carry an id
/// field of their own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleKey {
    /// A [`rim_resolve::domain::PairRule`], keyed by its `(after, before)`
    /// pair.
    Pair {
        /// The dependent side.
        after: ModId,
        /// The dependency side.
        before: ModId,
    },
    /// A [`rim_resolve::domain::PlacementRule`], keyed by the pinned mod.
    Placement {
        /// The pinned mod.
        mod_id: ModId,
    },
    /// An [`rim_resolve::domain::IncompatibleRule`], keyed by its
    /// unordered pair.
    Incompatible {
        /// One of the two mods.
        a: ModId,
        /// The other.
        b: ModId,
    },
}

pub(crate) fn rule_key(rule: &Rule) -> RuleKey {
    match rule {
        Rule::Pair(r) => RuleKey::Pair {
            after: r.after.clone(),
            before: r.before.clone(),
        },
        Rule::Placement(r) => RuleKey::Placement {
            mod_id: r.mod_id.clone(),
        },
        Rule::Incompatible(r) => RuleKey::Incompatible {
            a: r.a.clone(),
            b: r.b.clone(),
        },
    }
}

/// [`PromotedRuleKey`] (`rim_resolve::domain`, application-layer-agnostic)
/// to [`RuleKey`] (this crate's own, `Session::promote_imported_rule`'s
/// actual parameter) — `Action::PromoteRule`'s payload arrives in the
/// former shape since `rim-resolve` stays independent of `rim-session`.
pub(super) fn rule_key_from_promoted(rule: &PromotedRuleKey) -> RuleKey {
    match rule {
        PromotedRuleKey::Pair { after, before } => RuleKey::Pair {
            after: after.clone(),
            before: before.clone(),
        },
        PromotedRuleKey::Placement { mod_id } => RuleKey::Placement {
            mod_id: mod_id.clone(),
        },
    }
}

impl Session {
    /// Removes every rule matching `key`, restricted to `origin` when
    /// given (every origin when `None`). After
    /// [`Session::promote_imported_rule`], two rows share one [`RuleKey`] —
    /// an imported rule and its promoted `UserDecision` copy — so a
    /// key-only removal would make both [`Session::upsert_rule`] and
    /// [`Session::delete_rule`] silently wipe *both* rows instead of just
    /// the one the caller meant to change.
    fn remove_rule_matching(&mut self, key: &RuleKey, origin: Option<RuleOrigin>) {
        let matches_origin = |o: RuleOrigin| origin.is_none_or(|only| only == o);
        match key {
            RuleKey::Pair { after, before } => self.rules.pairs.retain(|r| {
                !(r.after == *after && r.before == *before && matches_origin(r.origin))
            }),
            RuleKey::Placement { mod_id } => self
                .rules
                .placements
                .retain(|r| !(r.mod_id == *mod_id && matches_origin(r.origin))),
            RuleKey::Incompatible { a, b } => self.rules.incompatibles.retain(|r| {
                !(((r.a == *a && r.b == *b) || (r.a == *b && r.b == *a))
                    && matches_origin(r.origin))
            }),
        }
    }

    /// Adds `rule`, replacing any existing rule with the same [`RuleKey`]
    /// *and* the same origin as `rule` itself, then recomputes the sort —
    /// origin-scoped so upserting a promoted (`UserDecision`) copy's
    /// comment, say, never also deletes the imported original sharing its
    /// key (see [`Session::remove_rule_matching`]'s own doc comment).
    pub fn upsert_rule(&mut self, rule: Rule) {
        let key = rule_key(&rule);
        self.remove_rule_matching(&key, Some(rule.origin()));
        match rule {
            Rule::Pair(r) => self.rules.pairs.push(r),
            Rule::Placement(r) => self.rules.placements.push(r),
            Rule::Incompatible(r) => self.rules.incompatibles.push(r),
        }
        self.recompute_sort();
    }

    /// Removes the rule matching `key`, restricted to `origin` when given,
    /// then recomputes the sort. `origin: None` removes every rule at
    /// `key` regardless of origin — today's only caller
    /// (`apps/desktop`'s rules page, via [`crate::use_cases::DeleteRule`])
    /// doesn't yet know a promoted rule's own origin to pass one; see
    /// [`Session::remove_rule_matching`]'s own doc comment for why a
    /// `Some` caller matters once one exists.
    pub fn delete_rule(&mut self, key: &RuleKey, origin: Option<RuleOrigin>) {
        self.remove_rule_matching(key, origin);
        self.recompute_sort();
    }

    /// "Promote to user rule":
    /// copies the imported pair or placement rule named by `key` into a
    /// new `UserDecision`-origin rule with the same key, then recomputes
    /// the sort. The imported original is left untouched — still listed,
    /// still subject to its own toggle — so the promoted copy survives
    /// both [`Settings::use_imported_pairs`]/[`Settings::use_imported_placements`]
    /// and a later re-import ([`Session::apply_import`] only ever removes
    /// an imported-origin rule, never a `UserDecision` one). Demoting a
    /// promoted rule (or deleting any other) is the existing
    /// [`Session::delete_rule`] — there is no separate "demote".
    ///
    /// Returns whether a copy was actually added — `false` (a no-op) when
    /// no imported rule matches `key`, when `key` already has a promoted
    /// (`UserDecision`) copy, or when `key` is
    /// [`RuleKey::Incompatible`] (promotion only covers pairs and placements —
    /// there is nothing ordering-related to promote for an
    /// incompatibility).
    pub fn promote_imported_rule(&mut self, key: &RuleKey) -> bool {
        let promoted = match key {
            RuleKey::Pair { after, before } => {
                let already_has_user_rule = self.rules.pairs.iter().any(|r| {
                    r.after == *after && r.before == *before && !is_imported_origin(r.origin)
                });
                let imported_source = self
                    .rules
                    .pairs
                    .iter()
                    .find(|r| {
                        r.after == *after && r.before == *before && is_imported_origin(r.origin)
                    })
                    .cloned();
                match (already_has_user_rule, imported_source) {
                    (false, Some(source)) => {
                        self.rules.pairs.push(PairRule {
                            after: source.after,
                            before: source.before,
                            origin: RuleOrigin::UserDecision,
                            comment: source.comment,
                            // Carried through, not reset to `false`: an
                            // imported row is never expected to carry
                            // `overrides_declared: true` in practice (the
                            // field is honoured only for `UserDecision`
                            // rows — see its own doc comment), but a
                            // promote is a copy, and a copy that silently
                            // dropped a field the source happened to
                            // carry would be a real, if currently
                            // unreachable, bug.
                            overrides_declared: source.overrides_declared,
                        });
                        true
                    }
                    _ => false,
                }
            }
            RuleKey::Placement { mod_id } => {
                let already_has_user_rule = self
                    .rules
                    .placements
                    .iter()
                    .any(|r| r.mod_id == *mod_id && !is_imported_origin(r.origin));
                let imported_source = self
                    .rules
                    .placements
                    .iter()
                    .find(|r| r.mod_id == *mod_id && is_imported_origin(r.origin))
                    .cloned();
                match (already_has_user_rule, imported_source) {
                    (false, Some(source)) => {
                        self.rules.placements.push(PlacementRule {
                            mod_id: source.mod_id,
                            placement: source.placement,
                            origin: RuleOrigin::UserDecision,
                            comment: source.comment,
                        });
                        true
                    }
                    _ => false,
                }
            }
            RuleKey::Incompatible { .. } => false,
        };
        if promoted {
            self.recompute_sort();
        }
        promoted
    }

    /// Sets (or replaces) a manual tag override, then recomputes the
    /// sort.
    pub fn set_manual_tag(&mut self, mod_id: ModId, tag: Tag, mode: TagMode) {
        self.rules
            .manual_tags
            .retain(|m| !(m.mod_id == mod_id && m.tag == tag));
        self.rules.manual_tags.push(ManualTag { mod_id, tag, mode });
        self.recompute_sort();
    }

    /// Replaces imported rules **per origin, independently** —
    /// `userRules.json` is read live off a RimSort install rather than
    /// copied into Rimmerge's own storage, so an import can legitimately
    /// carry the two fetched databases without a user-rules path at all
    /// (`rimmerge db refresh && rimmerge import --from-cache`, the most
    /// natural command pair). Clearing every imported-origin rule
    /// unconditionally and re-adding whatever `imported` carries would be
    /// correct only if every caller always imported all three files
    /// together; under the scenario above it would silently delete the
    /// user's own hand-added `RimSortUser` rules, which must survive
    /// untouched. See this method's own test module for the regression
    /// test, written failing-first against exactly that.
    ///
    /// `imported.user_rules: Some(rules)` replaces every existing
    /// `RuleOrigin::RimSortUser` rule with `rules` (a re-import
    /// supersedes the last one rather than accumulating duplicates; an
    /// empty `Vec` is a real "this source was imported and found
    /// nothing active," distinct from `None`). `None` means that source
    /// was not part of this import, so its existing rules are left
    /// exactly as they are. The same per-origin rule applies to
    /// `community_rules`/[`RuleOrigin::RimSortCommunity`] and
    /// `steam_dependencies`/[`RuleOrigin::SteamDb`]. `RuleOrigin::UserDecision`
    /// rows — including a [`Self::promote_imported_rule`] copy — are
    /// never touched by any arm.
    pub fn apply_import(&mut self, imported: crate::ports::ImportedRules) {
        Self::replace_origin_rules(
            &mut self.rules,
            RuleOrigin::RimSortUser,
            imported.user_rules,
        );
        Self::replace_origin_rules(
            &mut self.rules,
            RuleOrigin::RimSortCommunity,
            imported.community_rules,
        );
        Self::replace_origin_rules(
            &mut self.rules,
            RuleOrigin::SteamDb,
            imported.steam_dependencies,
        );
        self.recompute_sort();
    }

    /// Replaces every rule of `origin` in `rules` with `replacement`'s
    /// contents — a no-op when `replacement` is `None` (that source
    /// wasn't part of this import). The shared "clear this one origin,
    /// then re-add" step every arm of [`Self::apply_import`] needs;
    /// deliberately keyed by exact `origin` equality, not
    /// [`is_imported_origin`] — that helper answers "is this rule from
    /// *any* imported source," the wrong question here, where each
    /// origin must be judged independently against its own field.
    ///
    /// **A rule whose own `origin` doesn't match `origin` is skipped, not
    /// stored.** Considered and rejected storing it anyway with only a
    /// `debug_assert!` as the guard: the failure is *not* symmetric across
    /// `apply_import`'s three arms (`RimSortUser` -> `RimSortCommunity` ->
    /// `SteamDb`, in that order). A rule mis-tagged with an origin that
    /// sorts *later* does get cleared by that later arm's own `retain` —
    /// but a rule mis-tagged with `RimSortUser` while stored under, say,
    /// `community_rules: Some([..])` survives every arm (`RimSortUser`'s
    /// own arm already ran), persists to `rules.json` as a `RimSortUser`
    /// row outranking community/SteamDB, and — because a later import
    /// legitimately carries `user_rules: None` — nothing ever clears it
    /// again. It becomes permanently indistinguishable from the user's
    /// own hand-added rule. Storing under the wrong origin is strictly
    /// worse than dropping the rule outright, precisely because of the
    /// precedence lattice this method exists to maintain. No caller can
    /// produce this today: `rim_io::RimSortImporter` always builds each
    /// `Vec<Rule>` through `rules_file::parse`/`steam_db::parse` with
    /// exactly the origin it's about to be stored under. A
    /// `debug_assert!` stays as the loud, zero-cost-in-release signal for
    /// a *future* caller that gets this wrong — matching this codebase's
    /// existing convention for a trusted-but-checkable precondition (see
    /// `rim_resolve::domain::merge`'s `PathSegment`/`ItemId::Key`
    /// `debug_assert!`s) — but it runs as its own pass over
    /// `replacement`, entirely *before* `retain`/`push` ever touch
    /// `rules`, so a debug build that does panic here never leaves
    /// `rules` half-cleared/half-repopulated.
    fn replace_origin_rules(
        rules: &mut StoredRules,
        origin: RuleOrigin,
        replacement: Option<Vec<Rule>>,
    ) {
        let Some(replacement) = replacement else {
            return;
        };
        for rule in &replacement {
            debug_assert!(
                rule.origin() == origin,
                "apply_import's caller must tag every rule in this arm's own Vec with {origin:?}; \
                 got {:?} — storing it under the wrong origin would make it indistinguishable \
                 from a UserDecision rule (or outrank one) once a later import legitimately \
                 omits this source",
                rule.origin()
            );
        }

        rules.pairs.retain(|r| r.origin != origin);
        rules.placements.retain(|r| r.origin != origin);
        rules.incompatibles.retain(|r| r.origin != origin);
        for rule in replacement {
            if rule.origin() != origin {
                // Release-mode fail-safe for the case the debug_assert
                // above catches loudly in a debug build: skip rather than
                // store under the wrong origin (see this method's own
                // doc comment for why that's the safer failure).
                continue;
            }
            match rule {
                Rule::Pair(r) => rules.pairs.push(r),
                Rule::Placement(r) => rules.placements.push(r),
                Rule::Incompatible(r) => rules.incompatibles.push(r),
            }
        }
    }

    /// Replaces the current settings, then recomputes the sort (the
    /// threshold changes `Auto`/`NeedsInput` status; the enforcement
    /// toggles change the sorter's own output).
    pub fn update_settings(&mut self, settings: Settings) {
        self.rules.settings = settings;
        self.recompute_sort();
    }

    /// A snapshot of every rule/setting currently in effect, for a caller
    /// that wants to roll back a rules-page mutation
    /// ([`Session::upsert_rule`]/[`Session::delete_rule`]/
    /// [`Session::set_manual_tag`]/[`Session::update_settings`]/
    /// [`Session::apply_import`]) if persisting it fails.
    #[must_use]
    pub fn rules_snapshot(&self) -> StoredRules {
        self.rules.clone()
    }

    /// Replaces every rule/setting with `rules` (restoring a
    /// [`Session::rules_snapshot`]) and recomputes the sort.
    pub fn restore_rules(&mut self, rules: StoredRules) {
        self.rules = rules;
        self.recompute_sort();
    }
}
