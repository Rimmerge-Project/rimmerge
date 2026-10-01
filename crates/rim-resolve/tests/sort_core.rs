//! Integration tests for `rim_resolve::sort` on hand-made mini reports:
//! tiers, each layer's pairwise precedence, cycle-break tie-break,
//! promotion out of `Body` by a `Declared` edge, and missing-mod handling.

use std::collections::BTreeSet;

use rim_analyzer::domain::{EdgeKind, EdgeStatus, EdgeStrength, LoadOrder, ModId};
use rim_resolve::domain::{
    PairRule, Placement, PlacementRule, Rule, RuleOrigin, RuleSet, SorterOverrides, Tagging,
};
use rim_resolve::evaluate;
use rim_resolve::sort::{EnforcedLayers, SortInput, Tier, sort};
use rim_resolve::test_support::ReportBuilder;

fn empty_current(builder: &ReportBuilder) -> LoadOrder {
    builder.insertion_order()
}

fn base_input<'a>(
    report: &'a rim_analyzer::domain::Report,
    rules: &'a RuleSet,
    tagging: &'a Tagging,
    overrides: &'a SorterOverrides,
    current: &'a LoadOrder,
) -> SortInput<'a> {
    SortInput {
        report,
        rules,
        tagging,
        overrides,
        current,
        enforce: EnforcedLayers::default(),
        // `PreserveCurrent`, not the sorter's own `Rebuild` default: every
        // test in this file that builds through `base_input` was written
        // against current-position base keys, and switching them to
        // display-name ranks would just be testing something else.
        // Rebuild's own behavior gets dedicated coverage in
        // `sort_proptest.rs`/`sort_golden.rs`.
        tie_break: rim_resolve::sort::TieBreak::PreserveCurrent,
    }
}

// --- tiers -------------------------------------------------------------

#[test]
fn core_and_dlc_always_load_before_everything_else() {
    let builder = ReportBuilder::new()
        .mod_("regular.mod")
        .dlc("ludeon.royalty")
        .core("ludeon.rimworld");
    let current = empty_current(&builder);
    let report = builder.build();
    let rules = RuleSet::default();
    let tagging = Tagging::default();
    let overrides = SorterOverrides::default();

    let outcome = sort(&base_input(&report, &rules, &tagging, &overrides, &current));

    let order = outcome.order.as_slice();
    let core_pos = order
        .iter()
        .position(|id| *id == ModId::new("ludeon.rimworld"))
        .unwrap();
    let dlc_pos = order
        .iter()
        .position(|id| *id == ModId::new("ludeon.royalty"))
        .unwrap();
    let mod_pos = order
        .iter()
        .position(|id| *id == ModId::new("regular.mod"))
        .unwrap();
    assert!(core_pos < dlc_pos, "Core must load before Dlc");
    assert!(dlc_pos < mod_pos, "Dlc must load before a regular mod");
    assert_eq!(
        outcome.placements[&ModId::new("ludeon.rimworld")].tier,
        Tier::Core
    );
    assert_eq!(
        outcome.placements[&ModId::new("ludeon.royalty")].tier,
        Tier::Dlc
    );
}

#[test]
fn placement_rules_pin_top_and_bottom() {
    let builder = ReportBuilder::new()
        .mod_("body.mod")
        .mod_("top.mod")
        .mod_("bottom.mod");
    let current = builder.insertion_order();
    let report = builder.build();
    let rules = RuleSet::new(vec![
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("top.mod"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("bottom.mod"),
            placement: Placement::Bottom,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
    ]);
    let tagging = Tagging::default();
    let overrides = SorterOverrides::default();

    let outcome = sort(&base_input(&report, &rules, &tagging, &overrides, &current));
    let order = outcome.order.as_slice();
    let top = order
        .iter()
        .position(|id| *id == ModId::new("top.mod"))
        .unwrap();
    let body = order
        .iter()
        .position(|id| *id == ModId::new("body.mod"))
        .unwrap();
    let bottom = order
        .iter()
        .position(|id| *id == ModId::new("bottom.mod"))
        .unwrap();
    assert!(top < body);
    assert!(body < bottom);
}

/// `apply_placement_extremes` bounds the `Top` region by the nominal
/// `Tier::Top` span, never by a plain prefix of the whole order
/// (`order[..=last Top pin]`): such a prefix reaches into every
/// `Core`/`Dlc` mod ahead of the pin — tier order is carried only by
/// `Membership`/`Boundary` sentinel edges, which `extremize_region`
/// deliberately filters out, so a pin with no `Real` predecessor in that
/// oversized region would be free to jump ahead of `Core` itself. The
/// fixture has zero edges connecting the pin to `Core`/`Dlc`: with a
/// prefix bound, `aaa.toppin` (alphabetically first, entirely unrelated)
/// races to the very front of the whole order; with the tier-span bound,
/// it stays behind every `Core` and `Dlc` mod.
#[test]
fn a_top_pin_never_sorts_before_core_or_a_dlc() {
    let builder = ReportBuilder::new()
        .core("ludeon.rimworld")
        .dlc("ludeon.rimworld.royalty")
        .mod_("body.mod")
        .mod_("aaa.toppin");
    let current = builder.insertion_order();
    let report = builder.build();
    let rules = RuleSet::new(vec![Rule::Placement(PlacementRule {
        mod_id: ModId::new("aaa.toppin"),
        placement: Placement::Top,
        origin: RuleOrigin::UserDecision,
        comment: None,
    })]);
    let tagging = Tagging::default();
    let overrides = SorterOverrides::default();

    let outcome = sort(&base_input(&report, &rules, &tagging, &overrides, &current));

    let order = outcome.order.as_slice();
    let position_of = |id: &str| {
        order
            .iter()
            .position(|m| *m == ModId::new(id))
            .expect("mod present in the sorted order")
    };
    let core_pos = position_of("ludeon.rimworld");
    let dlc_pos = position_of("ludeon.rimworld.royalty");
    let pin_pos = position_of("aaa.toppin");

    assert!(
        core_pos < pin_pos && dlc_pos < pin_pos,
        "an explicit Top pin must never sort ahead of Core or a DLC, got order {order:?}"
    );
}

// --- layer precedence, pairwise -----------------------------------------
//
// Each case sets up a two-mod contradiction between two layers: a strong
// layer says `strong_after` must load after `strong_before`, and a weaker
// layer says the opposite. The stronger layer's edge must always survive.

fn assert_layer_wins(build_report: impl FnOnce(ReportBuilder) -> ReportBuilder, weaker_rule: Rule) {
    let builder = build_report(ReportBuilder::new().mod_("a").mod_("b"));
    let current = builder.insertion_order();
    let report = builder.build();
    let rules = RuleSet::new(vec![weaker_rule]);
    let tagging = Tagging::default();
    let overrides = SorterOverrides::default();

    let outcome = sort(&base_input(&report, &rules, &tagging, &overrides, &current));

    // The stronger layer said "a after b" (b loads first); that must hold.
    assert_eq!(
        outcome.order.position(&ModId::new("b")),
        Some(0),
        "the stronger layer's ordering must survive the cycle"
    );
}

fn pair(after: &str, before: &str, origin: RuleOrigin) -> Rule {
    Rule::Pair(rim_resolve::domain::PairRule {
        after: ModId::new(after),
        before: ModId::new(before),
        origin,
        comment: None,
        overrides_declared: false,
    })
}

/// [`pair`]'s declared-edge-override sibling:
/// always `RuleOrigin::UserDecision` with `overrides_declared: true` —
/// the only shape the sorter honours the flag for.
fn override_pair(after: &str, before: &str) -> Rule {
    Rule::Pair(rim_resolve::domain::PairRule {
        after: ModId::new(after),
        before: ModId::new(before),
        origin: RuleOrigin::UserDecision,
        comment: None,
        overrides_declared: true,
    })
}

#[test]
fn hard_beats_declared() {
    assert_layer_wins(
        |b| b.hard_edge("a", "b"),
        pair("b", "a", RuleOrigin::RimSortUser), // contradicts: says b after a
    );
}

#[test]
fn declared_beats_rimsort_user() {
    assert_layer_wins(
        |b| b.declared_edge("a", "b"),
        pair("b", "a", RuleOrigin::RimSortUser),
    );
}

/// The declared-edge override's own end-to-end proof
/// a `UserDecision` pair rule flagged
/// `overrides_declared` actually flips the emitted order relative to a
/// contradicting `Declared` edge — not just a ledger finding, the real
/// sort result. Uses `assert_layer_wins` with the *rule* as the
/// "stronger" side by naming the override as `weaker_rule` but asserting
/// the *rule's* own claim survives — `assert_layer_wins` itself is
/// direction-agnostic (it just checks whichever mod the report/rule's
/// combined setup says must load first), so this reuses it exactly like
/// every other pairwise-precedence test in this block, only with the
/// rule instead of the report holding the correct direction.
#[test]
fn declared_override_beats_declared() {
    let builder = ReportBuilder::new().mod_("a").mod_("b").declared_edge(
        "a", "b", // Declared: a after b (b loads first per the declaration).
    );
    let current = builder.insertion_order();
    let report = builder.build();
    // The override claims the opposite: b after a.
    let rules = RuleSet::new(vec![override_pair("b", "a")]);
    let tagging = Tagging::default();
    let overrides = SorterOverrides::default();

    let outcome = sort(&base_input(&report, &rules, &tagging, &overrides, &current));

    assert_eq!(
        outcome.order.position(&ModId::new("a")),
        Some(0),
        "the override rule's own claim (b after a, so a loads first) must win over the \
         contradicting Declared edge"
    );
}

/// Requirement 5: the override must never beat a `Hard` edge, however
/// it's flagged.
#[test]
fn hard_beats_declared_override() {
    assert_layer_wins(|b| b.hard_edge("a", "b"), override_pair("b", "a"));
}

#[test]
fn rimsort_user_beats_rimsort_community() {
    let builder = ReportBuilder::new().mod_("a").mod_("b");
    let current = builder.insertion_order();
    let report = builder.build();
    let rules = RuleSet::new(vec![
        pair("a", "b", RuleOrigin::RimSortUser),
        pair("b", "a", RuleOrigin::RimSortCommunity),
    ]);
    let tagging = Tagging::default();
    let overrides = SorterOverrides::default();

    let outcome = sort(&base_input(&report, &rules, &tagging, &overrides, &current));
    assert_eq!(outcome.order.position(&ModId::new("b")), Some(0));
}

#[test]
fn rimsort_community_beats_steamdb() {
    let builder = ReportBuilder::new().mod_("a").mod_("b");
    let current = builder.insertion_order();
    let report = builder.build();
    let rules = RuleSet::new(vec![
        pair("a", "b", RuleOrigin::RimSortCommunity),
        pair("b", "a", RuleOrigin::SteamDb),
    ]);
    let tagging = Tagging::default();
    let overrides = SorterOverrides::default();

    let outcome = sort(&base_input(&report, &rules, &tagging, &overrides, &current));
    assert_eq!(outcome.order.position(&ModId::new("b")), Some(0));
}

#[test]
fn user_decision_beats_rimsort_user() {
    let builder = ReportBuilder::new().mod_("a").mod_("b");
    let current = builder.insertion_order();
    let report = builder.build();
    let rules = RuleSet::new(vec![pair("b", "a", RuleOrigin::RimSortUser)]);
    let tagging = Tagging::default();
    let mut overrides = SorterOverrides::default();
    overrides.reorders.push((ModId::new("a"), ModId::new("b")));

    let outcome = sort(&base_input(&report, &rules, &tagging, &overrides, &current));
    assert_eq!(outcome.order.position(&ModId::new("b")), Some(0));
}

/// A `PairRule` stored directly at `RuleOrigin::UserDecision` in the
/// `RuleSet` — the exact shape `Session::promote_imported_rule` produces
/// for a promoted pair rule — must feed the sorter's `UserDecision` layer
/// the same as an `overrides.reorders` entry does, not be silently
/// ignored. If `layers::rule_pair_edges` read only
/// `RimSortUser`/`RimSortCommunity`/`SteamDb`-origin rules, a promoted pair
/// rule would sit in the `RuleSet` with no effect on the sort (unlike a
/// placement rule, which `sort::tiers::assign` reads for any origin).
#[test]
fn a_user_decision_origin_pair_rule_beats_rimsort_user() {
    let builder = ReportBuilder::new().mod_("a").mod_("b");
    let current = builder.insertion_order();
    let report = builder.build();
    let rules = RuleSet::new(vec![
        pair("b", "a", RuleOrigin::RimSortUser),  // says b after a
        pair("a", "b", RuleOrigin::UserDecision), // contradicts: a promoted copy
    ]);
    let tagging = Tagging::default();
    let overrides = SorterOverrides::default();

    let outcome = sort(&base_input(&report, &rules, &tagging, &overrides, &current));

    assert_eq!(
        outcome.order.position(&ModId::new("b")),
        Some(0),
        "a UserDecision-origin pair rule stored in the RuleSet must outrank an imported one"
    );
}

#[test]
fn declared_beats_soft_and_awareness() {
    // Both weak strengths default to advisory (see
    // `EnforcedLayers::default`), so this test explicitly enforces both —
    // otherwise the soft edge would never join the graph at all and this
    // would only be testing `declared_edge` in isolation, not precedence.
    let builder = ReportBuilder::new().mod_("a").mod_("b");
    let report = builder
        .clone()
        .declared_edge("a", "b")
        .soft_edge("b", "a")
        .build();
    let current = builder.insertion_order();
    let outcome = sort(&SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers {
            soft: true,
            awareness: true,
            inferred: true,
        },
        tie_break: rim_resolve::sort::TieBreak::PreserveCurrent,
    });
    assert_eq!(outcome.order.position(&ModId::new("b")), Some(0));
}

// --- advisory (non-enforced) engine edges ---------------------------------

#[test]
fn awareness_edges_are_advisory_by_default_and_never_move_anything() {
    let builder = ReportBuilder::new().mod_("a").mod_("b");
    // `a` must be after `b` per the Awareness edge, but the current order
    // (and hence, with the edge merely advisory, the suggested order too)
    // has it the other way around.
    let report = builder.clone().awareness_edge("a", "b").build();
    let current = builder.insertion_order();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    assert_eq!(
        outcome.order.as_slice(),
        current.as_slice(),
        "an unenforced Awareness edge must never move a mod"
    );
    assert!(
        outcome.dropped.is_empty(),
        "it was never added, so it can't be dropped either"
    );
    let explanation = &outcome.placements[&ModId::new("a")];
    assert_eq!(explanation.advisory.len(), 1);
    assert!(
        !explanation.advisory[0].satisfied,
        "the advisory edge is violated by the (unchanged) emitted order"
    );
    assert_eq!(explanation.advisory[0].strength, EdgeStrength::Awareness);
}

#[test]
fn enforcing_awareness_turns_it_into_a_real_constraint() {
    let builder = ReportBuilder::new().mod_("a").mod_("b");
    let report = builder.clone().awareness_edge("a", "b").build();
    let current = builder.insertion_order();
    let overrides = SorterOverrides::default();
    let input = SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &overrides,
        current: &current,
        enforce: EnforcedLayers {
            soft: true,
            awareness: true,
            inferred: true,
        },
        tie_break: rim_resolve::sort::TieBreak::PreserveCurrent,
    };

    let outcome = sort(&input);

    assert_eq!(outcome.order.position(&ModId::new("b")), Some(0));
    assert!(outcome.placements[&ModId::new("a")].advisory.is_empty());
}

/// `enforce.inferred` is a real toggle, independent
/// of `soft`/`awareness`: with it explicitly `false`, an `Inferred`-
/// strength edge (`PatchRemovedNode`/`RetextureAfterOwner`/
/// `DefOverrideAfterOrigin`) must still stay advisory-only, regardless of
/// `awareness: true` right alongside it — the same independence the
/// `soft`/`awareness` toggles already have from each other. `b`
/// `PatchRemovedNode`-must-load-after `a`, but the current order is
/// `[b, a]`: were this edge actually enforced, the sorter would have to
/// move `b` after `a`, so an unchanged order is proof the edge never
/// reached the graph, not just an absence of evidence. (The complementary
/// case — `enforce.inferred: true`, the default, actually enforcing one —
/// is `an_inferred_edge_loses_to_user_decision_and_rimsort_user_but_beats_name_order`
/// above.)
#[test]
fn an_inferred_edge_stays_advisory_when_enforce_inferred_is_off() {
    let builder = ReportBuilder::new().mod_("b").mod_("a");
    let report = builder
        .clone()
        .edge("b", "a", EdgeKind::PatchRemovedNode, true)
        .build();
    let current = builder.insertion_order();
    let overrides = SorterOverrides::default();
    let input = SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &overrides,
        current: &current,
        enforce: EnforcedLayers {
            soft: true,
            awareness: true,
            inferred: false,
        },
        tie_break: rim_resolve::sort::TieBreak::PreserveCurrent,
    };

    let outcome = sort(&input);

    // Unchanged order: the edge was never added to the graph, so nothing
    // forced `b` after `a`.
    assert_eq!(outcome.order.position(&ModId::new("b")), Some(0));
    assert_eq!(outcome.order.position(&ModId::new("a")), Some(1));
    assert!(outcome.dropped.is_empty());

    let advisory = &outcome.placements[&ModId::new("b")].advisory;
    assert_eq!(advisory.len(), 1);
    assert!(matches!(
        &advisory[0].edge.provenance,
        rim_resolve::sort::EdgeProvenance::Engine {
            kind: EdgeKind::PatchRemovedNode,
            ..
        }
    ));
    assert_eq!(
        advisory[0].strength,
        rim_analyzer::domain::EdgeStrength::Inferred,
        "the advisory entry must report its own real Inferred strength, \
         not borrow Awareness's the way an earlier interim seam did"
    );
}

#[test]
fn disabling_soft_enforcement_makes_it_advisory_too() {
    let builder = ReportBuilder::new().mod_("a").mod_("b");
    let report = builder.clone().soft_edge("a", "b").build();
    let current = builder.insertion_order();
    let overrides = SorterOverrides::default();
    let input = SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &overrides,
        current: &current,
        enforce: EnforcedLayers {
            soft: false,
            awareness: false,
            inferred: true,
        },
        tie_break: rim_resolve::sort::TieBreak::PreserveCurrent,
    };

    let outcome = sort(&input);

    assert_eq!(outcome.order.as_slice(), current.as_slice());
    let advisory = &outcome.placements[&ModId::new("a")].advisory;
    assert_eq!(advisory.len(), 1);
    assert_eq!(advisory[0].strength, EdgeStrength::Soft);
}

// --- cycle drop tie-break -------------------------------------------------

#[test]
fn cycle_break_drops_the_edge_pointing_at_the_least_depended_on_mod() {
    let builder = ReportBuilder::new()
        .mod_("popular")
        .mod_("unpopular")
        .hard_dependents("popular", 10)
        .declared_edge("popular", "unpopular") // unpopular must load before popular
        .declared_edge("unpopular", "popular"); // popular must load before unpopular: cycle
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    assert_eq!(outcome.dropped.len(), 1);
    // The edge naming `unpopular` as `before` is the one that gets cut
    // (it points at the least-depended-on mod).
    assert_eq!(outcome.dropped[0].edge.before, ModId::new("unpopular"));
}

// --- promotion out of Body -------------------------------------------------

#[test]
fn a_declared_edge_promotes_a_mod_out_of_the_body_tier() {
    let builder = ReportBuilder::new().mod_("body.mod").mod_("bottom.mod");
    let report = builder
        .clone()
        // body.mod must load *after* bottom.mod: contradicts the natural
        // Body-before-Bottom tier order, forcing body.mod's own (weak,
        // Awareness-layer, the weakest layer) tier membership edge to
        // yield.
        .declares_load_after("body.mod", "bottom.mod")
        .declared_edge("body.mod", "bottom.mod")
        .build();
    let current = builder.insertion_order();
    let rules = RuleSet::new(vec![Rule::Placement(PlacementRule {
        mod_id: ModId::new("bottom.mod"),
        placement: Placement::Bottom,
        origin: RuleOrigin::UserDecision,
        comment: None,
    })]);

    let outcome = sort(&base_input(
        &report,
        &rules,
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    // bottom.mod must still load before body.mod, per the Declared edge.
    assert_eq!(outcome.order.position(&ModId::new("bottom.mod")), Some(0));
    let explanation = &outcome.placements[&ModId::new("body.mod")];
    assert!(
        matches!(
            explanation.tier_reason,
            rim_resolve::sort::TierReason::PromotedBy(_)
        ),
        "expected a PromotedBy tier reason, got {:?}",
        explanation.tier_reason
    );
}

/// When a `Top`
/// mod's own tier membership edge and a `Body` mod's own tier membership
/// edge would *both* be candidates at the same layer as the `Real` edge
/// that contradicts them (only possible when the real edge is itself
/// `Awareness`-strength, `Body`'s own weakest membership layer, and
/// `enforce.awareness` is on), the sorter must prefer dropping the
/// `Membership` edge over the `Real` one — a `MayRequire` reference from a
/// `Top` mod onto a `Body` mod must still hold, promoting the `Body` mod
/// out of its tier, rather than the reference itself being silently
/// dropped to keep the `Body` mod in place.
#[test]
fn a_top_mods_may_require_onto_a_body_mod_promotes_it_out_of_the_body_tier_under_enforced_awareness()
 {
    let builder = ReportBuilder::new()
        .mod_("top.mod")
        .mod_("body.mod")
        // Without the Membership-over-Real preference, the
        // `(before.hard_dependents asc, after.hard_dependents desc, ...)`
        // tie-break alone would pick the *Real* edge here (body.mod has 0
        // hard dependents same as the membership edge's own sentinel
        // source, but top.mod's higher count makes
        // `Reverse(after_hard_dependents)` favor dropping the edge that
        // points at it) — proving this test actually exercises that
        // preference rather than passing by coincidence of alphabetical mod
        // names.
        .hard_dependents("top.mod", 5);
    let report = builder
        .clone()
        // top.mod must load *after* body.mod: contradicts the natural
        // Top-before-Body tier order, and — since this is Awareness
        // strength, the same layer body.mod's own tier membership edge
        // binds at — both edges are candidates in the very same cycle
        // break.
        .edge("top.mod", "body.mod", EdgeKind::MayRequire, true)
        .build();
    let current = builder.insertion_order();
    let rules = RuleSet::new(vec![Rule::Placement(PlacementRule {
        mod_id: ModId::new("top.mod"),
        placement: Placement::Top,
        origin: RuleOrigin::UserDecision,
        comment: None,
    })]);

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers {
            soft: false,
            awareness: true,
            inferred: true,
        },
        tie_break: rim_resolve::sort::TieBreak::PreserveCurrent,
    });

    // The MayRequire edge must hold: body.mod loads before top.mod,
    // exactly the opposite of the two mods' default Body/Top tier order —
    // proof the sorter dropped body.mod's own tier membership edge, not
    // the real cross-mod edge.
    assert!(
        outcome.order.position(&ModId::new("body.mod"))
            < outcome.order.position(&ModId::new("top.mod")),
        "expected body.mod before top.mod, got order {:?}",
        outcome.order.as_slice()
    );
    let explanation = &outcome.placements[&ModId::new("body.mod")];
    assert!(
        matches!(
            explanation.tier_reason,
            rim_resolve::sort::TierReason::PromotedBy(_)
        ),
        "expected body.mod to be PromotedBy out of Body, got {:?}",
        explanation.tier_reason
    );
}

// --- Placement pins sort to the extreme edge of their tier --------

/// An explicit `Bottom` pin
/// with no relation of its own to anything else must still sort *after* a
/// mod merely dependency-promoted into the Bottom region by a *different*
/// pin — never sharing the region's alphabetical rank as if the two were
/// equally "just there". `zzz.dependent`'s name is deliberately chosen to
/// sort *after* `bbb.pinned`'s: under a tie-break of pure `Rebuild`
/// display-name rank with no placement bias, the ready-set would emit
/// `bbb.pinned` before `zzz.dependent` purely alphabetically — a popular
/// pin's own dependents interleaving with an unrelated pin by alphabet,
/// "dissolving" the tier for the second pin.
#[test]
fn an_explicit_bottom_pin_sorts_after_a_mod_promoted_in_by_a_different_pin() {
    let builder = ReportBuilder::new()
        .mod_("aaa.provider")
        .mod_("bbb.pinned")
        .mod_("zzz.dependent");
    let report = builder
        .clone()
        // zzz.dependent must load after aaa.provider: promotes it into
        // the Bottom region exactly like the real `example.framework`
        // dependents.
        .declares_load_after("zzz.dependent", "aaa.provider")
        .declared_edge("zzz.dependent", "aaa.provider")
        .build();
    let current = builder.insertion_order();
    let rules = RuleSet::new(vec![
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("aaa.provider"),
            placement: Placement::Bottom,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("bbb.pinned"),
            placement: Placement::Bottom,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
    ]);

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    let dependent_pos = outcome
        .order
        .position(&ModId::new("zzz.dependent"))
        .unwrap();
    let pinned_pos = outcome.order.position(&ModId::new("bbb.pinned")).unwrap();
    assert!(
        dependent_pos < pinned_pos,
        "expected the promoted mod (zzz.dependent) before the unrelated \
         Bottom pin (bbb.pinned), got order {:?}",
        outcome.order.as_slice()
    );
}

/// The `Top` mirror: an explicit `Top` pin with no relation of its own
/// to anything else must still sort *before* a mod merely
/// dependency-promoted into the Top region by a *different* pin.
/// `aaa.dependent`'s name is deliberately chosen to sort *before*
/// `zzz.pinned`'s, for the same reason as the Bottom test above (proving
/// the placement bias overrides alphabetical rank, not merely agreeing
/// with it by coincidence).
#[test]
fn an_explicit_top_pin_sorts_before_a_mod_promoted_in_by_a_different_pin() {
    let builder = ReportBuilder::new()
        .mod_("aaa.dependent")
        .mod_("mmm.provider")
        .mod_("zzz.pinned");
    let report = builder
        .clone()
        // aaa.dependent must load before mmm.provider: promotes it into
        // the Top region.
        .declares_load_after("mmm.provider", "aaa.dependent")
        .declared_edge("mmm.provider", "aaa.dependent")
        .build();
    let current = builder.insertion_order();
    let rules = RuleSet::new(vec![
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("mmm.provider"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("zzz.pinned"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
    ]);

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    let dependent_pos = outcome
        .order
        .position(&ModId::new("aaa.dependent"))
        .unwrap();
    let pinned_pos = outcome.order.position(&ModId::new("zzz.pinned")).unwrap();
    assert!(
        pinned_pos < dependent_pos,
        "expected the unrelated Top pin (zzz.pinned) before the promoted \
         mod (aaa.dependent), got order {:?}",
        outcome.order.as_slice()
    );
}

/// `placement_bias` alone only
/// orders nodes ready in the *same* Kahn round — it cannot hold an
/// already-ready, unrelated pin back on the chance that a *different*
/// pin's own not-yet-ready dependent will later need the extreme slot
/// more. `aaa.leafpin` is deliberately named to sort *before*
/// `mmm.pin` — both become ready in the very same early round (neither
/// has a prerequisite of its own), so a per-round bias tie-break
/// alone emits `aaa.leafpin` immediately, before `ttt.dependent` (which
/// only becomes ready once `mmm.pin` itself is emitted) ever gets a
/// chance to compete for the tier's own trailing slot.
/// `sort::emit::extremize_region`, a post-emission pass, must still push
/// `aaa.leafpin` — a pin with no relation of its own to anything —
/// behind `mmm.pin`'s whole promoted chain, landing it in the true last
/// slot.
#[test]
fn an_unrelated_bottom_pin_ready_in_an_earlier_round_still_ends_up_after_a_blocked_pins_dependent()
{
    let builder = ReportBuilder::new()
        .mod_("aaa.leafpin")
        .mod_("mmm.pin")
        .mod_("ttt.dependent");
    let report = builder
        .clone()
        // ttt.dependent must load after mmm.pin: mmm.pin can never itself
        // reach the true trailing slot, since something (ttt.dependent)
        // must always follow it.
        .declares_load_after("ttt.dependent", "mmm.pin")
        .declared_edge("ttt.dependent", "mmm.pin")
        .build();
    let current = builder.insertion_order();
    let rules = RuleSet::new(vec![
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("aaa.leafpin"),
            placement: Placement::Bottom,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("mmm.pin"),
            placement: Placement::Bottom,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
    ]);

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    assert_eq!(
        outcome.order.as_slice(),
        &[
            ModId::new("mmm.pin"),
            ModId::new("ttt.dependent"),
            ModId::new("aaa.leafpin"),
        ],
        "the unconstrained pin (aaa.leafpin) must land after mmm.pin's whole \
         promoted chain, got order {:?}",
        outcome.order.as_slice()
    );
}

/// The `Top` counterpart of the test above needs `Core`/`Dlc` in it:
/// without them it passes with or without `extremize_region` doing any
/// work at all — an unconstrained `Top` pin is already ready in the base
/// Kahn's very first round and wins the head slot from [`placement_bias`]
/// alone, before a second pass ever runs (the module doc's own
/// "forward-Kahn ready-set order does not suffer the same lookahead gap
/// on this side" is true, but it is not the property this test needs).
/// This scenario has teeth: it is the
/// [`a_top_pin_never_sorts_before_core_or_a_dlc`] shape combined with a
/// free-vs-edge-blocked pin pair — with a plain `order[..=last Top
/// pin]` prefix (this fixture's two `Top` pins bracket the *entire*
/// order, so such a prefix swallows `Core` and `Dlc` whole),
/// `zzz.freepin` would win the region's very first slot ahead of both,
/// landing before `Core` itself; with the nominal-tier-span bound,
/// `Core`/`Dlc` are excluded from the `Top` region entirely and the
/// free/blocked pin ordering is unchanged. `zzz.freepin` wins that slot
/// because its closure is the one-mod one, the smallest, so its block is
/// moved *last* and therefore lands nearest the head, while
/// `mmm.blockedpin`'s two-mod closure moves first.
#[test]
fn a_free_top_pin_and_a_blocked_top_pin_both_stay_behind_core_and_dlc() {
    let builder = ReportBuilder::new()
        .core("ludeon.rimworld")
        .dlc("ludeon.rimworld.royalty")
        .mod_("zzz.freepin")
        .mod_("mmm.blockedpin")
        .mod_("qqq.prerequisite");
    let report = builder
        .clone()
        // mmm.blockedpin must load after qqq.prerequisite: it can never
        // itself reach the Top region's true leading slot, since
        // something (qqq.prerequisite) must always precede it.
        .declares_load_after("mmm.blockedpin", "qqq.prerequisite")
        .declared_edge("mmm.blockedpin", "qqq.prerequisite")
        .build();
    let current = builder.insertion_order();
    let rules = RuleSet::new(vec![
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("zzz.freepin"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("mmm.blockedpin"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
    ]);

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    assert_eq!(
        outcome.order.as_slice(),
        &[
            ModId::new("ludeon.rimworld"),
            ModId::new("ludeon.rimworld.royalty"),
            ModId::new("zzz.freepin"),
            ModId::new("qqq.prerequisite"),
            ModId::new("mmm.blockedpin"),
        ],
        "Core and a DLC must both stay ahead of every Top pin, and the \
         free pin must still land before the blocked pin's own \
         prerequisite chain, got order {:?}",
        outcome.order.as_slice()
    );
}

// --- Pins with dependents reach the extreme too -------
//
// A per-slot peel would give the region's extreme slot to *leaf* pins
// only: a pin with region-internal successors becomes eligible only once
// every one of them is already assigned, and those successors are
// ordinary `0`-bias nodes — picked only after every non-pin merely
// sitting later in the base order — so the pin ends up as far from the
// extreme as its own dependents' base ranks push it. `extremize_region`
// moves each pin's whole closure as one block instead.

/// Every `Hard`/`Declared` edge in `report` holds in `order` —
/// `sort_golden.rs`'s own `assert_invariants` does the same job over the
/// full fixture, but lives in a different test binary (integration tests
/// each compile as their own crate), so the closure-move tests below
/// carry this small local counterpart rather than reaching across.
///
/// Filtered on the edge's own strength, not applied to every
/// `report.edges` entry: `Soft`/`Awareness`/`Inferred` edges are advisory
/// under some `EnforcedLayers` settings and a `Hard`/`Declared` one can
/// still be legitimately dropped by a cycle break, so an unfiltered
/// version would fail on a future fixture for reasons that have nothing
/// to do with this pass. The fixtures below deliberately carry only
/// `Declared` (`LoadAfter`) edges and no cycles, so nothing is skipped
/// today — the filter exists so that stays true by construction.
fn assert_declared_edges_hold(report: &rim_analyzer::domain::Report, order: &LoadOrder) {
    for edge_report in &report.edges {
        let edge = &edge_report.edge;
        if !matches!(edge.strength(), EdgeStrength::Hard | EdgeStrength::Declared) {
            continue;
        }
        let (Some(after), Some(before)) =
            (order.position(&edge.after), order.position(&edge.before))
        else {
            continue;
        };
        assert!(
            before < after,
            "accepted edge {} after {} must hold in {:?}",
            edge.after,
            edge.before,
            order.as_slice()
        );
    }
}

/// The invariant the closure block move actually guarantees, asserted
/// directly: within the `Bottom` region every mod positioned *after* an
/// explicit `Bottom` pin is itself such a pin or lies in some pin's own
/// successor closure. Each entry of `pins` is one pin plus the rest of
/// its closure.
fn assert_nothing_unrelated_follows_a_bottom_pin(order: &LoadOrder, pins: &[(&str, &[&str])]) {
    let in_some_closure: std::collections::BTreeSet<ModId> = pins
        .iter()
        .flat_map(|(pin, closure)| {
            std::iter::once(ModId::new(pin)).chain(closure.iter().copied().map(ModId::new))
        })
        .collect();
    let Some(first_pin) = pins
        .iter()
        .filter_map(|(pin, _)| order.position(&ModId::new(pin)))
        .min()
    else {
        panic!("every pin must be somewhere in {:?}", order.as_slice());
    };
    for (index, id) in order.as_slice().iter().enumerate().skip(first_pin) {
        assert!(
            in_some_closure.contains(id),
            "{id} (position {index}) follows a Bottom pin without being a pin or in any \
             pin's closure, in {:?}",
            order.as_slice()
        );
    }
}

/// A real install's own performance-optimizer-mod shape, reduced:
/// `mmm.pin_p` is a `Bottom` pin with a two-hop `Declared` dependent
/// chain of its own (`aaa.dep_p1` -> `bbb.dep_p2`), `nnn.pin_q` a second
/// `Bottom` pin with three dependents whose display names all rank
/// *after* `mmm.pin_p`'s own two under `Rebuild`, and `lll.leafpin` a
/// third, relation-free pin.
///
/// Under a per-slot peel, `mmm.pin_p` could only be assigned once
/// both of its own dependents were, and those two — bias-`0` nodes
/// ranking *first* alphabetically — would be the very last non-pins the
/// peel reached, so `mmm.pin_p` would land at the region's *leading* edge
/// with `nnn.pin_q`'s whole unrelated block behind it: `[mmm.pin_p,
/// aaa.dep_p1, bbb.dep_p2, nnn.pin_q, xxx.dep_q1, yyy.dep_q2,
/// zzz.dep_q3, lll.leafpin]`. With the closure block move, each pin's
/// closure is moved to the tail as one block, largest closure first, so
/// only the pins' own closures ever follow a pin.
#[test]
fn a_bottom_pin_with_its_own_dependents_still_reaches_the_tail_past_another_pins_closure() {
    let builder = ReportBuilder::new()
        .mod_("aaa.dep_p1")
        .mod_("bbb.dep_p2")
        .mod_("lll.leafpin")
        .mod_("mmm.pin_p")
        .mod_("nnn.pin_q")
        .mod_("xxx.dep_q1")
        .mod_("yyy.dep_q2")
        .mod_("zzz.dep_q3");
    let report = builder
        .clone()
        .declares_load_after("aaa.dep_p1", "mmm.pin_p")
        .declared_edge("aaa.dep_p1", "mmm.pin_p")
        .declares_load_after("bbb.dep_p2", "aaa.dep_p1")
        .declared_edge("bbb.dep_p2", "aaa.dep_p1")
        .declares_load_after("xxx.dep_q1", "nnn.pin_q")
        .declared_edge("xxx.dep_q1", "nnn.pin_q")
        .declares_load_after("yyy.dep_q2", "nnn.pin_q")
        .declared_edge("yyy.dep_q2", "nnn.pin_q")
        .declares_load_after("zzz.dep_q3", "nnn.pin_q")
        .declared_edge("zzz.dep_q3", "nnn.pin_q")
        .build();
    let current = builder.insertion_order();
    let rules = RuleSet::new(
        ["lll.leafpin", "mmm.pin_p", "nnn.pin_q"]
            .into_iter()
            .map(|mod_id| {
                Rule::Placement(PlacementRule {
                    mod_id: ModId::new(mod_id),
                    placement: Placement::Bottom,
                    origin: RuleOrigin::UserDecision,
                    comment: None,
                })
            })
            .collect(),
    );

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    assert_declared_edges_hold(&report, &outcome.order);
    assert_nothing_unrelated_follows_a_bottom_pin(
        &outcome.order,
        &[
            ("lll.leafpin", &[]),
            ("mmm.pin_p", &["aaa.dep_p1", "bbb.dep_p2"]),
            ("nnn.pin_q", &["xxx.dep_q1", "yyy.dep_q2", "zzz.dep_q3"]),
        ],
    );
    assert_eq!(
        outcome.order.as_slice(),
        &[
            // `nnn.pin_q`'s closure is the largest (4), so it is moved
            // first and ends up furthest from the tail...
            ModId::new("nnn.pin_q"),
            ModId::new("xxx.dep_q1"),
            ModId::new("yyy.dep_q2"),
            ModId::new("zzz.dep_q3"),
            // ...then `mmm.pin_p`'s own (3), immediately followed by
            // exactly its own two dependents and nothing else...
            ModId::new("mmm.pin_p"),
            ModId::new("aaa.dep_p1"),
            ModId::new("bbb.dep_p2"),
            // ...and the leaf pin (closure 1) takes the true last slot.
            ModId::new("lll.leafpin"),
        ],
        "each Bottom pin must be followed by exactly its own closure, largest closure \
         first, got order {:?}",
        outcome.order.as_slice()
    );
}

/// The `Head` mirror of the test above: `mmm.pin_p` is a `Top` pin with
/// a two-hop `Declared` *prerequisite* chain of its own
/// (`yyy.pre_p2` -> `xxx.pre_p1`), `nnn.pin_q` a second `Top` pin with
/// three prerequisites whose display names all rank *before*
/// `mmm.pin_p`'s own two under `Rebuild` (the mirror of the `Bottom`
/// fixture's own rank relation, since the `Head` peel filled slots from
/// the smallest base index inward), and `lll.leafpin` a third,
/// relation-free pin. `Core`/`Dlc` are present so the region is the
/// real, `top_region_start`-bounded one rather than a degenerate
/// whole-order prefix, and every prerequisite's name ranks after theirs
/// so nothing is promoted ahead of `Core` itself.
#[test]
fn a_top_pin_with_its_own_prerequisites_still_reaches_the_head_past_another_pins_closure() {
    let builder = ReportBuilder::new()
        .core("ludeon.rimworld")
        .dlc("ludeon.rimworld.royalty")
        .mod_("lll.leafpin")
        .mod_("mmm.pin_p")
        .mod_("nnn.pin_q")
        .mod_("ooo.pre_q1")
        .mod_("ppp.pre_q2")
        .mod_("qqq.pre_q3")
        .mod_("xxx.pre_p1")
        .mod_("yyy.pre_p2");
    let report = builder
        .clone()
        .declares_load_after("mmm.pin_p", "xxx.pre_p1")
        .declared_edge("mmm.pin_p", "xxx.pre_p1")
        .declares_load_after("xxx.pre_p1", "yyy.pre_p2")
        .declared_edge("xxx.pre_p1", "yyy.pre_p2")
        .declares_load_after("nnn.pin_q", "ooo.pre_q1")
        .declared_edge("nnn.pin_q", "ooo.pre_q1")
        .declares_load_after("nnn.pin_q", "ppp.pre_q2")
        .declared_edge("nnn.pin_q", "ppp.pre_q2")
        .declares_load_after("nnn.pin_q", "qqq.pre_q3")
        .declared_edge("nnn.pin_q", "qqq.pre_q3")
        .build();
    let current = builder.insertion_order();
    let rules = RuleSet::new(
        ["lll.leafpin", "mmm.pin_p", "nnn.pin_q"]
            .into_iter()
            .map(|mod_id| {
                Rule::Placement(PlacementRule {
                    mod_id: ModId::new(mod_id),
                    placement: Placement::Top,
                    origin: RuleOrigin::UserDecision,
                    comment: None,
                })
            })
            .collect(),
    );

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    assert_declared_edges_hold(&report, &outcome.order);
    assert_eq!(
        outcome.order.as_slice(),
        &[
            ModId::new("ludeon.rimworld"),
            ModId::new("ludeon.rimworld.royalty"),
            // The leaf pin (closure 1) takes the true first slot...
            ModId::new("lll.leafpin"),
            // ...then `mmm.pin_p`'s own closure (3), immediately preceded
            // by exactly its own two prerequisites and nothing else...
            ModId::new("yyy.pre_p2"),
            ModId::new("xxx.pre_p1"),
            ModId::new("mmm.pin_p"),
            // ...and `nnn.pin_q`'s largest closure (4) furthest from the
            // head.
            ModId::new("ooo.pre_q1"),
            ModId::new("ppp.pre_q2"),
            ModId::new("qqq.pre_q3"),
            ModId::new("nnn.pin_q"),
        ],
        "each Top pin must be preceded by exactly its own closure, largest closure \
         furthest from the head, got order {:?}",
        outcome.order.as_slice()
    );
}

/// The tie rule, which nothing else in this suite reaches: every other
/// closure-move fixture gives its pins *distinct* closure sizes, so
/// `pin_closures_in_processing_order`'s `base_rank` never decides
/// anything and flipping its direction leaves the whole suite green.
/// Here both `Bottom` pins have a two-mod closure (each one dependent),
/// so size ties and only base order is left: the pin *earlier* in the
/// base order is moved first and must therefore end up *further* from the
/// tail, preserving the order `emit::run`'s own heap emitted the two pins
/// in (the `effective_key` rule).
///
/// Verified discriminating: flipping `base_rank`'s `Extreme::Tail` arm to
/// descending yields `[bbb.pin2, ddd.dep2, aaa.pin1, ccc.dep1]` and fails
/// this assertion.
#[test]
fn two_equal_closure_bottom_pins_keep_their_base_order() {
    let builder = ReportBuilder::new()
        .mod_("aaa.pin1")
        .mod_("bbb.pin2")
        .mod_("ccc.dep1")
        .mod_("ddd.dep2");
    let report = builder
        .clone()
        .declares_load_after("ccc.dep1", "aaa.pin1")
        .declared_edge("ccc.dep1", "aaa.pin1")
        .declares_load_after("ddd.dep2", "bbb.pin2")
        .declared_edge("ddd.dep2", "bbb.pin2")
        .build();
    let current = builder.insertion_order();
    let rules = RuleSet::new(
        ["aaa.pin1", "bbb.pin2"]
            .into_iter()
            .map(|mod_id| {
                Rule::Placement(PlacementRule {
                    mod_id: ModId::new(mod_id),
                    placement: Placement::Bottom,
                    origin: RuleOrigin::UserDecision,
                    comment: None,
                })
            })
            .collect(),
    );

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    assert_declared_edges_hold(&report, &outcome.order);
    assert_eq!(
        outcome.order.as_slice(),
        &[
            ModId::new("aaa.pin1"),
            ModId::new("ccc.dep1"),
            ModId::new("bbb.pin2"),
            ModId::new("ddd.dep2"),
        ],
        "two equal-closure Bottom pins must keep their base order, the earlier one \
         further from the tail, got order {:?}",
        outcome.order.as_slice()
    );
}

/// The `Head` mirror of the tie test above, and the only test that
/// reaches `base_rank`'s own `Extreme::Head` arm (`region.len() - 1 -
/// index`). Both `Top` pins have a two-mod closure (each one
/// prerequisite), so the *later* pin in the base order is moved first and
/// must end up *further* from the head — the mirror of the `Tail` rule,
/// and again exactly the base order preserved. `Core`/`Dlc` are present
/// so the region is the real `top_region_start`-bounded one.
///
/// Verified discriminating: flipping `base_rank`'s `Extreme::Head` arm to
/// plain `index` (ascending) yields `[..., ppp.pre2, nnn.pin2, ooo.pre1,
/// mmm.pin1]` and fails this assertion.
#[test]
fn two_equal_closure_top_pins_keep_their_base_order() {
    let builder = ReportBuilder::new()
        .core("ludeon.rimworld")
        .dlc("ludeon.rimworld.royalty")
        .mod_("mmm.pin1")
        .mod_("nnn.pin2")
        .mod_("ooo.pre1")
        .mod_("ppp.pre2");
    let report = builder
        .clone()
        .declares_load_after("mmm.pin1", "ooo.pre1")
        .declared_edge("mmm.pin1", "ooo.pre1")
        .declares_load_after("nnn.pin2", "ppp.pre2")
        .declared_edge("nnn.pin2", "ppp.pre2")
        .build();
    let current = builder.insertion_order();
    let rules = RuleSet::new(
        ["mmm.pin1", "nnn.pin2"]
            .into_iter()
            .map(|mod_id| {
                Rule::Placement(PlacementRule {
                    mod_id: ModId::new(mod_id),
                    placement: Placement::Top,
                    origin: RuleOrigin::UserDecision,
                    comment: None,
                })
            })
            .collect(),
    );

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    assert_declared_edges_hold(&report, &outcome.order);
    assert_eq!(
        outcome.order.as_slice(),
        &[
            ModId::new("ludeon.rimworld"),
            ModId::new("ludeon.rimworld.royalty"),
            ModId::new("ooo.pre1"),
            ModId::new("mmm.pin1"),
            ModId::new("ppp.pre2"),
            ModId::new("nnn.pin2"),
        ],
        "two equal-closure Top pins must keep their base order, the later one further \
         from the head, got order {:?}",
        outcome.order.as_slice()
    );
}

/// A node reachable from *two* pins belongs to both closures, so both
/// block moves carry it — and the later-processed pin's move is the one
/// that decides where it lands. `mmm.pin1`'s closure (3) is processed
/// first, `aaa.pin2`'s (2) second, so `sss.shared` ends up in
/// `aaa.pin2`'s own block at the tail rather than in `mmm.pin1`'s, and
/// every edge still holds.
#[test]
fn a_mod_in_two_pins_closures_lands_in_the_later_processed_pins_block() {
    let builder = ReportBuilder::new()
        .mod_("aaa.pin2")
        .mod_("bbb.only1")
        .mod_("mmm.pin1")
        .mod_("sss.shared");
    let report = builder
        .clone()
        .declares_load_after("bbb.only1", "mmm.pin1")
        .declared_edge("bbb.only1", "mmm.pin1")
        .declares_load_after("sss.shared", "mmm.pin1")
        .declared_edge("sss.shared", "mmm.pin1")
        .declares_load_after("sss.shared", "aaa.pin2")
        .declared_edge("sss.shared", "aaa.pin2")
        .build();
    let current = builder.insertion_order();
    let rules = RuleSet::new(
        ["aaa.pin2", "mmm.pin1"]
            .into_iter()
            .map(|mod_id| {
                Rule::Placement(PlacementRule {
                    mod_id: ModId::new(mod_id),
                    placement: Placement::Bottom,
                    origin: RuleOrigin::UserDecision,
                    comment: None,
                })
            })
            .collect(),
    );

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    assert_declared_edges_hold(&report, &outcome.order);
    assert_eq!(
        outcome.order.as_slice(),
        &[
            ModId::new("mmm.pin1"),
            ModId::new("bbb.only1"),
            ModId::new("aaa.pin2"),
            ModId::new("sss.shared"),
        ],
        "the shared mod must land in aaa.pin2's own (later-processed) block, got order {:?}",
        outcome.order.as_slice()
    );
}

// --- missing mod handling ---------------------------------------------------

#[test]
fn a_missing_mod_is_omitted_by_default() {
    let builder = ReportBuilder::new().mod_("a").mod_("b").missing_mod("gone");
    let mut report = builder.build();
    report.missing_mods = vec![ModId::new("gone")];
    let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("gone"), ModId::new("b")]);

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    assert!(!outcome.order.as_slice().contains(&ModId::new("gone")));
}

#[test]
fn ignoring_a_missing_mod_reinserts_it_after_its_current_predecessor() {
    let builder = ReportBuilder::new().mod_("a").mod_("b");
    let mut report = builder.build();
    report.missing_mods = vec![ModId::new("gone")];
    let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("gone"), ModId::new("b")]);
    let mut overrides = SorterOverrides::default();
    overrides.kept_missing_mods.insert(ModId::new("gone"));

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &overrides,
        &current,
    ));

    let order = outcome.order.as_slice();
    let a_pos = order.iter().position(|id| *id == ModId::new("a")).unwrap();
    let gone_pos = order
        .iter()
        .position(|id| *id == ModId::new("gone"))
        .unwrap();
    assert_eq!(gone_pos, a_pos + 1, "gone must be reinserted right after a");
}

// --- TieBreak: membership is the active list in both modes ---------------
//
// The output is a permutation of `report.mods` — same length, same id
// set — checked with a fixture whose scan directory holds one active and
// one deactivated mod, and a second fixture whose `ModsConfig.xml` lists
// a mod absent from disk.

fn assert_permutation_of_active_mods(report: &rim_analyzer::domain::Report, order: &LoadOrder) {
    let expected: std::collections::BTreeSet<ModId> =
        report.mods.iter().map(|m| m.id.clone()).collect();
    let actual: std::collections::BTreeSet<ModId> = order.as_slice().iter().cloned().collect();
    assert_eq!(
        actual, expected,
        "output must be exactly the active mod set"
    );
    assert_eq!(order.as_slice().len(), report.mods.len(), "no mod twice");
}

/// A mod present on disk but not activated (per `ModsConfig.xml`) is never
/// scanned into `report.mods` at all (the analyzer's own job) — from the
/// sorter's side,
/// that's simply a report with fewer mods than the scan directory holds.
/// This builds exactly that shape (one active mod, one that would be a
/// sibling folder on disk but never enters `report.mods`) and checks the
/// output is a permutation of only the active one, under both `TieBreak`
/// modes.
#[test]
fn a_deactivated_mod_on_disk_never_becomes_a_node_in_either_tie_break_mode() {
    // `report.mods` models only `active.mod` — `deactivated.mod` is
    // deliberately never added, standing in for a mod folder the analyzer
    // found on disk but `ModsConfig.xml` doesn't list.
    let builder = ReportBuilder::new().mod_("active.mod");
    let current = builder.insertion_order();
    let report = builder.build();

    for tie_break in [
        rim_resolve::sort::TieBreak::PreserveCurrent,
        rim_resolve::sort::TieBreak::Rebuild,
    ] {
        let outcome = sort(&SortInput {
            report: &report,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            overrides: &SorterOverrides::default(),
            current: &current,
            enforce: EnforcedLayers::default(),
            tie_break,
        });
        assert_permutation_of_active_mods(&report, &outcome.order);
        assert!(
            !outcome
                .order
                .as_slice()
                .contains(&ModId::new("deactivated.mod")),
            "a mod never added to report.mods must never appear in the output ({tie_break:?})"
        );
    }
}

/// A mod `ModsConfig.xml` lists but that isn't found on disk lands in
/// `report.missing_mods`, never `report.mods` — the sorter drops it from
/// the suggested order by default (see `a_missing_mod_is_omitted_by_default`)
/// regardless of which base key every *other* mod gets.
#[test]
fn a_listed_mod_absent_from_disk_still_yields_a_permutation_of_active_mods_in_either_mode() {
    let builder = ReportBuilder::new().mod_("a").mod_("b");
    let mut report = builder.build();
    report.missing_mods = vec![ModId::new("gone")];
    let current = LoadOrder::new(vec![ModId::new("a"), ModId::new("gone"), ModId::new("b")]);

    for tie_break in [
        rim_resolve::sort::TieBreak::PreserveCurrent,
        rim_resolve::sort::TieBreak::Rebuild,
    ] {
        let outcome = sort(&SortInput {
            report: &report,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            overrides: &SorterOverrides::default(),
            current: &current,
            enforce: EnforcedLayers::default(),
            tie_break,
        });
        assert_permutation_of_active_mods(&report, &outcome.order);
    }
}

/// `Rebuild`'s own base key: with no real edges at all, the emitted order
/// must be exactly display-name order (case-insensitive), falling back to
/// [`ModId`] for two mods sharing a name — regardless of `current`, which
/// this test deliberately scrambles relative to that name order.
#[test]
fn rebuild_orders_unconstrained_mods_by_display_name_case_insensitively() {
    let report = ReportBuilder::new()
        .mod_with("z.mod", |m| m.name = "alpha".to_string())
        .mod_with("y.mod", |m| m.name = "Beta".to_string())
        .mod_with("x.mod", |m| m.name = "charlie".to_string())
        .build();
    // Scrambled relative to name order, to prove `current` plays no part.
    let current = LoadOrder::new(vec![
        ModId::new("x.mod"),
        ModId::new("y.mod"),
        ModId::new("z.mod"),
    ]);

    let outcome = sort(&SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    assert_eq!(
        outcome.order.as_slice(),
        &[
            ModId::new("z.mod"), // "alpha"
            ModId::new("y.mod"), // "Beta" (case-insensitive: b, not B)
            ModId::new("x.mod"), // "charlie"
        ]
    );
}

// --- Rebuild emission does not propagate ---------------------------

/// Three mods, `aaa`/`mmm`/`zzz`, name-ranked in that exact
/// order, with `zzz` declared as `aaa`'s own prerequisite (`aaa` after
/// `zzz`). Run twice against the identical report, varying only
/// `tie_break`:
///
/// - `PreserveCurrent` (current order `[aaa, mmm, zzz]`): unchanged
///   behavior — `zzz` is pulled all the way forward to just before its
///   dependent `aaa`, landing ahead of even `mmm`, exactly as
///   `propagate_keys`'s own minimal-disturbance rule has always done.
/// - `Rebuild`: no propagation at all. `zzz` stays at its own name rank
///   (last of the three); `mmm`, unconstrained, is simply emitted the
///   moment its own name rank comes up, `zzz` only once it's ready
///   (after `mmm`), and `aaa` last of all once `zzz` unblocks it — "a
///   prerequisite is placed when its own name comes up, a dependent
///   waits", never a drag-forward. If `Rebuild` propagated too, a
///   heavily-depended-on but alphabetically-late content mod (here,
///   `zzz`) would be dragged toward the top by an early-named dependent,
///   the way a retexture pack's own dependency on its content owner
///   drags the owner up near the top of the order.
///
/// What would make this fail: changing `emit::run` to always call
/// `propagate_keys` collapses the two branches into the same
/// `[zzz, aaa, mmm]` order.
#[test]
fn rebuild_never_propagates_while_preserve_current_still_does() {
    let report = ReportBuilder::new()
        .mod_("aaa")
        .mod_("mmm")
        .mod_("zzz")
        .declared_edge("aaa", "zzz") // aaa after zzz: zzz is aaa's prerequisite.
        .build();
    let current = LoadOrder::new(vec![
        ModId::new("aaa"),
        ModId::new("mmm"),
        ModId::new("zzz"),
    ]);

    let preserve_current = sort(&SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::PreserveCurrent,
    });
    assert_eq!(
        preserve_current.order.as_slice(),
        &[ModId::new("zzz"), ModId::new("aaa"), ModId::new("mmm")],
        "PreserveCurrent must still pull zzz forward to just before its dependent aaa"
    );

    let rebuild = sort(&SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });
    assert_eq!(
        rebuild.order.as_slice(),
        &[ModId::new("mmm"), ModId::new("zzz"), ModId::new("aaa")],
        "Rebuild must never propagate: zzz stays at its own name rank, \
         aaa simply waits for it"
    );
}

/// A real-install shape: a punctuation-tagged
/// retexture pack (`[CF] Retexture Pack`) declared `loadAfter` a
/// late-alphabetical content owner (`ZZZ Ultimate Content Pack`), plus an
/// unrelated neutral mod. Two things must both hold under `Rebuild`:
///
/// - The bracket tag is stripped before ranking (leading only),
///   so the retexture pack ranks by "retexture pack" (`r`), not `"[cf] ..."`
///   (`[` sorts before every letter) — it does **not** cluster at the very
///   top the way an unstripped name would.
/// - The content owner is never dragged forward by its dependent (the
///   emit.rs half of the rule): it sits at its own natural late rank, right
///   after the neutral mod, not pulled toward the retexture pack's rank.
///
/// What would make this fail: dropping the leading-tag strip
/// (`normalized_rebuild_name`) would put the retexture pack first, ahead
/// of the neutral mod; removing the no-propagation rule (`emit.rs`) would
/// drag the content owner up to the very front instead of leaving it
/// after the neutral mod.
#[test]
fn rebuild_leaves_a_retexture_packs_content_owner_at_its_own_rank() {
    let report = ReportBuilder::new()
        .mod_with("aaa.neutral", |m| m.name = "AAA Neutral Mod".to_string())
        .mod_with("example.retexture", |m| {
            m.name = "[CF] Retexture Pack".to_string()
        })
        .mod_with("zzz.content", |m| {
            m.name = "ZZZ Ultimate Content Pack".to_string();
        })
        .declared_edge("example.retexture", "zzz.content") // retexture after content.
        .build();
    let current = LoadOrder::new(vec![
        ModId::new("example.retexture"),
        ModId::new("aaa.neutral"),
        ModId::new("zzz.content"),
    ]);

    let outcome = sort(&SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    assert_eq!(
        outcome.order.as_slice(),
        &[
            ModId::new("aaa.neutral"),
            ModId::new("zzz.content"),
            ModId::new("example.retexture"),
        ],
        "the retexture pack must leave the top (stripped-name ranked last \
         of the three) and must not drag its content owner up with it"
    );
}

// --- Layer::Inferred precedence -------------------------------------

/// Three independent pairs in one
/// report, each isolating one side of `Layer::Inferred`'s own precedence
/// (`SteamDb` < `Inferred` < `Soft`):
///
/// - `u1`/`u2`: a `RetextureAfterOwner` `Inferred` edge (`u1` after `u2`)
///   directly contradicts the user's own `Reorder` decision (`UserDecision`,
///   an earlier layer) — the decision must win.
/// - `r1`/`r2`: a `PatchRemovedNode` `Inferred` edge (`r1` after `r2`)
///   directly contradicts an imported `RimSortUser` pair rule (also an
///   earlier layer) — the rule must win.
/// - `aname`/`zname`: a lone `DefOverrideAfterOrigin` `Inferred` edge
///   (`aname` after `zname`), no contradiction at all, under `Rebuild`
///   (so nothing but the edge and each mod's own name rank is in play) —
///   `zname` must still load before `aname` even though plain
///   alphabetical order would put `aname` first with no edge at all:
///   `Layer::Inferred` is a real, enforced constraint (`EnforcedLayers::inferred`
///   defaults `true`), not merely advisory.
///
/// What would make this fail: folding `Layer::Inferred` into
/// `Awareness` (never enforced, never able to beat name order) breaks the
/// third pair; changing `EnforcedLayers::inferred`'s own default to
/// `false` breaks it the same way; misordering `Inferred`
/// relative to `UserDecision`/`RimSortUser` in `LAYER_ORDER` breaks the
/// first two.
#[test]
fn an_inferred_edge_loses_to_user_decision_and_rimsort_user_but_beats_name_order() {
    let report = ReportBuilder::new()
        .mod_("u1")
        .mod_("u2")
        .edge("u1", "u2", EdgeKind::RetextureAfterOwner, true) // Inferred: u1 after u2.
        .mod_("r1")
        .mod_("r2")
        .edge("r1", "r2", EdgeKind::PatchRemovedNode, true) // Inferred: r1 after r2.
        .mod_with("aname", |m| m.name = "aname".to_string())
        .mod_with("zname", |m| m.name = "zname".to_string())
        .edge("aname", "zname", EdgeKind::DefOverrideAfterOrigin, true) // Inferred: aname after zname.
        // A fourth pair, isolating
        // `Layer::Inferred`'s own precedence over `Layer::Soft` —
        // `LAYER_ORDER` places `Inferred` *before* `Soft`, so this is the
        // one direction the other three pairs above never exercise
        // (`u1`/`r1`/`aname` all show Inferred *losing* or merely beating
        // bare name order). Moving `Inferred` to the very end of
        // `LAYER_ORDER` would leave this whole test green except for this
        // one assertion.
        .mod_("s1")
        .mod_("s2")
        .edge("s1", "s2", EdgeKind::DefOverrideAfterOrigin, true) // Inferred: s1 after s2.
        .edge("s2", "s1", EdgeKind::AssemblyRef, false) // Soft: s2 after s1 (contradicting).
        .build();
    let current = LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect::<Vec<_>>());

    let mut overrides = SorterOverrides::default();
    // u2 after u1: the opposite of the Inferred edge above.
    overrides
        .reorders
        .push((ModId::new("u2"), ModId::new("u1")));

    let rules = RuleSet::new(vec![Rule::Pair(PairRule {
        // r2 after r1: the opposite of the Inferred edge above.
        after: ModId::new("r2"),
        before: ModId::new("r1"),
        origin: RuleOrigin::RimSortUser,
        comment: None,
        overrides_declared: false,
    })]);

    let outcome = sort(&SortInput {
        report: &report,
        rules: &rules,
        tagging: &Tagging::default(),
        overrides: &overrides,
        current: &current,
        // `soft: true` so the contradicting Soft edge (s2/s1) is a real
        // constraint too, not merely advisory — otherwise it could never
        // contradict anything in the first place.
        enforce: EnforcedLayers {
            soft: true,
            awareness: false,
            inferred: true,
        },
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    let order = outcome.order.as_slice();
    let pos = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();

    assert!(
        pos("u1") < pos("u2"),
        "the user's own Reorder decision must win over the Inferred edge"
    );
    assert!(
        pos("r1") < pos("r2"),
        "the imported RimSortUser rule must win over the Inferred edge"
    );
    assert!(
        pos("zname") < pos("aname"),
        "the Inferred edge must be enforced even against plain alphabetical \
         name order, since EnforcedLayers::inferred defaults to true"
    );
    assert!(
        pos("s2") < pos("s1"),
        "the Inferred edge must beat a contradicting Soft edge, since \
         Layer::Inferred precedes Layer::Soft in LAYER_ORDER"
    );
}

// --- propagated emission keys -----------------------------------------------

#[test]
fn a_prerequisite_far_behind_two_dependents_is_pulled_forward_to_just_before_the_earliest() {
    // `prereq` sits at the very end of the current order, but two mods far
    // earlier (`dep1` at 5, `dep2` at 8) both need it loaded first. Pushing
    // either dependent back to `prereq`'s own position would disturb every
    // filler mod between; pulling `prereq` forward instead moves exactly
    // one mod. `dep1`'s key (5) beats `dep2`'s (8), so `prereq` lands
    // immediately before `dep1` specifically, and every other mod's
    // relative order (fillers and both dependents alike) is preserved
    // exactly as in `current` -- only `prereq` moved through them.
    let mut builder = ReportBuilder::new();
    for i in 0..5 {
        builder = builder.mod_(&format!("fillerA{i:02}"));
    }
    builder = builder.mod_("dep1");
    for i in 0..2 {
        builder = builder.mod_(&format!("fillerB{i:02}"));
    }
    builder = builder.mod_("dep2");
    for i in 0..3 {
        builder = builder.mod_(&format!("fillerC{i:02}"));
    }
    builder = builder
        .mod_("prereq")
        .declared_edge("dep1", "prereq")
        .declared_edge("dep2", "prereq");
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    let order = outcome.order.as_slice();
    let prereq_pos = order
        .iter()
        .position(|id| *id == ModId::new("prereq"))
        .unwrap();
    let dep1_pos = order
        .iter()
        .position(|id| *id == ModId::new("dep1"))
        .unwrap();
    assert_eq!(
        prereq_pos + 1,
        dep1_pos,
        "prereq must land immediately before dep1, the dependent with the earliest key"
    );

    // Every mod except `prereq` keeps its exact relative order versus
    // `current` -- only `prereq` moved through the sequence.
    let without_prereq = |order: &[ModId]| -> Vec<ModId> {
        order
            .iter()
            .filter(|id| **id != ModId::new("prereq"))
            .cloned()
            .collect()
    };
    assert_eq!(without_prereq(order), without_prereq(current.as_slice()));

    let prereq_explanation = &outcome.placements[&ModId::new("prereq")];
    assert_eq!(prereq_explanation.tie_break.effective_key, 5);
    assert_eq!(prereq_explanation.tie_break.current_position, Some(12));
    assert!(matches!(&prereq_explanation.tie_break.pulled_forward_by,
        Some(edge) if edge.after == ModId::new("dep1")
    ));

    // Neither dependent was itself pulled forward.
    for dep in ["dep1", "dep2"] {
        let explanation = &outcome.placements[&ModId::new(dep)];
        assert!(explanation.tie_break.pulled_forward_by.is_none());
        assert_eq!(
            Some(explanation.tie_break.effective_key),
            explanation.tie_break.current_position
        );
    }
}

// --- Emission direction for a violated edge (pull vs. push) -----------

#[test]
fn a_chain_of_prerequisites_meets_in_the_middle_at_its_own_pivot() {
    // a must load before b, which must load before c. c sits early in the
    // current order; a and b sit much later. A pull-only rule would drag
    // the whole chain forward to sit right before c's own early position
    // (3 mods moved to `effective_key: 5`).
    //
    // The emission direction picks *which* hop pulls and which pushes, per
    // edge, by comparing each side's own propagating closure size:
    // - edge a->b: upstream(a) = 0 (a has no propagating prerequisite of
    //   its own), downstream(b) = 1 (b has c downstream) — downstream is
    //   *not* smaller, so this hop stays Pull: a is
    //   pulled forward to sit right before b.
    // - edge b->c: upstream(b) = 1 (a is upstream of b), downstream(c) = 0
    //   (c has no propagating dependent of its own) — downstream *is*
    //   smaller, so this hop switches to Push: c is pushed back to sit
    //   right after b, instead of b being pulled forward to c.
    // Net effect: b — the pivot both decisions converge on — stays
    // exactly at its own current position (moved 0 slots, not pulled to
    // c's early spot), while a and c both move to meet it there. a, b,
    // c stay contiguous and in the right relative order either way, and
    // this disturbs one *fewer* mod overall than a pull-only rule (10
    // moved instead of 11, in this exact fixture) — a small instance of
    // the real-install shape where one dependency drags a long chain.
    let mut builder = ReportBuilder::new();
    for i in 0..5 {
        builder = builder.mod_(&format!("fillerA{i:02}"));
    }
    builder = builder.mod_("c");
    for i in 0..4 {
        builder = builder.mod_(&format!("fillerB{i:02}"));
    }
    builder = builder.mod_("b");
    for i in 0..4 {
        builder = builder.mod_(&format!("fillerC{i:02}"));
    }
    builder = builder
        .mod_("a")
        .declared_edge("b", "a") // a before b
        .declared_edge("c", "b"); // b before c
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    let order = outcome.order.as_slice();
    let pos = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();
    let (a_pos, b_pos, c_pos) = (pos("a"), pos("b"), pos("c"));
    assert_eq!(a_pos + 1, b_pos, "a must sit right before b");
    assert_eq!(b_pos + 1, c_pos, "b must sit right before c");

    let b_current_pos = current
        .as_slice()
        .iter()
        .position(|m| *m == ModId::new("b"))
        .unwrap();
    assert_eq!(
        b_pos, b_current_pos,
        "b is the pivot both hops converge on — it never moves \
         from its own current position, unlike a pull-only emission's \
         behaviour (which pulled it all the way to c's early spot)"
    );

    let a_explanation = &outcome.placements[&ModId::new("a")];
    let b_explanation = &outcome.placements[&ModId::new("b")];
    let c_explanation = &outcome.placements[&ModId::new("c")];
    // a's own hop (a->b) is still decided Pull, so it still reports a
    // pulled-forward attribution, just to b's own (unmoved)
    // position (10) rather than c's (5).
    assert_eq!(a_explanation.tie_break.effective_key, 10);
    assert!(matches!(&a_explanation.tie_break.pulled_forward_by,
        Some(edge) if edge.after == ModId::new("b")
    ));
    // b's own hop (b->c) is now decided Push, so b is no longer the one
    // being pulled — its own `pulled_forward_by` is `None`, and its
    // `effective_key` equals its own base/current position exactly.
    assert_eq!(b_explanation.tie_break.effective_key, 10);
    assert!(b_explanation.tie_break.pulled_forward_by.is_none());
    // c moved (its own `effective_key` differs from its current
    // position, 5), but was *pushed*, not pulled — `pulled_forward_by`
    // has no equivalent field for this direction (see `push_keys`'s own
    // doc comment), so it stays `None` here despite c genuinely having
    // moved.
    assert_eq!(c_explanation.tie_break.effective_key, 10);
    assert_eq!(c_explanation.tie_break.current_position, Some(5));
    assert!(c_explanation.tie_break.pulled_forward_by.is_none());
}

#[test]
fn a_dependent_with_no_downstream_of_its_own_is_pushed_back_past_a_prerequisites_long_upstream_chain()
 {
    // The motivating shape for push, stated directly: a late prerequisite
    // (`prereq`) with a five-mod chain of
    // its own upstream prerequisites (`p1..p5`, all sitting right next to
    // it) versus an early dependent (`dep`) with no downstream chain of
    // its own at all. Pulling `prereq` forward to `dep`'s position would
    // drag the whole five-mod chain forward with it — one dependency
    // moving many mods. `dep`'s downstream closure (0) is smaller than
    // `prereq`'s upstream closure (5), so the sorter picks Push instead:
    // pushing `dep` back past `prereq` costs exactly one move.
    let mut builder = ReportBuilder::new();
    builder = builder.mod_("dep");
    for i in 0..3 {
        builder = builder.mod_(&format!("filler{i:02}"));
    }
    for i in 1..=5 {
        builder = builder.mod_(&format!("p{i}"));
    }
    builder = builder.mod_("prereq");
    for hop in 1..5 {
        // p{hop} before p{hop+1}
        builder = builder.declared_edge(&format!("p{}", hop + 1), &format!("p{hop}"));
    }
    builder = builder
        .declared_edge("prereq", "p5") // p5 before prereq
        .declared_edge("dep", "prereq"); // prereq before dep
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    let order = outcome.order.as_slice();
    let current_pos = |id: &str| {
        current
            .as_slice()
            .iter()
            .position(|m| *m == ModId::new(id))
            .unwrap()
    };
    let sorted_pos = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();

    // `dep` is the only mod that actually moves — pulled out from the
    // very front and reinserted right after `prereq` — so everything
    // else's own *absolute* index necessarily shifts down by exactly one
    // slot to fill the gap it left. `without_dep` (mirroring the older
    // `a_prerequisite_far_behind_two_dependents...` test's own
    // `without_prereq` technique) checks the thing Push actually promises:
    // every other mod's *relative* order to every other is untouched —
    // the whole upstream chain, prereq included, never gets dragged
    // anywhere.
    let without_dep = |order: &[ModId]| -> Vec<ModId> {
        order
            .iter()
            .filter(|id| **id != ModId::new("dep"))
            .cloned()
            .collect()
    };
    assert_eq!(without_dep(order), without_dep(current.as_slice()));

    // dep is the only mod that moves, landing right after prereq.
    assert_eq!(sorted_pos("dep"), sorted_pos("prereq") + 1);
    assert_ne!(
        sorted_pos("dep"),
        current_pos("dep"),
        "dep must actually have moved, not coincidentally landed back where it started"
    );

    let dep_explanation = &outcome.placements[&ModId::new("dep")];
    assert_eq!(
        dep_explanation.tie_break.effective_key,
        current_pos("prereq")
    );
    assert!(
        dep_explanation.tie_break.pulled_forward_by.is_none(),
        "dep was pushed, not pulled — pulled_forward_by has no equivalent \
         field for this direction, see push_keys's own doc comment"
    );
}

// --- PreserveCurrent propagates through author/user edges only ----

/// A `RetextureAfterOwner`
/// `Inferred` edge whose prerequisite (`content`) sits late in the
/// current order and whose dependent (`retexture`) sits at the very
/// front, with neither side having any propagating chain of its own — so
/// pull and push cost exactly the same (both bare base-key distances),
/// and the `Inferred` tie rule decides: a tie pushes the dependent, never
/// pulls the prerequisite. `content` stays almost exactly where it
/// started; `retexture` is the one that moves, waiting for `content` and
/// landing right after it — and the edge itself must still hold in the
/// final order (the tie rule decides *how* an `Inferred` edge is
/// satisfied, never *whether*).
///
/// What would make this fail: flipping `Inferred`'s own tie rule to
/// prefer `Pull` (matching every author/user layer's own tie rule)
/// would pull `content` forward to sit just before `retexture` at the
/// very front instead, displacing every filler between them.
#[test]
fn an_inferred_tie_still_moves_the_dependent_not_the_prerequisite() {
    let mut builder = ReportBuilder::new().mod_("retexture");
    for i in 0..5 {
        builder = builder.mod_(&format!("filler{i:02}"));
    }
    // Inferred: retexture after content.
    builder =
        builder
            .mod_("content")
            .edge("retexture", "content", EdgeKind::RetextureAfterOwner, true);
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    let order = outcome.order.as_slice();
    let pos = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();

    assert!(
        pos("content") < pos("retexture"),
        "the Inferred edge must still be satisfied: content before retexture"
    );
    assert_eq!(
        pos("retexture"),
        6,
        "retexture (the dependent) must be the one that moves, ending up \
         last, not pulling content forward to sit near position 0"
    );
    assert_eq!(
        pos("content"),
        5,
        "content (the prerequisite) must stay almost exactly at its own \
         base position, not get dragged forward to just before retexture"
    );

    let content_explanation = &outcome.placements[&ModId::new("content")];
    assert!(
        content_explanation.tie_break.pulled_forward_by.is_none(),
        "content must never be reported as pulled forward by the Inferred edge"
    );
}

// --- Emission direction through Inferred edges -------------------------

/// The biomescore shape: `prereq` sits far
/// behind (no ancestors of its own), while its dependent `dep` drags a
/// two-mod `Declared` chain (`d1`, `d2`). Pushing `dep` back would push
/// three mods (`dep`, `d1`, `d2`); pulling `prereq` forward moves exactly
/// one. Pull costs 9 (`10 - 1`), push costs 24 (`9 + 8 + 7`), so pull
/// wins even though the edge is merely `Inferred`.
///
/// What would make this fail: leaving `Layer::Inferred` out of
/// `propagates` (today's behavior) — `dep`, `d1` and `d2` all get pushed
/// back past `prereq` instead.
#[test]
fn an_inferred_edge_pulls_a_lone_late_prerequisite_forward_when_the_dependent_drags_a_declared_chain()
 {
    let mut builder = ReportBuilder::new()
        .mod_("filler0")
        .mod_("dep")
        .mod_("d1")
        .mod_("d2");
    for i in 1..=6 {
        builder = builder.mod_(&format!("filler{i}"));
    }
    builder = builder
        .mod_("prereq")
        .declared_edge("d1", "dep") // d1 after dep.
        .declared_edge("d2", "dep") // d2 after dep.
        .edge("dep", "prereq", EdgeKind::RetextureAfterOwner, true); // Inferred: dep after prereq.
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    let order = outcome.order.as_slice();
    let pos = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();

    assert_eq!(
        pos("prereq") + 1,
        pos("dep"),
        "prereq must land immediately before dep"
    );

    let without_prereq = |order: &[ModId]| -> Vec<ModId> {
        order
            .iter()
            .filter(|id| **id != ModId::new("prereq"))
            .cloned()
            .collect()
    };
    assert_eq!(
        without_prereq(order),
        without_prereq(current.as_slice()),
        "only prereq moved -- dep, d1 and d2 keep their own relative order"
    );

    let prereq_explanation = &outcome.placements[&ModId::new("prereq")];
    assert!(
        matches!(&prereq_explanation.tie_break.pulled_forward_by,
            Some(edge) if edge.after == ModId::new("dep")),
        "prereq must be reported as pulled forward by the Inferred edge"
    );
}

/// The mirror shape: `prereq` itself drags a three-mod `Declared`
/// ancestor chain (`p1..p3`), while its dependent `dep` has no downstream
/// of its own. Pulling `prereq` forward would drag `p1..p3` along (cost
/// 30 = `9 + 6 + 7 + 8`); pushing `dep` back costs 9 (`9 - 0`), so push
/// wins.
#[test]
fn an_inferred_edge_pushes_the_dependent_back_when_the_prerequisite_drags_a_longer_late_chain() {
    let mut builder = ReportBuilder::new().mod_("dep");
    for i in 0..5 {
        builder = builder.mod_(&format!("filler{i}"));
    }
    builder = builder
        .mod_("p1")
        .mod_("p2")
        .mod_("p3")
        .mod_("prereq")
        .declared_edge("prereq", "p1") // prereq after p1.
        .declared_edge("prereq", "p2") // prereq after p2.
        .declared_edge("prereq", "p3") // prereq after p3.
        .edge("dep", "prereq", EdgeKind::RetextureAfterOwner, true); // Inferred: dep after prereq.
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    let order = outcome.order.as_slice();
    let current_pos = |id: &str| current.position(&ModId::new(id)).unwrap();
    let sorted_pos = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();

    let without_dep = |order: &[ModId]| -> Vec<ModId> {
        order
            .iter()
            .filter(|id| **id != ModId::new("dep"))
            .cloned()
            .collect()
    };
    assert_eq!(
        without_dep(order),
        without_dep(current.as_slice()),
        "only dep moved -- prereq and its whole upstream chain stay put"
    );
    assert_eq!(
        sorted_pos("dep"),
        sorted_pos("prereq") + 1,
        "dep must land right after prereq"
    );
    assert_ne!(
        sorted_pos("dep"),
        current_pos("dep"),
        "dep must actually have moved"
    );

    let dep_explanation = &outcome.placements[&ModId::new("dep")];
    assert_eq!(
        dep_explanation.tie_break.effective_key,
        current_pos("prereq")
    );
    assert!(
        dep_explanation.tie_break.pulled_forward_by.is_none(),
        "dep was pushed, not pulled -- pulled_forward_by has no equivalent for this direction"
    );
}

/// Counting the *whole* chain each option drags, not just how many mods
/// move: `dep`'s 3-mod `Declared` descendant chain sits right next to it
/// (close, so pushing it back past a far-away `prereq` is expensive:
/// `20 + 19 + 18 + 17 = 74`), while `prereq`'s own 1-mod ancestor chain
/// sits right next to it too (pulling it forward costs only
/// `20 + 19 = 39`). A per-mod "1 vs 1" comparison would tie and push;
/// the position-aware sum picks Pull instead.
#[test]
fn the_direction_counts_the_whole_chain_each_option_drags_not_one_mod() {
    let mut builder = ReportBuilder::new()
        .mod_("dep")
        .mod_("d1")
        .mod_("d2")
        .mod_("d3");
    for i in 0..15 {
        builder = builder.mod_(&format!("filler{i:02}"));
    }
    builder = builder
        .mod_("lateanc")
        .mod_("prereq")
        .declared_edge("d1", "dep") // d1 after dep.
        .declared_edge("d2", "dep") // d2 after dep.
        .declared_edge("d3", "dep") // d3 after dep.
        .declared_edge("prereq", "lateanc") // prereq after lateanc.
        .edge("dep", "prereq", EdgeKind::RetextureAfterOwner, true); // Inferred: dep after prereq.
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    let order = outcome.order.as_slice();
    let pos = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();

    assert_eq!(
        pos("lateanc"),
        0,
        "lateanc, prereq's own ancestor, moves to the very front"
    );
    assert_eq!(pos("lateanc") + 1, pos("prereq"));
    assert_eq!(pos("prereq") + 1, pos("dep"));
    assert_eq!(pos("dep") + 1, pos("d1"));
    assert_eq!(pos("d1") + 1, pos("d2"));
    assert_eq!(
        pos("d2") + 1,
        pos("d3"),
        "lateanc, prereq, dep, d1, d2 and d3 land as one contiguous block, \
         Pull having won over Push"
    );
}

/// A `Declared` descendant already sitting *after* the prerequisite
/// contributes nothing to a push's cost: `dep`'s two descendants sit at
/// positions 15/16, already past `prereq` at position 10, so pushing
/// `dep` back to meet `prereq` costs only `10` (nothing else moves with
/// it) — cheaper than pulling `prereq` forward past its own ancestor
/// (`10 + 8 = 18`). A same-count "count the mods, not the distance"
/// heuristic that also charged those already-satisfied descendants would
/// wrongly favor a pull instead.
#[test]
fn descendants_already_after_the_prerequisite_do_not_count_against_a_push() {
    let mut builder = ReportBuilder::new().mod_("dep");
    for i in 0..7 {
        builder = builder.mod_(&format!("fillerA{i}"));
    }
    builder = builder.mod_("anc").mod_("fillerB0").mod_("prereq");
    for i in 0..4 {
        builder = builder.mod_(&format!("fillerC{i}"));
    }
    builder = builder
        .mod_("d1")
        .mod_("d2")
        .declared_edge("prereq", "anc") // prereq after anc.
        .declared_edge("d1", "dep") // d1 after dep.
        .declared_edge("d2", "dep") // d2 after dep.
        .edge("dep", "prereq", EdgeKind::RetextureAfterOwner, true); // Inferred: dep after prereq.
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    let order = outcome.order.as_slice();
    let current_pos = |id: &str| current.position(&ModId::new(id)).unwrap();
    let sorted_pos = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();

    let without_dep = |order: &[ModId]| -> Vec<ModId> {
        order
            .iter()
            .filter(|id| **id != ModId::new("dep"))
            .cloned()
            .collect()
    };
    assert_eq!(
        without_dep(order),
        without_dep(current.as_slice()),
        "only dep moved -- anc, prereq, and dep's own descendants keep their relative order"
    );
    assert_eq!(sorted_pos("dep"), sorted_pos("prereq") + 1);

    let dep_explanation = &outcome.placements[&ModId::new("dep")];
    assert_eq!(
        dep_explanation.tie_break.effective_key,
        current_pos("prereq")
    );
    assert!(dep_explanation.tie_break.pulled_forward_by.is_none());
}

/// See [`an_inferred_tie_still_moves_the_dependent_not_the_prerequisite`]
/// (this file, above) for the case where pull and push cost the same and
/// the `Inferred` tie rule decides.
///
/// A `Declared` pull chain continues through a *satisfied* `Inferred`
/// hop: `e` depends on `d` (`Declared`), and `d` in turn depends on `p`
/// via an `Inferred` edge already satisfied in the current order
/// (`p` at 5, `d` at 10). Pulling `d` forward to `e`'s position must
/// carry `p` along too — the pull's own cost already counts it
/// (`10 + 5 = 15`), cheaper than pushing `e` and its two descendants back
/// (`10 + 9 + 8 = 27`).
///
/// What would make this fail: excluding `Inferred` from the propagating
/// subgraph `Anc`/`Desc` are built over would stop the chain at the `p`/`d`
/// hop, leaving `d` waiting for `p` at its own base key and pushing `e`,
/// `e1` and `e2` behind every filler instead.
#[test]
fn a_declared_pull_chain_continues_through_a_satisfied_inferred_hop() {
    let builder = ReportBuilder::new()
        .mod_("e")
        .mod_("e1")
        .mod_("e2")
        .mod_("filler0")
        .mod_("filler1")
        .mod_("p")
        .mod_("filler2")
        .mod_("filler3")
        .mod_("filler4")
        .mod_("filler5")
        .mod_("d")
        .declared_edge("e1", "e") // e1 after e.
        .declared_edge("e2", "e") // e2 after e.
        .declared_edge("e", "d") // e after d.
        .edge("d", "p", EdgeKind::RetextureAfterOwner, true); // Inferred: d after p (satisfied).
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    let order = outcome.order.as_slice();
    let pos = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();

    assert_eq!(
        pos("p"),
        0,
        "p, d's own Inferred prerequisite, moves to the very front"
    );
    assert_eq!(pos("p") + 1, pos("d"));
    assert_eq!(pos("d") + 1, pos("e"));
    assert_eq!(pos("e") + 1, pos("e1"));
    assert_eq!(
        pos("e1") + 1,
        pos("e2"),
        "p, d, e, e1 and e2 land as one contiguous block at the front"
    );
}

/// One leaf-leaf pair joined by both a `Declared` edge and an `Inferred`
/// edge, both `d after p`. Pull and push cost the same for this pair
/// (each side is a bare leaf), so the tie rule decides — and it takes
/// the *strongest* layer present, not the weakest: the `Declared` edge's
/// own tie rule (pull wins) governs, even though an `Inferred`-only pair
/// in the same shape would push instead (see
/// [`an_inferred_tie_still_moves_the_dependent_not_the_prerequisite`]).
#[test]
fn parallel_declared_and_inferred_edges_between_one_pair_resolve_in_one_direction() {
    let mut builder = ReportBuilder::new().mod_("d");
    for i in 0..4 {
        builder = builder.mod_(&format!("filler{i}"));
    }
    builder = builder
        .mod_("p")
        .declared_edge("d", "p") // Declared: d after p.
        .edge("d", "p", EdgeKind::RetextureAfterOwner, true); // Inferred: d after p (parallel).
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    let order = outcome.order.as_slice();
    let pos = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();

    assert_eq!(
        pos("p") + 1,
        pos("d"),
        "p must be pulled forward to sit immediately before d"
    );

    let p_explanation = &outcome.placements[&ModId::new("p")];
    assert!(
        p_explanation.tie_break.pulled_forward_by.is_some(),
        "p must be reported as pulled forward"
    );

    let d_explanation = &outcome.placements[&ModId::new("d")];
    assert_eq!(
        Some(d_explanation.tie_break.effective_key),
        d_explanation.tie_break.current_position,
        "d must keep its own base key -- it is not also pushed"
    );
}

/// Reuses [`an_inferred_edge_pulls_a_lone_late_prerequisite_forward_when_the_dependent_drags_a_declared_chain`]'s
/// own fixture (`prereq` cost-chosen `Pull` past `dep`'s `Declared`
/// chain), adding a `Hard` edge that makes `prereq` itself depend on `h`,
/// with `h` sitting even later than `prereq` — a genuinely violated
/// `Hard` edge the cost heuristic has no say over. Whatever direction the
/// heuristic estimates for either edge, Kahn's in-degree tracking still
/// enforces every accepted edge strictly: this pins that the estimate
/// may be wrong, but correctness never is.
#[test]
fn an_inferred_pull_never_breaks_a_hard_or_declared_edge() {
    let mut builder = ReportBuilder::new()
        .mod_("filler0")
        .mod_("dep")
        .mod_("d1")
        .mod_("d2");
    for i in 1..=6 {
        builder = builder.mod_(&format!("filler{i}"));
    }
    builder = builder
        .mod_("prereq")
        .mod_("h")
        .declared_edge("d1", "dep")
        .declared_edge("d2", "dep")
        .edge("dep", "prereq", EdgeKind::RetextureAfterOwner, true) // Inferred: dep after prereq.
        .hard_edge("prereq", "h"); // Hard: prereq after h (violated: h sits after prereq).
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    for edge_report in &report.edges {
        assert_eq!(
            evaluate::ordering_status(
                &edge_report.edge.after,
                &edge_report.edge.before,
                &outcome.order
            ),
            EdgeStatus::Satisfied,
            "every accepted edge must hold in the final order: {} after {}",
            edge_report.edge.after,
            edge_report.edge.before
        );
    }

    let expected: BTreeSet<ModId> = report.mods.iter().map(|m| m.id.clone()).collect();
    let actual: BTreeSet<ModId> = outcome.order.as_slice().iter().cloned().collect();
    assert_eq!(
        actual, expected,
        "the output must be a permutation of the input"
    );
    assert_eq!(outcome.order.as_slice().len(), report.mods.len());
}

/// A `Top` pin plus an unrelated `Body` pair whose `Inferred` edge is
/// cost-chosen `Pull` (the same fixture as
/// [`an_inferred_edge_pulls_a_lone_late_prerequisite_forward_when_the_dependent_drags_a_declared_chain`]).
/// The pin still reaches the very front of the order, the pulled mod
/// stays exactly where the `Body`-only version put it, and neither is
/// promoted into the other's tier.
#[test]
fn a_top_pin_keeps_the_head_after_an_inferred_pull() {
    let mut builder = ReportBuilder::new()
        .mod_("filler0")
        .mod_("dep")
        .mod_("d1")
        .mod_("d2");
    for i in 1..=6 {
        builder = builder.mod_(&format!("filler{i}"));
    }
    builder = builder
        .mod_("prereq")
        .mod_("top.mod")
        .declared_edge("d1", "dep")
        .declared_edge("d2", "dep")
        .edge("dep", "prereq", EdgeKind::RetextureAfterOwner, true); // Inferred: dep after prereq.
    let current = builder.insertion_order();
    let report = builder.build();
    let rules = RuleSet::new(vec![Rule::Placement(PlacementRule {
        mod_id: ModId::new("top.mod"),
        placement: Placement::Top,
        origin: RuleOrigin::UserDecision,
        comment: None,
    })]);

    let outcome = sort(&base_input(
        &report,
        &rules,
        &Tagging::default(),
        &SorterOverrides::default(),
        &current,
    ));

    let order = outcome.order.as_slice();
    assert_eq!(
        order[0],
        ModId::new("top.mod"),
        "the Top pin must sit at the very front"
    );
    for id in &order[1..] {
        assert_ne!(*id, ModId::new("top.mod"));
    }

    let pos = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();
    assert_eq!(
        pos("prereq") + 1,
        pos("dep"),
        "the Body pair's own Pull decision is unaffected by the unrelated Top pin"
    );
    assert_eq!(
        outcome.placements[&ModId::new("prereq")].tier,
        Tier::Body,
        "the pulled mod must stay in Body, not get promoted into Top"
    );
}

/// `Rebuild` never calls `propagate_keys`/`sort::direction` at all (same
/// fixture as
/// [`an_inferred_edge_pulls_a_lone_late_prerequisite_forward_when_the_dependent_drags_a_declared_chain`],
/// under `TieBreak::Rebuild` this time): plain name-ranked Kahn, the
/// dependent simply waiting for its prerequisite, exactly as before this
/// change.
#[test]
fn rebuild_is_unaffected_by_inferred_emission_direction() {
    let mut builder = ReportBuilder::new()
        .mod_("filler0")
        .mod_("dep")
        .mod_("d1")
        .mod_("d2");
    for i in 1..=6 {
        builder = builder.mod_(&format!("filler{i}"));
    }
    builder = builder
        .mod_("prereq")
        .declared_edge("d1", "dep")
        .declared_edge("d2", "dep")
        .edge("dep", "prereq", EdgeKind::RetextureAfterOwner, true); // Inferred: dep after prereq.
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: rim_resolve::sort::TieBreak::Rebuild,
    });

    assert_eq!(
        outcome.order.as_slice(),
        &[
            ModId::new("filler0"),
            ModId::new("filler1"),
            ModId::new("filler2"),
            ModId::new("filler3"),
            ModId::new("filler4"),
            ModId::new("filler5"),
            ModId::new("filler6"),
            ModId::new("prereq"),
            ModId::new("dep"),
            ModId::new("d1"),
            ModId::new("d2"),
        ],
        "Rebuild must produce plain name-ranked Kahn: dep simply waits for prereq"
    );
    for explanation in outcome.placements.values() {
        assert!(
            explanation.tie_break.pulled_forward_by.is_none(),
            "Rebuild never pulls anything forward"
        );
    }
}

// --- position staleness after missing-mod reinsertion -----------------------

#[test]
fn placement_position_matches_the_final_order_after_missing_mod_reinsertion() {
    let builder = ReportBuilder::new().mod_("a").mod_("b").mod_("c");
    let mut report = builder.build();
    report.missing_mods = vec![ModId::new("gone")];
    let current = LoadOrder::new(vec![
        ModId::new("a"),
        ModId::new("gone"),
        ModId::new("b"),
        ModId::new("c"),
    ]);
    let mut overrides = SorterOverrides::default();
    overrides.kept_missing_mods.insert(ModId::new("gone"));

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &overrides,
        &current,
    ));

    for (id, explanation) in &outcome.placements {
        assert_eq!(
            outcome.order.position(id),
            Some(explanation.position),
            "placements[{id}].position must match the final order, not the pre-reinsertion one"
        );
    }
}

// --- end-to-end override coverage -------------------------------------------

#[test]
fn drop_edge_override_removes_the_edge_from_the_graph_entirely() {
    let builder = ReportBuilder::new().mod_("a").mod_("b").hard_edge("a", "b"); // a after b
    let current = builder.insertion_order(); // [a, b]: violates "a after b"
    let report = builder.build();
    let mut overrides = SorterOverrides::default();
    overrides
        .dropped_edges
        .insert((ModId::new("a"), ModId::new("b"), EdgeKind::AssemblyRef));

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &overrides,
        &current,
    ));

    assert_eq!(
        outcome.order.as_slice(),
        current.as_slice(),
        "with the edge dropped before it ever reaches the graph, nothing forces a after b"
    );
    assert!(
        outcome.dropped.is_empty(),
        "a DropEdge override removes the edge outright; it was never a cycle-break casualty"
    );
}

#[test]
fn choose_candidate_override_forces_the_named_candidate_over_the_default_pick() {
    // Without an override, the any-of default pick is the candidate
    // already before `after` in the current order (`candidate.a`, at
    // position 0). `ChooseCandidate` must override that default.
    let builder = ReportBuilder::new()
        .mod_("candidate.a")
        .mod_("candidate.b")
        .mod_("dependent")
        .any_of(
            "dependent",
            "shared.dll",
            &["candidate.a", "candidate.b"],
            true,
        );
    let current = builder.insertion_order();
    let report = builder.build();
    let mut overrides = SorterOverrides::default();
    overrides.chosen_candidates.insert(
        (ModId::new("dependent"), "shared.dll".to_string()),
        ModId::new("candidate.b"),
    );

    let outcome = sort(&base_input(
        &report,
        &RuleSet::default(),
        &Tagging::default(),
        &overrides,
        &current,
    ));

    assert_eq!(outcome.any_of_choices.len(), 1);
    assert_eq!(
        outcome.any_of_choices[0].chosen,
        Some(ModId::new("candidate.b")),
        "the override must win over the default already-before-today pick"
    );
    assert!(
        outcome.order.position(&ModId::new("candidate.b")).unwrap()
            < outcome.order.position(&ModId::new("dependent")).unwrap(),
        "the forced candidate must actually load before the dependent"
    );
}
