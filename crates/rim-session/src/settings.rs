//! [`Settings`]: user-configurable knobs that change what the sorter and
//! ledger produce, persisted alongside the rest of a profile's rules (see
//! [`crate::ports::StoredRules`]).
//!
//! **Network policy is not here.** The four fields that used to live on
//! this struct (`fetch_community_rules`/`fetch_steam_workshop`/
//! `allow_network_refresh`/`fetch_rimmerge_rules`) moved to
//! `crate::app_settings::NetworkPolicy`, an app-global setting (one per
//! machine, not per profile) — see that type's own doc comment for why.
//! `rim_io::rules::SettingsDto` still *reads* those four keys from an
//! existing `rules.json` for backward compatibility, but never writes
//! them again.

use rim_resolve::domain::{Confidence, Rule, RuleOrigin, RuleSet};
use rim_resolve::sort::{EnforcedLayers, TieBreak};

/// The confidence threshold separating `Auto` from `NeedsInput`, plus the
/// sorter strictness/evidence toggles (see
/// [`EnforcedLayers`](rim_resolve::sort::EnforcedLayers)), the tie-break,
/// and the imported-rule toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// The confidence threshold separating `Auto` from `NeedsInput`.
    pub threshold: Confidence,
    /// Which weak engine-edge layers the sorter treats as real ordering
    /// constraints versus merely advisory.
    pub enforce: EnforcedLayers,
    /// Merge-first suggestions for clean previews: when on, a
    /// `DefOverride`/`PatchCollision` finding whose merge preview turns out
    /// clean (or merely needs a field choice) has its ledger suggestion
    /// re-derived to lead with `Merge` — see
    /// [`rim_resolve::domain::redecide_for_clean_merge`]. Off leaves `Merge`
    /// only ever a user-chosen alternative.
    pub suggest_merge_when_clean: bool,
    /// Which base key an unconstrained mod's emission starts from.
    /// Default [`TieBreak::Rebuild`]: a list built from the mods' own
    /// evidence is the point of sorting; `PreserveCurrent` stays available
    /// for anyone who wants minimal disturbance from a list they already
    /// trust.
    pub tie_break: TieBreak,
    /// Whether imported (`RimSortUser`/`RimSortCommunity`/`SteamDb`)
    /// pair rules feed the sorter. **Default `true`**: on a large real
    /// install, over 80% of imported pairs are already redundant with a
    /// derived edge the sorter enforces regardless, and the rest are the
    /// same unreconstructable community knowledge
    /// `use_imported_placements` is trusted for — most of these rules *are*
    /// the evidence restated. A `UserDecision` pair — including a promoted
    /// copy, see [`crate::Session::promote_imported_rule`] — is never
    /// filtered by this toggle.
    pub use_imported_pairs: bool,
    /// Whether imported placement rules (`Top`/`Bottom` tier pins)
    /// feed the sorter. Default `true` — a placement is a tier statement
    /// no file-level fact can reconstruct (a frame-rate tuning mod's own
    /// "load last" convention, Example's own, ...), unlike a pair rule.
    pub use_imported_placements: bool,
    /// Whether the ledger surfaces `DanglingDefReference` findings (a name
    /// written at a recognized reference site that no active def of any
    /// type actually has). **Default `false`**: measured against a real,
    /// large install, tier 1's vote-based reference-site inference
    /// recovered only a minority of the logged `Could not resolve
    /// cross-reference` names, while also producing far more predictions
    /// than had a matching logged error — a false-positive volume well
    /// past what's usable, traced to self-identifying fields
    /// (`defName`/`identifier` on a nested object's own key, not a
    /// reference to another def) and a handful of coincidentally-voting
    /// free-text fields. Never affects the sorter — this finding carries
    /// no ordering edge at all.
    pub show_dangling_def_references: bool,
}

impl Default for Settings {
    /// Threshold 80, the basis of the initial confidence table.
    /// `enforce`/`EnforcedLayers::default()` is `soft`/`awareness` off,
    /// `inferred` on (see that type's own doc comment for why the third
    /// toggle's default differs from the other two).
    /// `suggest_merge_when_clean` defaults on (visible in the merge-mod
    /// page and the apply summary, and always reversible from the settings
    /// page). `tie_break` defaults to `Rebuild`, and
    /// `use_imported_pairs`/`use_imported_placements` both default on.
    /// `show_dangling_def_references` defaults off — see
    /// that field's own doc comment for the measured false-positive rate
    /// behind the default.
    fn default() -> Self {
        Self {
            threshold: Confidence::new(80)
                .unwrap_or_else(|_| unreachable!("80 is within Confidence's 0..=100 range")),
            enforce: EnforcedLayers::default(),
            suggest_merge_when_clean: true,
            tie_break: TieBreak::default(),
            use_imported_pairs: true,
            use_imported_placements: true,
            show_dangling_def_references: false,
        }
    }
}

/// Which settings produced a suggested order — surfaced by `apply
/// --dry-run`'s diff and the why-panel,
/// so a large disturbance — e.g. the first `Rebuild` of a list RimSort
/// produced — is explained rather than alarming
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortProvenance {
    /// Which base key an unconstrained mod's emission started from.
    pub tie_break: TieBreak,
    /// Whether imported pair rules fed the sorter.
    pub use_imported_pairs: bool,
    /// Whether imported placement rules fed the sorter.
    pub use_imported_placements: bool,
}

impl Settings {
    /// This settings snapshot's own [`SortProvenance`].
    #[must_use]
    pub fn sort_provenance(&self) -> SortProvenance {
        SortProvenance {
            tie_break: self.tie_break,
            use_imported_pairs: self.use_imported_pairs,
            use_imported_placements: self.use_imported_placements,
        }
    }
}

/// Whether `origin` names one of RimSort's three imported db files — never
/// [`RuleOrigin::UserDecision`]. Shared by [`filter_imported_rules`] and
/// [`crate::Session::promote_imported_rule`] (a pair/placement rule can
/// only be promoted from one of these origins, never from a user's own).
/// **Not** used by [`crate::Session::apply_import`]
/// — that method judges each
/// origin independently against its own `ImportedRules` field, which is
/// exactly the question this helper *can't* answer ("is this rule from
/// any imported source" is the wrong question once `user_rules`/
/// `community_rules`/`steam_dependencies` can each independently be
/// `None`); see `Session::replace_origin_rules`'s own doc comment.
#[must_use]
pub(crate) fn is_imported_origin(origin: RuleOrigin) -> bool {
    match origin {
        RuleOrigin::RimSortUser | RuleOrigin::RimSortCommunity | RuleOrigin::SteamDb => true,
        RuleOrigin::UserDecision => false,
    }
}

/// Applies the two imported-rule toggles to `rule_set`: drops every imported
/// (non-[`RuleOrigin::UserDecision`]) pair rule when `use_imported_pairs`
/// is `false`, and every imported placement rule when
/// `use_imported_placements` is `false`. Incompatibility rules and every
/// `UserDecision`-origin rule (a manual rule, or a promoted copy — see
/// [`crate::Session::promote_imported_rule`]) are never filtered — an
/// imported incompatible cannot move a mod and costs nothing to keep, so
/// there is no reason to drop it. It is stored and listed on the rules
/// page either way, but produces no finding yet: `FindingKey::
/// IncompatiblePair` is emitted from `report.incompatible_active_pairs`
/// (the analyzer's own About.xml-declared conflicts), and nothing in
/// `ledger::findings::extract` consumes `RuleSet::incompatibles()` — a
/// tracked gap, not a toggle exemption.
#[must_use]
pub fn filter_imported_rules(
    rule_set: RuleSet,
    use_imported_pairs: bool,
    use_imported_placements: bool,
) -> RuleSet {
    if use_imported_pairs && use_imported_placements {
        return rule_set;
    }
    RuleSet::new(
        rule_set
            .iter()
            .filter(|rule| match rule {
                Rule::Pair(pair) => use_imported_pairs || !is_imported_origin(pair.origin),
                Rule::Placement(placement) => {
                    use_imported_placements || !is_imported_origin(placement.origin)
                }
                Rule::Incompatible(_) => true,
            })
            .cloned()
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Placement, PlacementRule};

    use super::*;

    #[test]
    fn default_threshold_is_80_with_soft_and_awareness_off_but_inferred_on() {
        let settings = Settings::default();
        assert_eq!(settings.threshold.percent(), 80);
        assert!(!settings.enforce.soft);
        assert!(!settings.enforce.awareness);
        assert!(settings.enforce.inferred);
    }

    #[test]
    fn suggest_merge_when_clean_defaults_on() {
        assert!(Settings::default().suggest_merge_when_clean);
    }

    /// Both import toggles default on,
    /// alongside `Rebuild`.
    #[test]
    fn tie_break_defaults_to_rebuild_and_both_import_toggles_default_on() {
        let settings = Settings::default();
        assert_eq!(settings.tie_break, TieBreak::Rebuild);
        assert!(settings.use_imported_pairs);
        assert!(settings.use_imported_placements);
    }

    // Network/fetch-toggle defaults are covered by
    // `crate::app_settings::tests::network_policy_defaults_are_on_except_steam_workshop`
    // now that those fields live on `NetworkPolicy`, not `Settings`.

    fn pair(after: &str, before: &str, origin: RuleOrigin) -> Rule {
        Rule::Pair(rim_resolve::domain::PairRule {
            after: ModId::new(after),
            before: ModId::new(before),
            origin,
            comment: None,
            overrides_declared: false,
        })
    }

    fn placement(mod_id: &str, origin: RuleOrigin) -> Rule {
        Rule::Placement(PlacementRule {
            mod_id: ModId::new(mod_id),
            placement: Placement::Bottom,
            origin,
            comment: None,
        })
    }

    #[test]
    fn imported_pairs_are_dropped_when_the_toggle_is_off() {
        let set = RuleSet::new(vec![
            pair("a", "b", RuleOrigin::RimSortCommunity),
            pair("c", "d", RuleOrigin::UserDecision),
        ]);

        let filtered = filter_imported_rules(set, false, true);

        assert_eq!(filtered.pairs().count(), 1);
        assert_eq!(
            filtered.pairs().next().unwrap().origin,
            RuleOrigin::UserDecision
        );
    }

    #[test]
    fn imported_placements_are_kept_when_the_toggle_is_on() {
        let set = RuleSet::new(vec![placement("a", RuleOrigin::RimSortCommunity)]);

        let filtered = filter_imported_rules(set, false, true);

        assert_eq!(filtered.placements().count(), 1);
    }

    #[test]
    fn imported_placements_are_dropped_when_the_toggle_is_off() {
        let set = RuleSet::new(vec![
            placement("a", RuleOrigin::RimSortCommunity),
            placement("b", RuleOrigin::UserDecision),
        ]);

        let filtered = filter_imported_rules(set, true, false);

        assert_eq!(filtered.placements().count(), 1);
        assert_eq!(
            filtered.placements().next().unwrap().origin,
            RuleOrigin::UserDecision
        );
    }

    #[test]
    fn incompatible_rules_are_never_filtered() {
        let set = RuleSet::new(vec![Rule::Incompatible(
            rim_resolve::domain::IncompatibleRule {
                a: ModId::new("a"),
                b: ModId::new("b"),
                origin: RuleOrigin::RimSortCommunity,
            },
        )]);

        let filtered = filter_imported_rules(set, false, false);

        assert_eq!(filtered.incompatibles().count(), 1);
    }

    #[test]
    fn sort_provenance_mirrors_the_three_settings() {
        let settings = Settings {
            tie_break: TieBreak::PreserveCurrent,
            use_imported_pairs: true,
            use_imported_placements: false,
            ..Settings::default()
        };

        let provenance = settings.sort_provenance();

        assert_eq!(provenance.tie_break, TieBreak::PreserveCurrent);
        assert!(provenance.use_imported_pairs);
        assert!(!provenance.use_imported_placements);
    }
}
