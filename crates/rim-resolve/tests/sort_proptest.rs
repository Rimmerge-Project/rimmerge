//! Property tests for `rim_resolve::sort`'s core invariants: the output is
//! always a permutation of the input, every accepted edge is satisfied,
//! and `sort` is deterministic and invariant under input-edge shuffling.

use std::collections::BTreeSet;

use proptest::prelude::*;
use rim_analyzer::domain::{EdgeKind, EdgeStatus, ModId};
use rim_resolve::domain::{
    Placement, PlacementRule, Rule, RuleOrigin, RuleSet, SorterOverrides, Tagging,
};
use rim_resolve::evaluate;
use rim_resolve::sort::{SortInput, sort};
use rim_resolve::test_support::ReportBuilder;

fn mod_name(i: usize) -> String {
    format!("mod{i:02}")
}

/// One of the five kinds
/// `arb_case`'s own edges are drawn from — `LoadAfter` (`Declared`),
/// `ModDependency` (also `Declared`, the mixed-kind `kind_rank` shape when
/// it lands opposite a `LoadAfter` between the same pair), and three
/// `Inferred` kinds: `RetextureAfterOwner` (the plain case),
/// `PatchRemovedNodeCosmetic` (the cosmetic edge `cycles::break_cycles`
/// drops first within a tied `Inferred` cycle) and
/// `ReplaceDiscardsAddition` (a second, distinct `Inferred` kind, so a
/// pair joined by two different `Inferred` edges is exercised too, not
/// only a `Declared`/`Inferred` mix) — deliberately never `Hard`:
/// `no_hard_edge_is_dropped_when_hard_edges_alone_are_acyclic` already
/// owns dedicated `Hard`-only coverage via `build_hard_report` below, and
/// mixing a `Hard` edge into these otherwise-`Declared`/`Inferred` cycles
/// would change what that test is isolating.
fn arb_kind() -> impl Strategy<Value = EdgeKind> {
    prop_oneof![
        Just(EdgeKind::LoadAfter),
        Just(EdgeKind::ModDependency),
        Just(EdgeKind::RetextureAfterOwner),
        // `PatchRemovedNodeCosmetic`/`ReplaceDiscardsAddition`: both
        // `Inferred` too, so a run over `arb_kind` still roughly balances
        // `Declared` (2 kinds) against `Inferred` (3 kinds) rather than
        // skewing further toward `Declared` now that `Inferred` also
        // propagates — and exercises the "parallel edges between one
        // pair, mixed layers" and cosmetic-drop shapes these two kinds are
        // for.
        Just(EdgeKind::PatchRemovedNodeCosmetic),
        Just(EdgeKind::ReplaceDiscardsAddition),
    ]
}

/// One slot's own generated edge: `None` (not kept) or `Some(kind)` (kept,
/// with this `EdgeKind`) — bundling the keep/drop coin flip with the kind
/// draw in one `Strategy` output is what lets a later `filter_map` recover
/// both together after shrinking, instead of two separately-shrunk vectors
/// that could desync.
fn arb_slot() -> impl Strategy<Value = Option<EdgeKind>> {
    // 1-in-4 chance of `None`, matching the old `any::<bool>()` mask's own
    // 50/50 keep rate closely enough (3 kept kinds vs. 1 dropped slot) —
    // not load-bearing precision, just keeping graph density in the same
    // rough range the existing `n` bounds (3..=7 mods) were tuned against.
    prop_oneof![
        1 => Just(None),
        3 => arb_kind().prop_map(Some),
    ]
}

/// A `(before_index, after_index, kind)` edge list.
type IndexEdges = Vec<(usize, usize, EdgeKind)>;

/// One mod's own explicit [`Placement`], or none — `Top`/`Bottom`
/// pins.
///
/// Without pins, every proptest here would pass `RuleSet::default()`, so
/// `sort::tiers` would never assign `Tier::Top`/`Tier::Bottom` by
/// placement, and `sort::emit::apply_placement_extremes` would return its
/// argument untouched every single time — the whole closure block move
/// would be unreachable from this file. Pins are rare on purpose (6-in-8 `None`)
/// so the ordinary unpinned shapes these properties were written for stay
/// the common case, while a run of 64 cases over 3..=7 mods still reaches
/// pinned regions, both-sides-pinned regions, and pins with dependents
/// often.
fn arb_placement() -> impl Strategy<Value = Option<Placement>> {
    prop_oneof![
        6 => Just(None),
        1 => Just(Some(Placement::Top)),
        1 => Just(Some(Placement::Bottom)),
    ]
}

/// The `RuleSet` `placements` describes: one `Rule::Placement` per pinned
/// mod, `RuleOrigin::UserDecision` throughout (origin only affects which
/// finding a *contradiction* produces, never the sorter's own placement
/// handling, so one origin is enough here).
fn rules_of(placements: &[Option<Placement>]) -> RuleSet {
    RuleSet::new(
        placements
            .iter()
            .enumerate()
            .filter_map(|(index, placement)| {
                placement.map(|placement| {
                    Rule::Placement(PlacementRule {
                        mod_id: ModId::new(mod_name(index)),
                        placement,
                        origin: RuleOrigin::UserDecision,
                        comment: None,
                    })
                })
            })
            .collect(),
    )
}

/// A random small graph: `n` mods, a guaranteed-acyclic "forward" edge set
/// built along the hidden order `0..n`, plus an unrestricted "extra" edge
/// set (any direction) that may close cycles with the forward set. Each
/// kept edge's own `EdgeKind` is drawn from [`arb_kind`] independently —
/// two edges can occupy the exact same `(before, after)` index pair with
/// different kinds (one from `dag_edges`, one from `extra_edges`, or one
/// of each drawn for the same slot in `all_pairs` since it's a superset
/// of `forward_pairs`), which is exactly the "`ModDependency` and
/// `LoadAfter` between the same two mods" shape — `build_report`'s own
/// dedup below keys on the full `(after, before, kind)` triple, not just
/// the pair, so such a duplicate-pair-different-kind edge is kept as two
/// real edges rather than collapsed into one.
fn arb_case() -> impl Strategy<Value = (usize, IndexEdges, IndexEdges, Vec<Option<Placement>>)> {
    (3usize..=7).prop_flat_map(|n| {
        let forward_pairs: Vec<(usize, usize)> = (0..n)
            .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
            .collect();
        let all_pairs: Vec<(usize, usize)> = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .collect();
        let forward_len = forward_pairs.len();
        let all_len = all_pairs.len();
        (
            proptest::collection::vec(arb_slot(), forward_len),
            proptest::collection::vec(arb_slot(), all_len),
            proptest::collection::vec(arb_placement(), n),
        )
            .prop_map(move |(forward_slots, extra_slots, placements)| {
                let dag_edges: IndexEdges = forward_pairs
                    .iter()
                    .zip(forward_slots)
                    .filter_map(|(p, slot)| slot.map(|kind| (p.0, p.1, kind)))
                    .collect();
                let extra_edges: IndexEdges = all_pairs
                    .iter()
                    .zip(extra_slots)
                    .filter_map(|(p, slot)| slot.map(|kind| (p.0, p.1, kind)))
                    .collect();
                (n, dag_edges, extra_edges, placements)
            })
    })
}

/// [`arb_case`]'s own tuple, plus a shuffled permutation of `0..n` used
/// as `current`'s own mod order. Every [`arb_case`] fixture uses
/// insertion order for `current` (`dag_edges` are all forward pairs), so
/// a violated pair only ever comes from `extra_edges`; shuffling
/// `current` independently is what lets a `dag_edges` pair — which
/// `sort::direction`'s own cost comparison never otherwise sees as
/// violated — land on the wrong side of the base order too.
fn arb_case_with_current() -> impl Strategy<
    Value = (
        usize,
        IndexEdges,
        IndexEdges,
        Vec<Option<Placement>>,
        Vec<usize>,
    ),
> {
    arb_case().prop_flat_map(|(n, dag_edges, extra_edges, placements)| {
        (
            Just(n),
            Just(dag_edges),
            Just(extra_edges),
            Just(placements),
            Just((0..n).collect::<Vec<usize>>()).prop_shuffle(),
        )
    })
}

/// The [`rim_analyzer::domain::LoadOrder`] [`arb_case_with_current`]'s own
/// shuffled index list names.
fn shuffled_current(shuffled_indices: &[usize]) -> rim_analyzer::domain::LoadOrder {
    rim_analyzer::domain::LoadOrder::new(
        shuffled_indices
            .iter()
            .map(|&index| ModId::new(mod_name(index)))
            .collect(),
    )
}

/// Builds a report from `n` mods and a `(before_index, after_index, kind)`
/// edge list (each triple meaning `after` must load after `before`, via
/// `kind`), returning it alongside the exact `(after, before, kind)`
/// triples added — deduplicated, so an identical triple appearing in both
/// `dag_edges` and `extra_edges` is only checked once; a duplicate
/// `(after, before)` *pair* with a *different* kind is kept as two
/// distinct edges (see [`arb_case`]'s own doc comment).
fn build_report(
    n: usize,
    dag_edges: &[(usize, usize, EdgeKind)],
    extra_edges: &[(usize, usize, EdgeKind)],
) -> (
    rim_analyzer::domain::Report,
    BTreeSet<(ModId, ModId, EdgeKind)>,
) {
    let mut builder = ReportBuilder::new();
    for i in 0..n {
        builder = builder.mod_(&mod_name(i));
    }
    let mut pairs: BTreeSet<(ModId, ModId, EdgeKind)> = BTreeSet::new();
    for &(before, after, kind) in dag_edges.iter().chain(extra_edges) {
        pairs.insert((
            ModId::new(mod_name(after)),
            ModId::new(mod_name(before)),
            kind,
        ));
    }
    for (after, before, kind) in &pairs {
        // `load_time: true` for every kind here: `RetextureAfterOwner`/
        // `ModDependency` ignore it entirely (`Edge::strength` only ever
        // refines `AssemblyRef`, never generated here — see `arb_kind`'s
        // own doc comment on why `Hard` kinds are excluded), and
        // `LoadAfter`'s own strength is `Declared` regardless.
        builder = builder.edge(after.as_str(), before.as_str(), *kind, true);
    }
    (builder.build(), pairs)
}

/// A plain `(before_index, after_index)` edge list, no kind — only
/// [`arb_acyclic_hard_case`]/[`build_hard_report`] use this shape: every
/// edge there is `AssemblyRef` unconditionally (see [`arb_kind`]'s own
/// doc comment for why the mixed-kind `Declared`/`Inferred` diversification
/// [`IndexEdges`] carries deliberately excludes `Hard`).
type PlainIndexEdges = Vec<(usize, usize)>;

/// A guaranteed-acyclic set of `Hard`-strength edges only (no `extra`
/// backward edges) — since `Hard` is the very first layer `sort` adds, the
/// graph at that point contains nothing but `Hard`-layer edges, so no
/// cycle can exist unless these edges alone already form one. With none
/// here, `sort` must never need to drop any of them.
///
/// **Deliberately gets no [`arb_placement`] arm**, unlike
/// [`arb_case`]: tier membership sentinels bind at `Layer::Hard` too
/// (`sort::tiers::membership_layer`), so a placement pin plus a `Hard`
/// edge pointing the other way is a genuine `Hard`-layer cycle — which
/// would make a dropped `Hard` edge *correct* and this property's whole
/// premise ("the graph at that point contains nothing but these edges")
/// false. Pinning stays in [`arb_case`], where dropped edges are excluded
/// from the assertion rather than forbidden.
fn arb_acyclic_hard_case() -> impl Strategy<Value = (usize, PlainIndexEdges)> {
    (3usize..=7).prop_flat_map(|n| {
        let forward_pairs: Vec<(usize, usize)> = (0..n)
            .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
            .collect();
        let len = forward_pairs.len();
        (Just(n), proptest::collection::vec(any::<bool>(), len)).prop_map(move |(n, mask)| {
            let edges: PlainIndexEdges = forward_pairs
                .iter()
                .zip(mask)
                .filter(|(_, keep)| *keep)
                .map(|(p, _)| *p)
                .collect();
            (n, edges)
        })
    })
}

fn build_hard_report(n: usize, edges: &[(usize, usize)]) -> rim_analyzer::domain::Report {
    let mut builder = ReportBuilder::new();
    for i in 0..n {
        builder = builder.mod_(&mod_name(i));
    }
    for &(before, after) in edges {
        builder = builder.hard_edge(&mod_name(after), &mod_name(before));
    }
    builder.build()
}

fn run_sort(
    report: &rim_analyzer::domain::Report,
    rules: &RuleSet,
    current: &rim_analyzer::domain::LoadOrder,
    tie_break: rim_resolve::sort::TieBreak,
) -> rim_resolve::sort::SortOutcome {
    sort(&SortInput {
        report,
        rules,
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current,
        enforce: rim_resolve::sort::EnforcedLayers::default(),
        tie_break,
    })
}

/// Both [`rim_resolve::sort::TieBreak`] modes, so determinism and
/// permutation invariance are checked in both.
const BOTH_TIE_BREAKS: [rim_resolve::sort::TieBreak; 2] = [
    rim_resolve::sort::TieBreak::PreserveCurrent,
    rim_resolve::sort::TieBreak::Rebuild,
];

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn output_is_always_a_permutation_of_the_input((n, dag, extra, placements) in arb_case()) {
        let (report, _pairs) = build_report(n, &dag, &extra);
        let rules = rules_of(&placements);
        let current = rim_analyzer::domain::LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect());

        for tie_break in BOTH_TIE_BREAKS {
            let outcome = run_sort(&report, &rules, &current, tie_break);

            let expected: BTreeSet<ModId> = report.mods.iter().map(|m| m.id.clone()).collect();
            let actual: BTreeSet<ModId> = outcome.order.as_slice().iter().cloned().collect();
            prop_assert_eq!(actual, expected, "{:?}", tie_break);
            prop_assert_eq!(outcome.order.as_slice().len(), report.mods.len(), "{:?}", tie_break);
        }
    }

    #[test]
    fn every_accepted_edge_is_satisfied((n, dag, extra, placements) in arb_case()) {
        let (report, pairs) = build_report(n, &dag, &extra);
        let rules = rules_of(&placements);
        let current = rim_analyzer::domain::LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect());

        for tie_break in BOTH_TIE_BREAKS {
            let outcome = run_sort(&report, &rules, &current, tie_break);

            // Keyed by the full `(after, before, kind)` triple, not just
            // the pair: two edges can share a pair with different kinds
            // (`arb_case`'s own mixed-kind shape), and only one of them might be
            // the one a cycle actually drops — the other, still-accepted
            // kind must still be checked below, not skipped just because
            // its sibling kind was dropped.
            let dropped: BTreeSet<(ModId, ModId, EdgeKind)> = outcome
                .dropped
                .iter()
                .filter_map(|d| match &d.edge.provenance {
                    rim_resolve::sort::EdgeProvenance::Engine { kind, .. } => {
                        Some((d.edge.after.clone(), d.edge.before.clone(), *kind))
                    }
                    _ => None,
                })
                .collect();

            for (after, before, kind) in &pairs {
                if dropped.contains(&(after.clone(), before.clone(), *kind)) {
                    continue;
                }
                prop_assert_eq!(evaluate::ordering_status(after, before, &outcome.order),
                    EdgeStatus::Satisfied,
                    "accepted edge {} after {} ({:?}) must hold in the final order ({:?})",
                    after,
                    before,
                    kind,
                    tie_break
                );
            }
        }
    }

    #[test]
    fn sort_is_deterministic((n, dag, extra, placements) in arb_case()) {
        let (report, _pairs) = build_report(n, &dag, &extra);
        let rules = rules_of(&placements);
        let current = rim_analyzer::domain::LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect());

        for tie_break in BOTH_TIE_BREAKS {
            let first = run_sort(&report, &rules, &current, tie_break);
            let second = run_sort(&report, &rules, &current, tie_break);

            // The whole outcome, not just the emitted order: two runs over
            // the identical input must agree on every field `SortOutcome`'s
            // own `PartialEq` compares (placements, dropped edges, any-of
            // choices, warnings, disturbance stats) — a narrower comparison
            // could hide non-determinism in, say, *which* edge a tied
            // cycle-break drops even when the final order happens to come
            // out the same either way.
            prop_assert_eq!(first, second, "{:?}", tie_break);
        }
    }

    #[test]
    fn no_hard_edge_is_dropped_when_hard_edges_alone_are_acyclic((n, edges) in arb_acyclic_hard_case()) {
        let report = build_hard_report(n, &edges);
        let current = rim_analyzer::domain::LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect());

        for tie_break in BOTH_TIE_BREAKS {
            let outcome = run_sort(&report, &RuleSet::default(), &current, tie_break);

            let hard_drops: Vec<_> = outcome
                .dropped
                .iter()
                .filter(|d| matches!(&d.edge.provenance,
                    rim_resolve::sort::EdgeProvenance::Engine { kind, .. }
                        if kind.strength() == rim_analyzer::domain::EdgeStrength::Hard
                ))
                .collect();
            prop_assert!(hard_drops.is_empty(),
                "no Hard edge may be dropped when Hard edges alone form no cycle ({tie_break:?}): {hard_drops:?}"
            );
        }
    }

    #[test]
    fn sort_is_invariant_under_input_edge_shuffling((n, dag, extra, placements) in arb_case()) {
        let (report, _pairs) = build_report(n, &dag, &extra);
        let rules = rules_of(&placements);
        let current = rim_analyzer::domain::LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect());
        let mut shuffled = report.clone();
        shuffled.edges.reverse();

        for tie_break in BOTH_TIE_BREAKS {
            let original = run_sort(&report, &rules, &current, tie_break);
            let reversed = run_sort(&shuffled, &rules, &current, tie_break);

            // Whole-outcome comparison — see `sort_is_deterministic`'s doc
            // comment for why a narrower one (just the emitted order) isn't
            // enough to catch a tie-break that depends on input edge order.
            prop_assert_eq!(original, reversed, "{:?}", tie_break);
        }
    }
}

// --- shuffled `current`: exercises a violated `dag_edges` pair too ---------
//
// Every property above builds `current` from insertion order, so only
// `extra_edges` (backward pairs) can ever be violated. `sort::direction`'s
// own cost comparison is decided per violated pair regardless of which
// edge list a pair came from, so these sibling properties (rather than
// widening the ones above, which would make a shrink hide which input
// shape failed) shuffle `current` too, reaching a violated `dag_edges`
// pair as well.

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn output_is_always_a_permutation_of_the_input_with_shuffled_current(
        (n, dag, extra, placements, shuffled) in arb_case_with_current()
    ) {
        let (report, _pairs) = build_report(n, &dag, &extra);
        let rules = rules_of(&placements);
        let current = shuffled_current(&shuffled);

        for tie_break in BOTH_TIE_BREAKS {
            let outcome = run_sort(&report, &rules, &current, tie_break);

            let expected: BTreeSet<ModId> = report.mods.iter().map(|m| m.id.clone()).collect();
            let actual: BTreeSet<ModId> = outcome.order.as_slice().iter().cloned().collect();
            prop_assert_eq!(actual, expected, "{:?}", tie_break);
            prop_assert_eq!(outcome.order.as_slice().len(), report.mods.len(), "{:?}", tie_break);
        }
    }

    #[test]
    fn every_accepted_edge_is_satisfied_with_shuffled_current(
        (n, dag, extra, placements, shuffled) in arb_case_with_current()
    ) {
        let (report, pairs) = build_report(n, &dag, &extra);
        let rules = rules_of(&placements);
        let current = shuffled_current(&shuffled);

        for tie_break in BOTH_TIE_BREAKS {
            let outcome = run_sort(&report, &rules, &current, tie_break);

            let dropped: BTreeSet<(ModId, ModId, EdgeKind)> = outcome
                .dropped
                .iter()
                .filter_map(|d| match &d.edge.provenance {
                    rim_resolve::sort::EdgeProvenance::Engine { kind, .. } => {
                        Some((d.edge.after.clone(), d.edge.before.clone(), *kind))
                    }
                    _ => None,
                })
                .collect();

            for (after, before, kind) in &pairs {
                if dropped.contains(&(after.clone(), before.clone(), *kind)) {
                    continue;
                }
                prop_assert_eq!(evaluate::ordering_status(after, before, &outcome.order),
                    EdgeStatus::Satisfied,
                    "accepted edge {} after {} ({:?}) must hold in the final order ({:?})",
                    after,
                    before,
                    kind,
                    tie_break
                );
            }
        }
    }

    #[test]
    fn sort_is_deterministic_with_shuffled_current(
        (n, dag, extra, placements, shuffled) in arb_case_with_current()
    ) {
        let (report, _pairs) = build_report(n, &dag, &extra);
        let rules = rules_of(&placements);
        let current = shuffled_current(&shuffled);

        for tie_break in BOTH_TIE_BREAKS {
            let first = run_sort(&report, &rules, &current, tie_break);
            let second = run_sort(&report, &rules, &current, tie_break);
            prop_assert_eq!(first, second, "{:?}", tie_break);
        }
    }

    #[test]
    fn sort_is_invariant_under_input_edge_shuffling_with_shuffled_current(
        (n, dag, extra, placements, shuffled) in arb_case_with_current()
    ) {
        let (report, _pairs) = build_report(n, &dag, &extra);
        let rules = rules_of(&placements);
        let current = shuffled_current(&shuffled);
        let mut edges_reversed = report.clone();
        edges_reversed.edges.reverse();

        for tie_break in BOTH_TIE_BREAKS {
            let original = run_sort(&report, &rules, &current, tie_break);
            let reversed = run_sort(&edges_reversed, &rules, &current, tie_break);
            prop_assert_eq!(original, reversed, "{:?}", tie_break);
        }
    }
}

// --- fixed point: nothing to satisfy, nothing should move -----------------

/// A guaranteed-drop-free case for
/// `preserve_current_is_a_fixed_point_on_its_own_output_when_nothing_is_dropped`:
/// `dag_edges` only, each pair already index-ordered (`i < j`), so the
/// graph can never contain a cycle, and no placements (a `Top`/`Bottom`
/// pin's own tier sentinel could otherwise contradict a `dag_edges` pair
/// and force a drop). `dropped.is_empty()` holds by construction, so the
/// property below asserts it directly instead of filtering on it with
/// `prop_assume!` — over `arb_case()`'s own mix of forward and backward
/// edges, almost every case has *some* drop, which made an
/// `prop_assume!`-filtered version of this property reject far more than
/// `ProptestConfig::default().max_global_rejects` (1024) allows and abort
/// with "too many global rejects" before ever reaching the fixed-point
/// check itself — a sampling problem, not a sorter one, avoided here by
/// construction rather than by raising the reject budget.
fn arb_drop_free_case() -> impl Strategy<Value = (usize, IndexEdges)> {
    (3usize..=7).prop_flat_map(|n| {
        let forward_pairs: Vec<(usize, usize)> = (0..n)
            .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
            .collect();
        let len = forward_pairs.len();
        proptest::collection::vec(arb_slot(), len).prop_map(move |slots| {
            let edges: IndexEdges = forward_pairs
                .iter()
                .zip(slots)
                .filter_map(|(pair, slot)| slot.map(|kind| (pair.0, pair.1, kind)))
                .collect();
            (n, edges)
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// If `PreserveCurrent` dropped nothing, every accepted edge already
    /// holds in `outcome.order` — no pair is violated, so every
    /// `sort::direction` decision is the satisfied-in-base-order "chain
    /// continuation" case, a no-op. Sorting `outcome.order` again as the
    /// new `current` must therefore reproduce it exactly. This catches
    /// any cost logic that moves a mod even when nothing is violated.
    #[test]
    fn preserve_current_is_a_fixed_point_on_its_own_output_when_nothing_is_dropped(
        (n, dag) in arb_drop_free_case()
    ) {
        let (report, _pairs) = build_report(n, &dag, &[]);
        let rules = RuleSet::default();
        let current = rim_analyzer::domain::LoadOrder::new(
            report.mods.iter().map(|m| m.id.clone()).collect()
        );

        let outcome = run_sort(
            &report,
            &rules,
            &current,
            rim_resolve::sort::TieBreak::PreserveCurrent,
        );
        prop_assert!(
            outcome.dropped.is_empty(),
            "an acyclic, placement-free case must never drop an edge"
        );

        let second = run_sort(
            &report,
            &rules,
            &outcome.order,
            rim_resolve::sort::TieBreak::PreserveCurrent,
        );

        prop_assert_eq!(second.order.as_slice(), outcome.order.as_slice());
    }
}
