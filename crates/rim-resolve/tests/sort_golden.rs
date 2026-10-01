//! Golden tests against analyzer data.
//!
//! `tests/golden/report.synthetic.json` is committed and drives fast
//! `insta` snapshots plus explicit invariant assertions on every run. It
//! is produced entirely by the real analyzer pipeline from a synthetic,
//! invented install
//! (`crates/rim-resolve/tests/fixtures/synthetic-install.json`), never
//! from real mod data, so it is safe to publish and never drifts with
//! real-install churn. The `#[ignore]`d full-size test below still reads
//! the real, un-trimmed `.journal/local/report.json` (git-ignored) to check the
//! same invariants at real scale — that one intentionally keeps real
//! data and real mod ids, since it is never committed.
//!
//! # regenerate
//!
//! Run from the workspace root (PowerShell). The scratch directory must
//! be the **relative** path `synthetic-install` at the workspace root,
//! never `$env:TEMP` -- `$env:TEMP` on Windows always embeds the
//! machine's username (`C:\Users\<name>\AppData\Local\Temp`), which the
//! generated `Report.metadata.game_dir`/`workshop_dir`/`mods_config`
//! fields would then carry verbatim into the committed golden, failing
//! the forbidden-token gate on every machine but the one that generated
//! it. Those fields embed `$S` verbatim, so a neutral relative name
//! keeps the committed file free of any maintainer-local path. The
//! procedure:
//!
//! ```powershell
//! $S = "synthetic-install"
//! cargo run -p rimmerge-cli --release -- fixture gen `
//!   --spec crates/rim-resolve/tests/fixtures/synthetic-install.json --out $S
//! cargo run -p rim-analyzer --release -- analyze `
//!   --game-dir "$S/game" --workshop-dir "$S/workshop/content/294100" `
//!   --mods-config "$S/game/ModsConfig.xml" --json "$S/report.json"
//! cargo run -p rimmerge-cli --release -- fixture trim `
//!   --in "$S/report.json" --out crates/rim-resolve/tests/golden/report.synthetic.json
//! rg -i -f .github/forbidden-tokens.txt crates/rim-resolve/tests/golden/report.synthetic.json
//! # must print nothing (ripgrep exits 1 on no match)
//! pwsh ./scripts/check-forbidden.ps1
//! # the full, whole-tree gate (public + local lists) -- a quick single-
//! # file rg check above is not a substitute for it
//! cargo nextest run -p rim-resolve --test sort_golden      # writes .snap.new
//! Remove-Item -Recurse -Force $S                           # the scratch install
//! ```
//!
//! Then **read** each `tests/snapshots/*.snap.new` by hand (no
//! `cargo-insta` in this environment) and rename it over the matching
//! `.snap` file to accept — never accept blindly; see
//! `crates/rim-resolve/CLAUDE.md`'s own note on this. The three
//! snapshots below are baselined this way against the synthetic golden's
//! own content, which is stable (deterministic, seeded generation) and
//! needs re-baselining only when the spec, the generator, or the sorter
//! itself changes.
//!
//! # Edge kinds this golden does not exercise
//!
//! The generator plants `is_framework_candidate`,
//! `AssemblyVersionPrecedence`, `ParentTemplate`, `RetextureAfterOwner`,
//! `ForceLoadAfter` and `ForceLoadBefore`. Four `EdgeKind` variants are
//! zero in this golden, disclosed rather than silently absent: `UsesType`
//! (needs a def in one mod naming a type another mod's *assembly* --
//! not merely a patch xpath -- owns; every synthetic assembly reference
//! here is exercised through `AssemblyRef`/`ExtendsHard`/`ExtendsSolo`
//! instead), `DefOverrideAfterOrigin` (its two origin rules --
//! `ParentName` ownership or a shared `defName` family prefix -- neither
//! of which this generator's own `plant_def_overrides` constructs
//! trigger; its two owners share a plain, unrelated defName with no
//! template lineage), and the `PatchInjectedNode`/`PatchRemovedNode`/
//! `PatchSelectsInjectedNode` trio (each needs a patch that actually
//! injects or removes a *structural* node another mod's patch then
//! selects -- a materially different, more elaborate construct than
//! anything `plant_patch_only_zoo` currently builds). Planting them is
//! open work, not an oversight.
//!
//! There is no default rule set, so every sort below runs against an
//! empty [`RuleSet`]/[`Tagging`] unless a test builds its own.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

use rim_analyzer::domain::{
    Conflict, EdgeStatus, LoadOrder, ModId, PatchCollisionSeverity, Report,
};
use rim_resolve::domain::{
    Finding, FindingKey, Placement, PlacementRule, Rule, RuleOrigin, RuleSet, SorterOverrides,
    Tagging,
};
use rim_resolve::evaluate;
use rim_resolve::sort::{EnforcedLayers, SortInput, SortOutcome, TieBreak, Tier, TierReason, sort};

fn load_report(path: &PathBuf) -> Option<Report> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(report) => Some(report),
        Err(error) => panic!("failed to parse {}: {error}", path.display()),
    }
}

/// The default confidence threshold — matches the one used for every
/// ledger built in this file.
fn default_threshold() -> rim_resolve::domain::Confidence {
    rim_resolve::domain::Confidence::new(80).unwrap_or_else(|_| unreachable!())
}

fn sort_report(
    report: &Report,
    enforce: EnforcedLayers,
    tie_break: TieBreak,
) -> (SortOutcome, LoadOrder) {
    let current = LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect());
    let outcome = sort(&SortInput {
        report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce,
        tie_break,
    });
    (outcome, current)
}

/// A short, stable label for a [`FindingKey`]'s variant, for grouping the
/// inbox breakdown by finding kind.
fn finding_kind_name(key: &FindingKey) -> &'static str {
    match key {
        FindingKey::EdgeDropped { .. } => "EdgeDropped",
        FindingKey::AnyOfChoice { .. } => "AnyOfChoice",
        FindingKey::DefOverride { .. } => "DefOverride",
        FindingKey::PatchCollision { .. } => "PatchCollision",
        FindingKey::TextureOverride { .. } => "TextureOverride",
        FindingKey::DuplicateAssembly { .. } => "DuplicateAssembly",
        FindingKey::LikelyDuplicateMod { .. } => "LikelyDuplicateMod",
        FindingKey::MissingMod { .. } => "MissingMod",
        FindingKey::MissingDependency { .. } => "MissingDependency",
        FindingKey::IncompatiblePair { .. } => "IncompatiblePair",
        FindingKey::UnsupportedVersion { .. } => "UnsupportedVersion",
        FindingKey::UndeclaredHardDependency { .. } => "UndeclaredHardDependency",
        FindingKey::TagInferred { .. } => "TagInferred",
        FindingKey::LazyReferenceViolated { .. } => "LazyReferenceViolated",
        FindingKey::DeclarationQuestioned { .. } => "DeclarationQuestioned",
        FindingKey::DeclarationOverridden { .. } => "DeclarationOverridden",
        FindingKey::DuplicateTemplateName { .. } => "DuplicateTemplateName",
        FindingKey::KeyedTranslationCollision { .. } => "KeyedTranslationCollision",
        FindingKey::SoundOverride { .. } => "SoundOverride",
        FindingKey::UndeclaredTypeDependency { .. } => "UndeclaredTypeDependency",
        FindingKey::RuntimePatchCollision { .. } => "RuntimePatchCollision",
        FindingKey::TranspilerCollision { .. } => "TranspilerCollision",
        FindingKey::RuleOverruled { .. } => "RuleOverruled",
        FindingKey::PlacementOverruled { .. } => "PlacementOverruled",
        FindingKey::PlacementQuestioned { .. } => "PlacementQuestioned",
        FindingKey::PlacementOrderingOverridden { .. } => "PlacementOrderingOverridden",
        FindingKey::PlacementPromotesDependents { .. } => "PlacementPromotesDependents",
        FindingKey::MissingTexturePath { .. } => "MissingTexturePath",
        FindingKey::PatchWillFail { .. } => "PatchWillFail",
        FindingKey::ContributesNothing { .. } => "ContributesNothing",
        FindingKey::UndecodableTexture { .. } => "UndecodableTexture",
        FindingKey::BrokenInheritance { .. } => "BrokenInheritance",
        FindingKey::NearMissModReference { .. } => "NearMissModReference",
        FindingKey::DiscardedAddition { .. } => "DiscardedAddition",
        FindingKey::DanglingDefReference { .. } => "DanglingDefReference",
    }
}

fn print_needs_input_breakdown(label: &str, ledger: &rim_resolve::domain::Ledger) {
    let mut by_kind: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    for entry in &ledger.entries {
        if entry.status == rim_resolve::domain::ResolutionStatus::NeedsInput {
            *by_kind.entry(finding_kind_name(&entry.key)).or_insert(0) += 1;
        }
    }
    eprintln!(
        "needs_input breakdown [{label}] (total {}): {by_kind:?}",
        ledger.stats.needs_input
    );
}

/// Invariants any [`SortOutcome`] must satisfy over `report`, regardless
/// of scale. `enforce` must match whatever [`EnforcedLayers`] the outcome
/// was actually sorted with: a `Soft`/`Awareness` edge that isn't enforced
/// is advisory-only (never added to the graph, so never "accepted" in the
/// sense this checks) and is allowed to be violated.
fn assert_invariants(report: &Report, outcome: &SortOutcome, enforce: EnforcedLayers) {
    let expected: BTreeSet<ModId> = report.mods.iter().map(|m| m.id.clone()).collect();
    let actual: BTreeSet<ModId> = outcome.order.as_slice().iter().cloned().collect();
    assert_eq!(
        actual, expected,
        "the emitted order must be a permutation of every active mod"
    );
    assert_eq!(
        outcome.order.as_slice().len(),
        report.mods.len(),
        "no mod may be emitted twice"
    );

    let dropped: BTreeSet<(ModId, ModId)> = outcome
        .dropped
        .iter()
        .map(|d| (d.edge.after.clone(), d.edge.before.clone()))
        .collect();
    for edge_report in &report.edges {
        let edge = &edge_report.edge;
        let is_advisory_only = match edge.strength() {
            rim_analyzer::domain::EdgeStrength::Soft => !enforce.soft,
            rim_analyzer::domain::EdgeStrength::Awareness => !enforce.awareness,
            // `Layer::Inferred` is gated on `enforce.inferred` exactly like
            // `Soft`/`Awareness` above.
            rim_analyzer::domain::EdgeStrength::Inferred => !enforce.inferred,
            rim_analyzer::domain::EdgeStrength::Hard
            | rim_analyzer::domain::EdgeStrength::Declared => false,
        };
        if is_advisory_only || dropped.contains(&(edge.after.clone(), edge.before.clone())) {
            continue;
        }
        assert_eq!(
            evaluate::edge_status(edge, &outcome.order),
            EdgeStatus::Satisfied,
            "accepted edge {} after {} ({:?}) must hold in the final order",
            edge.after,
            edge.before,
            edge.kind
        );
    }

    // No `Hard`-strength edge is dropped unless the drop was unavoidable —
    // every dropped `Hard` edge's witness cycle must be non-empty (proof a
    // real cycle forced it, not a bookkeeping mistake).
    for dropped_edge in &outcome.dropped {
        if let rim_resolve::sort::EdgeProvenance::Engine { kind, .. } =
            &dropped_edge.edge.provenance
            && kind.strength() == rim_analyzer::domain::EdgeStrength::Hard
        {
            assert!(
                !dropped_edge.witness_cycle.is_empty()
                    || dropped_edge.edge.after == dropped_edge.edge.before,
                "a dropped Hard edge must carry a witness cycle: {dropped_edge:?}"
            );
        }
    }

    // Every placement's recorded position matches its actual index.
    for (id, explanation) in &outcome.placements {
        assert_eq!(outcome.order.position(id), Some(explanation.position));
    }

    // `apply_placement_extremes`
    // may only permute mods *within* their own nominal tier's physical
    // block — never move one across a tier boundary itself, which would
    // mean tier order (Core -> Dlc -> Top -> Body -> Bottom) broke down
    // somewhere. `explanation.tier` is each mod's *nominal* tier
    // (`TierReason::PromotedBy` never rewrites it — `sort::tiers::assign_one`),
    // so a mod that legitimately got promoted ahead of its own nominal
    // tier by a stronger real edge (the golden fixture's own real
    // `example.patchlib` -> `example.earlyloader` chain) is *expected* to
    // sit outside its own nominal tier's
    // block — a naive "tiers must be monotonically non-decreasing along
    // the whole order" assert would trip on it. Excluding every
    // `PromotedBy` mod from the sequence before checking monotonicity is
    // what keeps this check honest without flagging that legitimate case.
    let physical_tier_sequence: Vec<(&ModId, Tier)> = outcome
        .order
        .as_slice()
        .iter()
        .filter_map(|id| {
            let explanation = outcome.placements.get(id)?;
            let is_promoted = matches!(explanation.tier_reason, TierReason::PromotedBy(_));
            (!is_promoted).then_some((id, explanation.tier))
        })
        .collect();
    for pair in physical_tier_sequence.windows(2) {
        let ((before_id, before_tier), (after_id, after_tier)) = (pair[0], pair[1]);
        assert!(
            before_tier <= after_tier,
            "non-promoted mods must stay within their own nominal tier's physical \
             block, in tier order (Core -> Dlc -> Top -> Body -> Bottom): \
             {before_id} ({before_tier:?}) loads before {after_id} ({after_tier:?})"
        );
    }
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/report.synthetic.json")
}

#[test]
fn golden_fixture_invariants_and_snapshot() {
    let path = golden_path();
    let Some(report) = load_report(&path) else {
        panic!(
            "{} must be committed; regenerate per this file's own \"# regenerate\" doc comment",
            path.display()
        );
    };

    // `PreserveCurrent`, not the sorter's own `Rebuild` default: the
    // committed snapshots below are generated (and reviewed by hand)
    // against current-position base keys. `Rebuild`'s own behavior
    // gets its own, non-snapshotted invariant checks right below instead
    // of a second, much-larger set of committed snapshots.
    let (outcome, _current) = sort_report(
        &report,
        EnforcedLayers::default(),
        TieBreak::PreserveCurrent,
    );

    assert_invariants(&report, &outcome, EnforcedLayers::default());

    let order_snapshot: Vec<String> = outcome
        .order
        .as_slice()
        .iter()
        .map(ToString::to_string)
        .collect();
    insta::assert_debug_snapshot!(
        "golden_sorted_order_head",
        &order_snapshot[..order_snapshot.len().min(50)]
    );

    let mut dropped_snapshot: Vec<String> = outcome
        .dropped
        .iter()
        .map(|d| {
            format!(
                "{} after {} ({:?})",
                d.edge.after, d.edge.before, d.edge.provenance
            )
        })
        .collect();
    dropped_snapshot.sort();
    insta::assert_debug_snapshot!("golden_dropped_edges", dropped_snapshot);

    insta::assert_debug_snapshot!(
        "golden_summary",
        vec![
            format!("mods: {}", report.mods.len()),
            format!("dropped: {}", outcome.dropped.len()),
            format!("any_of_choices: {}", outcome.any_of_choices.len()),
            format!("warnings: {}", outcome.warnings.len()),
            // Pin `PreserveCurrent`'s own disturbance numbers on the golden
            // fixture too, not only the full-size `.journal/local/report.json`
            // (which isn't committed) — a regression here fails a fast,
            // always-run test instead of only the `#[ignore]`d one.
            format!(
                "preserve_current_kendall_tau_inversions: {}",
                outcome.stats.kendall_tau_inversions
            ),
            format!(
                "preserve_current_positions_changed: {}",
                outcome.stats.positions_changed
            ),
        ]
    );
}

/// Every `Hard`/`Declared` edge is
/// satisfied in the output for both `TieBreak` modes on the golden report,
/// and the output stays a permutation of `report.mods` regardless of mode.
/// [`assert_invariants`] already checks both (`Hard`/`Declared` are never
/// advisory-only, so its edge-satisfaction check already covers them,
/// and its permutation check is unconditional) — this test just runs it
/// under each mode in turn.
#[test]
fn golden_fixture_satisfies_hard_and_declared_edges_under_both_tie_breaks() {
    let path = golden_path();
    let Some(report) = load_report(&path) else {
        panic!(
            "{} must be committed; regenerate per this file's own \"# regenerate\" doc comment",
            path.display()
        );
    };

    for tie_break in [TieBreak::PreserveCurrent, TieBreak::Rebuild] {
        let (outcome, _current) = sort_report(&report, EnforcedLayers::default(), tie_break);
        assert_invariants(&report, &outcome, EnforcedLayers::default());
    }
}

/// Every other golden test sorts with `RuleSet::default()`, so
/// `apply_placement_extremes` early-returns and `extremize_region` never
/// runs — a `Top` pin sorting ahead of `Core` or a `Dlc` would pass every
/// one of them undetected. Pinning a golden-fixture mod `Top` exercises
/// the pass on every `cargo nextest run`, not only the `#[ignore]`d
/// full-size tier. `synth.toppin.001` is an ordinary, otherwise
/// unremarkable synthetic mod: this test's own `RuleSet` is what makes it
/// a pin, not anything about the mod itself.
#[test]
fn top_and_bottom_placements_on_the_golden_fixture_hold_the_tier_boundaries() {
    let path = golden_path();
    let Some(report) = load_report(&path) else {
        panic!(
            "{} must be committed; regenerate per this file's own \"# regenerate\" doc comment",
            path.display()
        );
    };
    // `synth.bottompin.001` (`Placement::Bottom`) joins the
    // `synth.toppin.001` pin in the same `RuleSet` -- both pins, both
    // boundaries, one test.
    let rules = RuleSet::new(vec![
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("synth.toppin.001"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
        Rule::Placement(PlacementRule {
            mod_id: ModId::new("synth.bottompin.001"),
            placement: Placement::Bottom,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }),
    ]);

    for tie_break in [TieBreak::PreserveCurrent, TieBreak::Rebuild] {
        let current = LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect());
        let outcome = sort(&SortInput {
            report: &report,
            rules: &rules,
            tagging: &Tagging::default(),
            overrides: &SorterOverrides::default(),
            current: &current,
            enforce: EnforcedLayers::default(),
            tie_break,
        });
        assert_invariants(&report, &outcome, EnforcedLayers::default());

        let order = outcome.order.as_slice();
        let position_of = |id: &str| order.iter().position(|m| *m == ModId::new(id)).unwrap();
        let top_pos = position_of("synth.toppin.001");
        let bottom_pos = position_of("synth.bottompin.001");
        let core_pos = position_of("ludeon.rimworld");
        assert!(
            core_pos < top_pos,
            "{tie_break:?}: a Top pin must never sort ahead of Core, got position {top_pos} \
             (Core at {core_pos})"
        );
        for dlc in [
            "ludeon.rimworld.royalty",
            "ludeon.rimworld.ideology",
            "ludeon.rimworld.biotech",
            "ludeon.rimworld.anomaly",
            "ludeon.rimworld.odyssey",
        ] {
            let dlc_pos = position_of(dlc);
            assert!(
                dlc_pos < top_pos,
                "{tie_break:?}: a Top pin must never sort ahead of a DLC ({dlc}), got \
                 position {top_pos} (DLC at {dlc_pos})"
            );
        }
        assert!(
            top_pos < bottom_pos,
            "{tie_break:?}: the Top pin ({top_pos}) must sort before the Bottom pin \
             ({bottom_pos})"
        );
        // Every `Tier::Body` mod (the ordinary, unpinned tier every
        // archetype/planted mod in this fixture nominally belongs to)
        // must sort before the Bottom pin -- `Bottom` is the terminal
        // tier, so nothing else may follow it.
        for (id, explanation) in &outcome.placements {
            if explanation.tier != Tier::Body {
                continue;
            }
            let body_pos = order.iter().position(|m| m == id).unwrap_or_else(|| {
                panic!("{tie_break:?}: {id} has a placement explanation but no order position")
            });
            assert!(
                body_pos < bottom_pos,
                "{tie_break:?}: Body-tier mod {id} (position {body_pos}) must sort before the \
                 Bottom pin (position {bottom_pos})"
            );
        }
    }
}

/// The anti-vacuous-green guard for the synthetic golden itself — an
/// accidentally-degenerate regeneration (e.g. a spec edit that zeroes
/// out most of `planted`, or a generator bug that silently drops
/// content) must fail this test loudly rather than let every other
/// golden assertion above pass vacuously over a near-empty report. Floors
/// mirror `apps/cli/tests/synthetic_install_e2e.rs`'s own anti-vacuous-
/// green assertions (same rationale, this crate's own fast copy so a
/// degenerate regeneration is caught here too, not only by that
/// `apps/cli` end-to-end test).
#[test]
fn the_synthetic_golden_is_large_and_nontrivial() {
    let path = golden_path();
    let Some(report) = load_report(&path) else {
        panic!(
            "{} must be committed; regenerate per this file's own \"# regenerate\" doc comment",
            path.display()
        );
    };

    assert!(
        report.mods.len() >= 200,
        "expected >= 200 active mods, got {}",
        report.mods.len()
    );
    assert!(
        !report.constraints.is_empty(),
        "expected at least one any-of constraint (>= 2 providers) -- got none"
    );
    assert!(
        report.conflicts.len() >= 50,
        "expected >= 50 conflicts across every kind, got {}",
        report.conflicts.len()
    );

    assert_patch_collision_floor(&report);

    let (outcome, _current) = sort_report(
        &report,
        EnforcedLayers::default(),
        TieBreak::PreserveCurrent,
    );
    assert!(
        !outcome.dropped.is_empty(),
        "expected at least one dropped edge (a forced cycle) -- got none"
    );
    let resolved_any_of = outcome
        .any_of_choices
        .iter()
        .filter(|c| c.chosen.is_some())
        .count();
    eprintln!(
        "synthetic golden: {} mods, {} conflicts, {} dropped edges, {} any-of choices \
         ({resolved_any_of} resolved)",
        report.mods.len(),
        report.conflicts.len(),
        outcome.dropped.len(),
        outcome.any_of_choices.len()
    );
}

/// The generator plants 30 patch-collision pairs (`planted.patch_collisions`)
/// across four shapes. The analyzer keys an appended `<li>` by the item it
/// injects, so a planter that appends *different* items from each owner
/// silently stops colliding and the golden loses its coverage of the
/// collision classifier while every other assertion stays green. This floor
/// makes that decay fail loudly: the total, both severities, and the
/// `removed_by` path must each stay represented.
fn assert_patch_collision_floor(report: &Report) {
    let collisions: Vec<_> = report
        .conflicts
        .iter()
        .filter_map(|conflict| match conflict {
            Conflict::PatchCollision(collision) => Some(collision),
            _ => None,
        })
        .collect();
    let contested = collisions
        .iter()
        .filter(|c| c.severity == PatchCollisionSeverity::Contested)
        .count();
    let additive = collisions.len() - contested;
    let removed = collisions
        .iter()
        .filter(|c| !c.removed_by.is_empty())
        .count();
    assert!(
        collisions.len() >= 30,
        "expected >= 30 patch collisions (30 planted pairs), got {} -- the generator's planted pairs no longer collide under the analyzer's current rules",
        collisions.len()
    );
    assert!(
        contested >= 15,
        "expected >= 15 contested collisions, got {contested}"
    );
    assert!(
        additive >= 5,
        "expected >= 5 additive collisions, got {additive}"
    );
    assert!(
        removed >= 5,
        "expected >= 5 collisions with a remover, got {removed}"
    );
}

/// A cheap guard against fabricated edge evidence (such as
/// `AssemblyVersionPrecedence` evidence from treating `0.0.0.0` as a real
/// version, which drags a large closure with it): a hand-built fixture,
/// not the golden report, since the golden report's own real
/// `example.patchlib` -> `example.earlyloader` edge triggers this engine's
/// unrelated tier-promotion behavior (`TierReason::PromotedBy`) even with
/// zero violated edges — a small fixture
/// with only same-tier `Local` mods and no framework/tier-crossing edges
/// sidesteps that unrelated quirk and isolates what this test actually
/// means to check: when every enforced (`Hard`/`Declared`) edge already
/// holds, `PreserveCurrent`'s topological walk — whose ready set is
/// tie-broken by current position — must reproduce the current order
/// exactly, not just as a permutation. If a change to edge inference
/// fabricates evidence the current order doesn't actually violate, this
/// catches the reorder without needing `.journal/local/report.json`.
#[test]
fn preserve_current_with_only_already_satisfied_enforced_edges_is_unchanged() {
    let builder = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a")
        .mod_("b")
        .mod_("c")
        .mod_("d")
        .mod_("e")
        // Both edges already hold in insertion order (a before b, c before
        // d) — nothing here is violated, so nothing should move.
        .declared_edge("b", "a")
        .declared_edge("d", "c");
    let current = builder.insertion_order();
    let report = builder.build();

    let outcome = sort(&SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: TieBreak::PreserveCurrent,
    });
    assert_invariants(&report, &outcome, EnforcedLayers::default());

    assert_eq!(
        outcome.order.as_slice(),
        current.as_slice(),
        "with only already-satisfied enforced edges, PreserveCurrent must leave the current order unchanged"
    );
}

/// Install-calibrated finding-count upper bounds, named only through
/// `RIMMERGE_EXPECTED_SORT_BANDS=<name>:<max>,<name>:<max>,...` — these
/// counts (how many `DeclarationQuestioned`/`DuplicateTemplateName`/...
/// findings a real install produces) are a property of *that install's*
/// own mod set, not a structural fact any install must satisfy, so a
/// hardcoded number here would be a band that only happens to be true
/// today, so it lives behind a pin. Unset, a caller gets `None` back for every
/// name and skips the upper-bound assertion entirely — the surrounding
/// floor/determinism assertions and the measured-value `eprintln!` still
/// run regardless, so the test still asserts something on every machine
/// in the tier, just not a specific ceiling.
fn expected_sort_bands() -> std::collections::BTreeMap<String, u64> {
    let Ok(raw) = std::env::var("RIMMERGE_EXPECTED_SORT_BANDS") else {
        return std::collections::BTreeMap::new();
    };
    raw.split(',')
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| {
            let (name, max) = entry.split_once(':').unwrap_or_else(|| {
                panic!("RIMMERGE_EXPECTED_SORT_BANDS entries must be '<name>:<max>', got {entry}")
            });
            let max: u64 = max.parse().unwrap_or_else(|_| {
                panic!("RIMMERGE_EXPECTED_SORT_BANDS's {name} bound must be a number, got {max}")
            });
            (name.to_string(), max)
        })
        .collect()
}

/// Asserts `measured <= bound` when `bands` names `metric`; otherwise
/// only prints the measured value as the candidate line for a future
/// pin — never panics for a name `bands` doesn't carry, since an unset
/// `RIMMERGE_EXPECTED_SORT_BANDS` (or one that simply doesn't mention
/// this metric yet) is a normal, expected state, not an error.
fn assert_sort_band(bands: &std::collections::BTreeMap<String, u64>, metric: &str, measured: u64) {
    match bands.get(metric) {
        Some(&bound) => assert!(
            measured <= bound,
            "{metric} count regressed past the pinned bound ({bound}): {measured}"
        ),
        None => {
            eprintln!("{metric}={measured} (no RIMMERGE_EXPECTED_SORT_BANDS entry -- not asserted)")
        }
    }
}

/// The maintainer's own real installed community placements, named only
/// through `RIMMERGE_EXPECTED_PLACEMENTS` so no third-party mod id is
/// hardcoded here.
/// Format: `<mod id>:top|bottom,...`.
///
/// This function is only ever called once `.journal/local/report.json` has
/// already been loaded — this file's own "am I in the real-install tier"
/// gate, since it has no `RIMMERGE_GAME_DIR` concept of its own (it reads
/// a cached report, never scans directly). Unset at that point **panics**,
/// the same three-state rule every `RIMMERGE_GAME_DIR`-gated guard in the
/// workspace already follows (`require_pin_var`, `crates/rim-analyzer/tests/common/mod.rs`
/// and its two siblings) — soft-skipping here would turn the whole
/// measurement into a silent no-op even when the report is present and
/// ready to check.
fn expected_placements() -> Vec<(ModId, Placement)> {
    let raw = std::env::var("RIMMERGE_EXPECTED_PLACEMENTS").unwrap_or_else(|_| {
        panic!("RIMMERGE_EXPECTED_PLACEMENTS is not set, but .journal/local/report.json is present -- this \
             measurement needs it (format: <mod id>:top|bottom,...). See .journal/local/real-install-pins.ps1."
        )
    });
    raw.split(',')
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| {
            let (mod_id, placement) = entry.split_once(':').unwrap_or_else(|| {
                panic!(
                    "RIMMERGE_EXPECTED_PLACEMENTS entries must be '<mod id>:top|bottom', got \
                     {entry}"
                )
            });
            let placement = match placement {
                "top" => Placement::Top,
                "bottom" => Placement::Bottom,
                other => panic!(
                    "RIMMERGE_EXPECTED_PLACEMENTS placement must be 'top' or 'bottom', got {other}"
                ),
            };
            (ModId::new(mod_id), placement)
        })
        .collect()
}

/// The real install's named `ModDependency` pairs, as
/// `(dependent, provider, is_declared_contradiction)`, named only through
/// `RIMMERGE_EXPECTED_DECLARED_PAIRS` — same reasoning and the same
/// "panics once `.journal/local/report.json` is already loaded" rule as
/// [`expected_placements`] above; these are real, currently-active mod
/// ids checked against a real, live edge in the report, so unlike a
/// count band there is no shape-only fallback that still means
/// anything. Format: `<dependent>,<provider>,<true|false>;...` (`;`-joined
/// triples, `,`-joined fields).
fn expected_declared_pairs() -> Vec<(String, String, bool)> {
    let raw = std::env::var("RIMMERGE_EXPECTED_DECLARED_PAIRS").unwrap_or_else(|_| {
        panic!("RIMMERGE_EXPECTED_DECLARED_PAIRS is not set, but .journal/local/report.json is present -- this \
             measurement needs it (format: <dependent>,<provider>,<true|false>;...). See \
             .journal/local/real-install-pins.ps1."
        )
    });
    raw.split(';')
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| {
            let mut parts = entry.splitn(3, ',');
            let (Some(dependent), Some(provider), Some(contradiction)) =
                (parts.next(), parts.next(), parts.next())
            else {
                panic!(
                    "RIMMERGE_EXPECTED_DECLARED_PAIRS entries must be \
                     '<dependent>,<provider>,<true|false>', got {entry}"
                )
            };
            let contradiction = contradiction.parse::<bool>().unwrap_or_else(|_| {
                panic!(
                    "RIMMERGE_EXPECTED_DECLARED_PAIRS entry's third field must be 'true' or 'false', \
                     got {contradiction}"
                )
            });
            (dependent.to_string(), provider.to_string(), contradiction)
        })
        .collect()
}

#[test]
#[ignore = "reads the full, un-trimmed .journal/local/report.json; run explicitly with `cargo test -- --ignored`"]
fn full_size_report_invariants_and_runtime_budget() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.journal/local/report.json");
    let Some(report) = load_report(&path) else {
        eprintln!("skipping: {} not found", path.display());
        return;
    };

    let current = LoadOrder::new(report.mods.iter().map(|m| m.id.clone()).collect());

    // The sorter's default: the measurement this test's own numbers
    // record below is against `Rebuild`, not `PreserveCurrent` — see the
    // dedicated `PreserveCurrent` block further down for that mode's own
    // baseline.
    let start = Instant::now();
    let outcome = sort(&SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: TieBreak::Rebuild,
    });
    let elapsed = start.elapsed();

    assert_invariants(&report, &outcome, EnforcedLayers::default());

    eprintln!(
        "full-size sort (Rebuild): {} mods, {} dropped, {} any-of choices, {} warnings in {elapsed:?}",
        report.mods.len(),
        outcome.dropped.len(),
        outcome.any_of_choices.len(),
        outcome.warnings.len()
    );
    let resolved_any_of = outcome
        .any_of_choices
        .iter()
        .filter(|c| c.chosen.is_some())
        .count();
    eprintln!(
        "any-of choices resolved: {resolved_any_of} / {}",
        outcome.any_of_choices.len()
    );

    fn print_stats(label: &str, outcome: &SortOutcome, total: usize) {
        let s = &outcome.stats;
        eprintln!(
            "{label}: kendall_tau_inversions={} mods_displaced_over_10={} mods_displaced_over_50={} max_displacement={} positions_changed={} / {total}",
            s.kendall_tau_inversions,
            s.mods_displaced_over_10,
            s.mods_displaced_over_50,
            s.max_displacement,
            s.positions_changed
        );
    }

    fn displacement_cause(outcome: &SortOutcome, id: &ModId) -> String {
        outcome.placements.get(id).map_or_else(|| "<no placement explanation>".to_string(),
            |explanation| match (&explanation.tie_break.pulled_forward_by,
                &explanation.became_ready_after) {
                (Some(edge), _) => format!("pulled_forward_by: {} after {} ({:?}, layer {:?}) — effective_key {} vs current_position {:?}",
                    edge.after,
                    edge.before,
                    edge.provenance,
                    edge.layer,
                    explanation.tie_break.effective_key,
                    explanation.tie_break.current_position),
                (None, Some(edge)) => format!("became_ready_after: {} after {} ({:?}, layer {:?})",
                    edge.after, edge.before, edge.provenance, edge.layer
                ),
                (None, None) => format!("tier_reason: {:?}", explanation.tier_reason),
            })
    }

    fn print_top_n(label: &str, outcome: &SortOutcome, current: &LoadOrder, n: usize) {
        let mut displacements: Vec<(usize, ModId, usize, usize)> = Vec::new();
        for (new_pos, id) in outcome.order.as_slice().iter().enumerate() {
            if let Some(old_pos) = current.position(id) {
                let displacement = old_pos.abs_diff(new_pos);
                if displacement > 0 {
                    displacements.push((displacement, id.clone(), old_pos, new_pos));
                }
            }
        }
        displacements.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        eprintln!("--- top {n} displacements under {label} ---");
        for (displacement, id, old_pos, new_pos) in displacements.iter().take(n) {
            eprintln!(
                "{id}: {old_pos} -> {new_pos} (Δ{displacement}) — {}",
                displacement_cause(outcome, id)
            );
        }
    }

    /// Every mod pair whose relative order flipped between `current` and
    /// `outcome.order` — the exact pairs [`DisturbanceStats::kendall_tau_inversions`]
    /// counts, listed out (rather than just counted) so each one can be
    /// checked by hand against the edge or rule that forced it. `O(n^2)`;
    /// fine for a one-off diagnostic at ~1000 mods, and only meant to be
    /// read when the total is small.
    fn print_every_inversion(label: &str, outcome: &SortOutcome, current: &LoadOrder) {
        let suggested = outcome.order.as_slice();
        let mut pairs: Vec<(ModId, ModId)> = Vec::new();
        for i in 0..suggested.len() {
            let Some(pos_i) = current.position(&suggested[i]) else {
                continue;
            };
            for item_j in &suggested[i + 1..] {
                if let Some(pos_j) = current.position(item_j)
                    && pos_i > pos_j
                {
                    pairs.push((suggested[i].clone(), item_j.clone()));
                }
            }
        }
        eprintln!(
            "--- every inversion under {label} ({} pairs) ---",
            pairs.len()
        );
        for (earlier, later) in &pairs {
            // `earlier` now sits before `later` despite loading after it in
            // `current`. Print both mods' own displacement causes — the
            // pair is inverted because at least one of the two moved; the
            // other one's line will show no real edge if it simply held
            // still and got crossed.
            eprintln!("  ({earlier}, {later}):");
            eprintln!(
                "    {earlier} (current #{:?} -> suggested #{:?}) — {}",
                current.position(earlier),
                outcome.order.position(earlier),
                displacement_cause(outcome, earlier)
            );
            eprintln!(
                "    {later} (current #{:?} -> suggested #{:?}) — {}",
                current.position(later),
                outcome.order.position(later),
                displacement_cause(outcome, later)
            );
        }
    }

    // The defaults (Soft/Awareness both advisory), no imported rules —
    // this is the reference order's *only* disturbance: every
    // inversion here is a correction the sorter is actually making,
    // individually verifiable.
    print_stats("defaults, no imported rules", &outcome, report.mods.len());
    print_top_n("defaults, no imported rules", &outcome, &current, 10);
    print_every_inversion("defaults, no imported rules", &outcome, &current);

    // --- `PreserveCurrent` ---
    //
    // Headroom stays wide enough that a real new `Declared`/`Hard` edge or
    // two does not trip the checks below, while a regression that
    // fabricates evidence at scale (tens of thousands of inversions, as
    // treating an assembly's `0.0.0.0` as a real version produces) trips
    // them immediately.
    let preserve_current_start = Instant::now();
    let preserve_current_outcome = sort(&SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers::default(),
        tie_break: TieBreak::PreserveCurrent,
    });
    let preserve_current_elapsed = preserve_current_start.elapsed();
    assert_invariants(
        &report,
        &preserve_current_outcome,
        EnforcedLayers::default(),
    );
    print_stats(
        "PreserveCurrent, no imported rules",
        &preserve_current_outcome,
        report.mods.len(),
    );
    // `PreserveCurrent`'s own disturbance against `current` is a real,
    // install-specific measurement (how far `suggested` has drifted from
    // whatever the real `ModsConfig.xml` currently says) — it moves every
    // time the install or the applied order changes, so a hardcoded
    // number here would be a band that only happens to be true today.
    // Named only through
    // `RIMMERGE_EXPECTED_KENDALL_BASELINE=<inversions>,<positions>`:
    //
    // - **Set**: the pinned baseline's own `<= base * 3 + 4` headroom —
    //   tight enough to still catch a fabricated-evidence regression at
    //   this install's own scale, wide enough to survive ordinary
    //   post-apply drift.
    // - **Unset**: no install-specific number to check against, so this
    //   falls back to the one relationship that holds regardless of which
    //   install produced `report`: `PreserveCurrent`'s minimal-disturbance
    //   design must never accrue *more* disturbance against `current` than
    //   a from-scratch `Rebuild` does — `outcome` (built above) is exactly
    //   that comparison, already computed, no second sort needed.
    //
    // Re-measure and re-pin after applying a new order — a jump here does
    // **not** necessarily mean the sorter regressed, it can just mean
    // `current` and `suggested` have diverged further, the expected state
    // after any change that alters what the analyzer sees.
    match std::env::var("RIMMERGE_EXPECTED_KENDALL_BASELINE") {
        Ok(raw) => {
            let (base_inversions_str, base_positions_str) = raw.split_once(',').unwrap_or_else(|| {
                panic!(
                    "RIMMERGE_EXPECTED_KENDALL_BASELINE must be '<inversions>,<positions>', got {raw}"
                )
            });
            let base_inversions: u64 = base_inversions_str.parse().unwrap_or_else(|_| {
                panic!("RIMMERGE_EXPECTED_KENDALL_BASELINE's inversions field must be a number, got {base_inversions_str}")
            });
            let base_positions: usize = base_positions_str.parse().unwrap_or_else(|_| {
                panic!("RIMMERGE_EXPECTED_KENDALL_BASELINE's positions field must be a number, got {base_positions_str}")
            });
            assert!(
                preserve_current_outcome.stats.kendall_tau_inversions <= base_inversions * 3 + 4,
                "PreserveCurrent kendall_tau_inversions regressed past the pinned baseline ({base_inversions}) with headroom: {}",
                preserve_current_outcome.stats.kendall_tau_inversions
            );
            assert!(
                preserve_current_outcome.stats.positions_changed <= base_positions * 3 + 4,
                "PreserveCurrent positions_changed regressed past the pinned baseline ({base_positions}) with headroom: {}",
                preserve_current_outcome.stats.positions_changed
            );
        }
        Err(_) => {
            eprintln!(
                "RIMMERGE_EXPECTED_KENDALL_BASELINE not set -- only asserting PreserveCurrent's \
                 disturbance never exceeds Rebuild's (measured: PreserveCurrent {}/{}, Rebuild {}/{})",
                preserve_current_outcome.stats.kendall_tau_inversions,
                preserve_current_outcome.stats.positions_changed,
                outcome.stats.kendall_tau_inversions,
                outcome.stats.positions_changed
            );
            assert!(
                preserve_current_outcome.stats.kendall_tau_inversions
                    <= outcome.stats.kendall_tau_inversions,
                "PreserveCurrent must never accrue more disturbance than a from-scratch Rebuild: {} > {}",
                preserve_current_outcome.stats.kendall_tau_inversions,
                outcome.stats.kendall_tau_inversions
            );
            assert!(
                preserve_current_outcome.stats.positions_changed <= outcome.stats.positions_changed,
                "PreserveCurrent must never accrue more disturbance than a from-scratch Rebuild: {} > {}",
                preserve_current_outcome.stats.positions_changed,
                outcome.stats.positions_changed
            );
        }
    }

    // The named `ModDependency` pairs, real mod ids named only through
    // `RIMMERGE_EXPECTED_DECLARED_PAIRS`: every one of them must
    // still be a live `ModDependency` edge in the report — a regression
    // here means the analyzer stopped emitting one of these specific
    // edges, a silent failure the aggregate inversions/moved-mods counts
    // alone would not pin to a cause. A pair flagged as a contradiction is
    // a genuine Declared-vs-Declared contradiction (the dependent also
    // declares `loadBefore` its provider, or the provider declares
    // `loadAfter` the dependent), checked against `dropped` rather than
    // satisfaction. The kind-rank pair
    // (`RIMMERGE_EXPECTED_KIND_RANK_DEPENDENT` names its dependent side)
    // is one of them: its provider's own `About.xml` declares a
    // `<loadAfter>` naming the dependent directly. `kind_rank`
    // (`sort/cycles.rs`) ranks `ModDependency` below `LoadAfter`, so this
    // pair's dependency edge is always the one dropped and the opposing
    // `loadAfter` always wins.
    let declared_named_pairs = expected_declared_pairs();
    let kind_rank_dependent = std::env::var("RIMMERGE_EXPECTED_KIND_RANK_DEPENDENT").ok();
    for (dependent, provider, is_declared_contradiction) in declared_named_pairs {
        let dependent = ModId::new(dependent);
        let provider = ModId::new(provider);
        let has_edge = report.edges.iter().any(|e| {
            e.edge.kind == rim_analyzer::domain::EdgeKind::ModDependency
                && e.edge.after == dependent
                && e.edge.before == provider
        });
        assert!(
            has_edge,
            "the named ModDependency pair is missing its edge: {dependent} depends on {provider}"
        );
        if is_declared_contradiction {
            // Either direction between this exact pair may be the one the
            // drop tie-break picked (the ModDependency edge itself, or the
            // opposing `loadBefore`/`loadAfter` declaration it
            // contradicts) — what matters is that the contradiction was
            // *not* silently dropped without a trace.
            let dropped_edge = preserve_current_outcome.dropped.iter().find(|d| {
                (d.edge.after == dependent && d.edge.before == provider)
                    || (d.edge.after == provider && d.edge.before == dependent)
            });
            assert!(
                dropped_edge.is_some(),
                "the named Declared-vs-Declared contradiction ({dependent}, {provider}) must still surface as a dropped edge"
            );

            // The kind-rank pair is the one pair `sort/cycles.rs`'s
            // `kind_rank` is *known* to resolve in a specific direction, not
            // just "some edge was dropped" — an assertion that weak would
            // not notice this exact direction silently flipping. Pin
            // `kind_rank`'s real contract on real data: the dropped edge is
            // the dependent's own `ModDependency`, the edge that overruled it
            // is the provider's own contradicting `LoadAfter`, and the
            // emitted order actually reflects that (provider after
            // dependent). If `kind_rank` ever changes, this pinned shape must
            // change with it.
            if kind_rank_dependent.as_deref() == Some(dependent.as_str()) {
                let dropped_edge = dropped_edge.expect("checked above");
                let rim_resolve::sort::EdgeProvenance::Engine { kind, .. } =
                    &dropped_edge.edge.provenance
                else {
                    panic!(
                        "expected the kind-rank pair's dropped edge to be an engine edge, got {:?}",
                        dropped_edge.edge.provenance
                    );
                };
                assert_eq!(
                    *kind,
                    rim_analyzer::domain::EdgeKind::ModDependency,
                    "the kind-rank pair's ModDependency edge must be the one dropped, not the opposing LoadAfter"
                );
                assert_eq!(
                    dropped_edge.edge.after, dependent,
                    "the dropped ModDependency edge's dependent side must be {dependent}"
                );
                assert_eq!(
                    dropped_edge.edge.before, provider,
                    "the dropped ModDependency edge's provider side must be {provider}"
                );
                let winner = dropped_edge
                    .winner
                    .as_ref()
                    .expect("a direct two-mod contradiction must name a winner");
                let rim_resolve::sort::EdgeProvenance::Engine {
                    kind: winner_kind, ..
                } = &winner.provenance
                else {
                    panic!(
                        "expected the kind-rank pair's winner edge to be an engine edge, got {:?}",
                        winner.provenance
                    );
                };
                assert_eq!(
                    *winner_kind,
                    rim_analyzer::domain::EdgeKind::LoadAfter,
                    "the provider's own LoadAfter declaration must win over the dropped ModDependency"
                );
                assert_eq!(
                    winner.after, provider,
                    "the winning LoadAfter edge's dependent side must be {provider}"
                );
                assert_eq!(
                    winner.before, dependent,
                    "the winning LoadAfter edge's provider side must be {dependent}"
                );
                let provider_pos = preserve_current_outcome.order.position(&provider);
                let dependent_pos = preserve_current_outcome.order.position(&dependent);
                assert!(
                    provider_pos.is_some()
                        && dependent_pos.is_some()
                        && provider_pos > dependent_pos,
                    "the kind-rank pair's winning LoadAfter must actually hold in the emitted order: {provider} ({provider_pos:?}) must load after {dependent} ({dependent_pos:?})"
                );
            }
        } else {
            let dependent_pos = preserve_current_outcome.order.position(&dependent);
            let provider_pos = preserve_current_outcome.order.position(&provider);
            assert!(
                dependent_pos.is_some() && provider_pos.is_some() && provider_pos < dependent_pos,
                "the named ModDependency pair is not satisfied by PreserveCurrent: {dependent} ({dependent_pos:?}) must load after {provider} ({provider_pos:?})"
            );
        }
    }

    // The real install's `DeclarationQuestioned` count. Printed and
    // asserted against a pinned band with headroom for legitimate
    // drift (new advisory edges as the install's own mods update) rather
    // than pinned exactly, same rationale as the inversions checks above.
    let mods_by_id_for_ledger: std::collections::BTreeMap<ModId, &rim_analyzer::domain::Mod> =
        report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let declaration_questioned_ledger =
        rim_resolve::ledger::build(&rim_resolve::ledger::BuildLedgerInput {
            report: &report,
            sort_outcome: &preserve_current_outcome,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            decisions: &rim_resolve::domain::DecisionSet::new(),
            threshold: default_threshold(),
            source: rim_resolve::domain::OrderSource::Current,
            current: &current,
            suggested: &preserve_current_outcome.order,
            mods_by_id: &mods_by_id_for_ledger,
            show_dangling_def_references: false,
        });
    let declaration_questioned_count = declaration_questioned_ledger
        .entries
        .iter()
        .filter(|entry| matches!(entry.key, FindingKey::DeclarationQuestioned { .. }))
        .count();
    eprintln!("DeclarationQuestioned count: {declaration_questioned_count}");
    let bands = expected_sort_bands();
    assert_sort_band(
        &bands,
        "declaration_questioned",
        declaration_questioned_count as u64,
    );

    // The conflict-derived finding counts, each asserted against a pinned
    // band with headroom over a measured scan rather than only printed
    // (`ledger[...]: N entries...`, below): DuplicateTemplateName,
    // KeyedTranslationCollision (grouped pairs), SoundOverride,
    // RuntimePatchCollision (per target), and UndeclaredTypeDependency.
    // Real-install drift as mods update their defs/DLLs between scan
    // sessions moves these without any behavior change, the same rationale
    // as `DeclarationQuestioned`'s own headroom above.
    //
    // `UndeclaredTypeDependency`'s band is the one that catches path-shape
    // injected-node demotions labelled as `UsesType` (they belong to
    // `EdgeKind::PatchSelectsInjectedNode`), which inflate this finding
    // several-fold. Keep that band tight (about 1.7x the measured count,
    // close to `DeclarationQuestioned`'s ratio) so a partial recurrence
    // fails too, not only a full-scale one.
    fn count_kind(
        ledger: &rim_resolve::domain::Ledger,
        matches_kind: impl Fn(&FindingKey) -> bool,
    ) -> usize {
        ledger
            .entries
            .iter()
            .filter(|entry| matches_kind(&entry.key))
            .count()
    }
    let duplicate_template_name_count = count_kind(&declaration_questioned_ledger, |key| {
        matches!(key, FindingKey::DuplicateTemplateName { .. })
    });
    let keyed_translation_collision_count = count_kind(&declaration_questioned_ledger, |key| {
        matches!(key, FindingKey::KeyedTranslationCollision { .. })
    });
    let sound_override_count = count_kind(&declaration_questioned_ledger, |key| {
        matches!(key, FindingKey::SoundOverride { .. })
    });
    let runtime_patch_collision_count = count_kind(&declaration_questioned_ledger, |key| {
        matches!(key, FindingKey::RuntimePatchCollision { .. })
    });
    let undeclared_type_dependency_count = count_kind(&declaration_questioned_ledger, |key| {
        matches!(key, FindingKey::UndeclaredTypeDependency { .. })
    });
    eprintln!(
        "DuplicateTemplateName={duplicate_template_name_count} KeyedTranslationCollision={keyed_translation_collision_count} SoundOverride={sound_override_count} RuntimePatchCollision={runtime_patch_collision_count} UndeclaredTypeDependency={undeclared_type_dependency_count}"
    );
    assert_sort_band(
        &bands,
        "duplicate_template_name",
        duplicate_template_name_count as u64,
    );
    assert_sort_band(
        &bands,
        "keyed_translation_collision",
        keyed_translation_collision_count as u64,
    );
    assert_sort_band(&bands, "sound_override", sound_override_count as u64);
    assert_sort_band(
        &bands,
        "runtime_patch_collision",
        runtime_patch_collision_count as u64,
    );
    assert_sort_band(
        &bands,
        "undeclared_type_dependency",
        undeclared_type_dependency_count as u64,
    );

    // `PatchInjectedNode` edges on the current install (`Hard` strength).
    // `assert_invariants` above already proves every `Hard` edge either
    // holds or was dropped with a proven witness cycle, but that's
    // implicit; this makes the violation count explicit and counted, not
    // just structurally implied.
    let patch_injected_node_edges: Vec<_> = report
        .edges
        .iter()
        .filter(|e| e.edge.kind == rim_analyzer::domain::EdgeKind::PatchInjectedNode)
        .collect();
    let patch_injected_node_violated = patch_injected_node_edges
        .iter()
        .filter(|e| {
            evaluate::edge_status(&e.edge, &preserve_current_outcome.order) != EdgeStatus::Satisfied
        })
        .count();
    eprintln!(
        "PatchInjectedNode edges: {} ({} violated)",
        patch_injected_node_edges.len(),
        patch_injected_node_violated
    );
    assert!(
        patch_injected_node_violated <= 2,
        "PatchInjectedNode violations regressed past the measured baseline (0) with headroom: {patch_injected_node_violated}"
    );

    // The measured cost of each strictness toggle: re-sort with just that
    // one layer forced back to enforced, and compare its inversions
    // against the advisory-everywhere baseline. `PreserveCurrent`, not
    // `Rebuild`: the "cost of enforcing Soft/Awareness" is
    // current-order-relative, and `PreserveCurrent` is the mode that
    // measures against the real current order.
    let soft_enabled_outcome = sort(&SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers {
            soft: true,
            awareness: false,
            inferred: true,
        },
        tie_break: TieBreak::PreserveCurrent,
    });
    assert_invariants(
        &report,
        &soft_enabled_outcome,
        EnforcedLayers {
            soft: true,
            awareness: false,
            inferred: true,
        },
    );
    print_stats(
        "soft enabled (cost of enforcing Soft)",
        &soft_enabled_outcome,
        report.mods.len(),
    );

    let awareness_enabled_outcome = sort(&SortInput {
        report: &report,
        rules: &RuleSet::default(),
        tagging: &Tagging::default(),
        overrides: &SorterOverrides::default(),
        current: &current,
        enforce: EnforcedLayers {
            soft: false,
            awareness: true,
            inferred: true,
        },
        tie_break: TieBreak::PreserveCurrent,
    });
    assert_invariants(
        &report,
        &awareness_enabled_outcome,
        EnforcedLayers {
            soft: false,
            awareness: true,
            inferred: true,
        },
    );
    print_stats(
        "awareness enabled (cost of enforcing Awareness)",
        &awareness_enabled_outcome,
        report.mods.len(),
    );
    eprintln!(
        "cost of enforcing Soft: {} extra inversions over the PreserveCurrent ({}) baseline",
        soft_enabled_outcome
            .stats
            .kendall_tau_inversions
            .saturating_sub(preserve_current_outcome.stats.kendall_tau_inversions),
        preserve_current_outcome.stats.kendall_tau_inversions
    );
    eprintln!(
        "cost of enforcing Awareness: {} extra inversions over the same baseline",
        awareness_enabled_outcome
            .stats
            .kendall_tau_inversions
            .saturating_sub(preserve_current_outcome.stats.kendall_tau_inversions)
    );

    let mut by_layer: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for dropped_edge in &outcome.dropped {
        *by_layer
            .entry(format!("{:?}", dropped_edge.edge.layer))
            .or_insert(0) += 1;
    }
    eprintln!("dropped edges by layer: {by_layer:?}");

    // --- ledger, before/after the confidence-table changes -------------
    let mods_by_id: std::collections::BTreeMap<ModId, &rim_analyzer::domain::Mod> =
        report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let threshold = default_threshold();
    for source in [
        rim_resolve::domain::OrderSource::Current,
        rim_resolve::domain::OrderSource::Suggested,
    ] {
        let ledger = rim_resolve::ledger::build(&rim_resolve::ledger::BuildLedgerInput {
            report: &report,
            sort_outcome: &outcome,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            decisions: &rim_resolve::domain::DecisionSet::new(),
            threshold,
            source,
            current: &current,
            suggested: &outcome.order,
            mods_by_id: &mods_by_id,
            show_dangling_def_references: false,
        });
        eprintln!(
            "ledger[{source:?}]: {} entries, auto={} needs_input={} overridden={} resolved_by_suggested={}",
            ledger.entries.len(),
            ledger.stats.auto,
            ledger.stats.needs_input,
            ledger.stats.overridden,
            ledger.stats.resolved_by_suggested
        );
        print_needs_input_breakdown(&format!("{source:?}"), &ledger);
    }

    // Diagnostic only, sizing a possible `PatchCollision` confidence rule
    // — does NOT change `ledger/suggest.rs`. Mirrors the
    // `DefOverride` `same_author` rule: how many Contested patch
    // collisions have every contributor sharing at least one author?
    let mut contested_total = 0usize;
    let mut contested_same_author = 0usize;
    for conflict in &report.conflicts {
        if let rim_analyzer::domain::Conflict::PatchCollision(p) = conflict
            && p.severity == rim_analyzer::domain::PatchCollisionSeverity::Contested
        {
            contested_total += 1;
            let mut authors = p
                .mods
                .iter()
                .filter_map(|entry| mods_by_id.get(&entry.mod_id))
                .map(|m| m.authors.iter().cloned().collect::<BTreeSet<String>>());
            let shared = authors.next().map(|first| {
                authors.fold(first, |acc, next| {
                    acc.intersection(&next).cloned().collect()
                })
            });
            if shared.is_some_and(|s| !s.is_empty()) {
                contested_same_author += 1;
            }
        }
    }
    eprintln!(
        "contested patch collisions with a shared author across every contributor: {contested_same_author} / {contested_total} (proposal only, not applied)"
    );

    // A second candidate rule: how many Contested collisions have exactly
    // one contributor whose op class actually mutates the target (the
    // rest being purely additive)? This is a proxy for the analyzer's own
    // Additive/Contested classification (case-insensitive substring match
    // on replace/remove/insert/setname/attributeset), not an exact
    // reproduction of it — good enough to size the proposal, not to ship.
    let mutating_terms = ["replace", "remove", "insert", "setname", "attributeset"];
    let mut single_mutator = 0usize;
    let mut either_signal = 0usize;
    for conflict in &report.conflicts {
        if let rim_analyzer::domain::Conflict::PatchCollision(p) = conflict
            && p.severity == rim_analyzer::domain::PatchCollisionSeverity::Contested
        {
            let mutators = p
                .mods
                .iter()
                .filter(|entry| {
                    let lower = entry.op_class.to_lowercase();
                    mutating_terms.iter().any(|term| lower.contains(term))
                })
                .count();
            let is_single_mutator = mutators == 1;
            if is_single_mutator {
                single_mutator += 1;
            }
            let mut authors = p
                .mods
                .iter()
                .filter_map(|entry| mods_by_id.get(&entry.mod_id))
                .map(|m| m.authors.iter().cloned().collect::<BTreeSet<String>>());
            let shared = authors.next().map(|first| {
                authors.fold(first, |acc, next| {
                    acc.intersection(&next).cloned().collect()
                })
            });
            let is_same_author = shared.is_some_and(|s| !s.is_empty());
            if is_single_mutator || is_same_author {
                either_signal += 1;
            }
        }
    }
    eprintln!(
        "contested patch collisions with exactly one mutating contributor: {single_mutator} / {contested_total} (proposal only, not applied)"
    );
    eprintln!(
        "contested patch collisions matching either candidate signal: {either_signal} / {contested_total} (proposal only, not applied)"
    );

    // `RuleOverruled` fires only once imported pairs are on or a user
    // decision is overruled. With no rules fed at all (this test's own
    // baseline `RuleSet::default()`),
    // `SortOutcome::dropped` can never contain a `Rule`-provenance entry,
    // so `RuleOverruled` must be exactly 0 in both ledgers built above.
    for source in [
        rim_resolve::domain::OrderSource::Current,
        rim_resolve::domain::OrderSource::Suggested,
    ] {
        let ledger = rim_resolve::ledger::build(&rim_resolve::ledger::BuildLedgerInput {
            report: &report,
            sort_outcome: &outcome,
            rules: &RuleSet::default(),
            tagging: &Tagging::default(),
            decisions: &rim_resolve::domain::DecisionSet::new(),
            threshold: default_threshold(),
            source,
            current: &current,
            suggested: &outcome.order,
            mods_by_id: &mods_by_id,
            show_dangling_def_references: false,
        });
        let rule_overruled_count = ledger
            .entries
            .iter()
            .filter(|entry| matches!(entry.key, FindingKey::RuleOverruled { .. }))
            .count();
        assert_eq!(
            rule_overruled_count, 0,
            "RuleOverruled must be 0 with no rules fed at all ({source:?})"
        );
    }
    eprintln!(
        "RuleOverruled count with no imported rules: 0 (as expected — no rule-origin edge can be dropped)"
    );

    // `PlacementOverruled`/`PlacementQuestioned` fire for every community
    // placement a Hard/Declared edge crosses. This crate cannot import the
    // install's real community placements (no RimSort db files to read
    // here), and **no third-party mod is hardcoded here**:
    // `RIMMERGE_EXPECTED_PLACEMENTS=<mod id>:top|bottom,...` names the
    // maintainer's own known real community placements. `expected_placements`
    // itself panics when this is unset and `.journal/local/report.json` is
    // present (its own doc comment has the three-state rationale);
    // a genuinely empty *value* (`RIMMERGE_EXPECTED_PLACEMENTS=""`) still
    // degenerates to the "nothing to check" shape below, since every mod id
    // in the loop already tolerates "not present in this install's report".
    let real_placements = expected_placements();
    if let Some((single_mod_id, _)) = real_placements
        .iter()
        .find(|(_, placement)| *placement == Placement::Bottom)
    {
        // Pinning just this one known placement and re-sorting against the
        // real install's own real Hard/Declared edges still measures a
        // genuine (if partial) placement-finding count from real evidence,
        // not synthetic edges.
        let single_mod_id = single_mod_id.clone();
        let single_placement_rules = RuleSet::new(vec![Rule::Placement(PlacementRule {
            mod_id: single_mod_id.clone(),
            placement: Placement::Bottom,
            origin: RuleOrigin::RimSortCommunity,
            comment: None,
        })]);
        if report.mods.iter().any(|m| m.id.base() == single_mod_id) {
            let single_outcome = sort(&SortInput {
                report: &report,
                rules: &single_placement_rules,
                tagging: &Tagging::default(),
                overrides: &SorterOverrides::default(),
                current: &current,
                enforce: EnforcedLayers::default(),
                tie_break: TieBreak::PreserveCurrent,
            });
            let single_findings = rim_resolve::ledger::extract_findings(
                &report,
                &single_outcome,
                &Tagging::default(),
                &single_outcome.order,
                &single_placement_rules,
                false,
            );
            let placement_overruled = single_findings
                .values()
                .filter(|f| matches!(f, Finding::PlacementOverruled { .. }))
                .count();
            let placement_questioned = single_findings
                .values()
                .filter(|f| matches!(f, Finding::PlacementQuestioned { .. }))
                .count();
            eprintln!(
                "the pinned Bottom placement against {single_mod_id}'s real Bottom placement alone: PlacementOverruled={placement_overruled}, PlacementQuestioned={placement_questioned}"
            );
            // Asserted, not only printed, so a real regression here (the
            // placement suddenly losing to a Hard/Declared edge, or a burst
            // of new unsatisfiable advisory relations) fails the gate instead
            // of showing up only in eyeballed diagnostics.
            // `PlacementOverruled` is pinned exactly at 0 — a genuine
            // invariant of this placement holding cleanly under the real
            // install's real Hard/Declared edges, not a number expected to
            // drift; `PlacementQuestioned`'s pinned band tolerates
            // real-install churn (Workshop auto-updates between scan
            // sessions) the same way `DeclarationQuestioned`'s own assertion
            // above does.
            assert_eq!(
                placement_overruled, 0,
                "{single_mod_id}'s real Bottom placement must hold cleanly against the real install's \
                 Hard/Declared edges"
            );
            eprintln!(
                "PlacementQuestioned against {single_mod_id}'s real Bottom placement: {placement_questioned}"
            );
            assert_sort_band(&bands, "placement_questioned", placement_questioned as u64);
        } else {
            eprintln!(
                "skipping the pinned Bottom placement's {single_mod_id} measurement: not present in this install's report"
            );
        }
    } else {
        eprintln!(
            "skipping the single-placement measurement: RIMMERGE_EXPECTED_PLACEMENTS names no \
             Bottom placement (or is unset)"
        );
    }

    // With imported pairs off and placements on, the placed mods stay in
    // their own tier in both modes; with placements off in `Rebuild`, the
    // test documents (not asserts) where they land. Every real placement
    // the maintainer named via
    // `RIMMERGE_EXPECTED_PLACEMENTS` above, fed together (this crate cannot
    // depend on `rim-io` to import them live).
    let real_placement_rules = RuleSet::new(
        real_placements
            .iter()
            .map(|(mod_id, placement)| {
                Rule::Placement(PlacementRule {
                    mod_id: mod_id.clone(),
                    placement: *placement,
                    origin: RuleOrigin::RimSortCommunity,
                    comment: None,
                })
            })
            .collect(),
    );
    let expected_tiers: Vec<(ModId, rim_resolve::sort::Tier)> = real_placements
        .iter()
        .map(|(mod_id, placement)| {
            let tier = match placement {
                Placement::Top => rim_resolve::sort::Tier::Top,
                Placement::Bottom => rim_resolve::sort::Tier::Bottom,
            };
            (mod_id.clone(), tier)
        })
        .collect();
    for tie_break in [TieBreak::Rebuild, TieBreak::PreserveCurrent] {
        let placements_outcome = sort(&SortInput {
            report: &report,
            rules: &real_placement_rules,
            tagging: &Tagging::default(),
            overrides: &SorterOverrides::default(),
            current: &current,
            enforce: EnforcedLayers::default(),
            tie_break,
        });
        assert_invariants(&report, &placements_outcome, EnforcedLayers::default());
        for (mod_id, expected_tier) in &expected_tiers {
            let id = mod_id.clone();
            if !report.mods.iter().any(|m| m.id.base() == id) {
                eprintln!(
                    "skipping placement-tier check for {mod_id} under {tie_break:?}: not present in this install's report"
                );
                continue;
            }
            let actual_tier = placements_outcome
                .placements
                .get(&id)
                .map(|explanation| explanation.tier);
            assert_eq!(
                actual_tier,
                Some(*expected_tier),
                "{mod_id} must stay in {expected_tier:?} under {tie_break:?} with every named placement fed and no imported pairs"
            );
        }

        // `Tier` membership alone is not enough: the leaf pins (the ones
        // with nothing of their own promoted) must actually occupy the true
        // extreme slots, and `PlacementOrderingOverridden`/
        // `PlacementPromotesDependents`'s own real counts are measured too.
        let placements_findings = rim_resolve::ledger::extract_findings(
            &report,
            &placements_outcome,
            &Tagging::default(),
            &placements_outcome.order,
            &real_placement_rules,
            false,
        );
        let ordering_overridden = placements_findings
            .values()
            .filter(|f| matches!(f, Finding::PlacementOrderingOverridden { .. }))
            .count();
        let mut promotes_dependents_total = 0usize;
        let mut promoted_elsewhere: BTreeSet<ModId> = BTreeSet::new();
        // Not named `leaf_pins`: "no `PlacementPromotesDependents` finding"
        // is not the same fact as "no dependents": `find_promotion_cause`'s BFS
        // attributes a promoted mod to only the *nearest* pin (see its own
        // doc comment), so a pin whose only dependent got attributed to a
        // *different*, closer pin lands here too despite still genuinely
        // having a region-internal edge. The extreme-slot assertion below
        // gates on that directly (`lower_bounds`/`upper_bounds` emptiness,
        // not this list alone) — this list only tracks the finding-based
        // fact its own name promises.
        let mut pins_with_no_attributed_promotions: Vec<(ModId, rim_resolve::sort::Tier)> =
            Vec::new();
        for (mod_id, expected_tier) in &expected_tiers {
            let id = mod_id.clone();
            if !report.mods.iter().any(|m| m.id.base() == id) {
                continue;
            }
            let key = FindingKey::PlacementPromotesDependents {
                mod_id: id.clone(),
                placement: match expected_tier {
                    rim_resolve::sort::Tier::Top => Placement::Top,
                    _ => Placement::Bottom,
                },
            };
            match placements_findings.get(&key) {
                Some(Finding::PlacementPromotesDependents { promoted, .. }) => {
                    promotes_dependents_total += promoted.len();
                    promoted_elsewhere.extend(promoted.iter().cloned());
                }
                _ => pins_with_no_attributed_promotions.push((id, *expected_tier)),
            }
        }
        eprintln!(
            "{tie_break:?}: PlacementOrderingOverridden={ordering_overridden}, \
             PlacementPromotesDependents total attributed={promotes_dependents_total}, \
             pins with no attributed promotions={}",
            pins_with_no_attributed_promotions.len()
        );
        assert_sort_band(
            &bands,
            "placement_ordering_overridden",
            ordering_overridden as u64,
        );
        assert_sort_band(
            &bands,
            "placement_promotes_dependents",
            promotes_dependents_total as u64,
        );
        // Transitive attribution: a placement's own attributed total can
        // include mods promoted only *transitively* (through an intermediate
        // promoted mod, never touching the pin's own graph node) —
        // if every attributed promotion were also a direct collision,
        // `promotes_dependents_total` could never exceed `ordering_overridden`
        // by more than the leaf-pins-have-none gap. This is the only
        // assertion in this file that would fail if `find_promotion_cause`'s
        // BFS regressed to "direct dependents only".
        if promotes_dependents_total > 0 {
            assert!(
                promotes_dependents_total > ordering_overridden,
                "expected some transitively-attributed promotions beyond direct collisions under \
                 {tie_break:?}: attributed={promotes_dependents_total}, collisions={ordering_overridden}"
            );
        }
        // A pin with zero region-internal edges of its own in the blocking
        // direction has a one-element closure, the smallest there is, so
        // `extremize_region` moves its block *last* and it lands at the
        // true extreme of the whole order — after every mod attributed to
        // *any* Bottom placement, or before every mod attributed to *any*
        // Top placement — never merely tied with them alphabetically.
        // Gated on `lower_bounds`/`upper_bounds` emptiness directly, not
        // merely on `pins_with_no_attributed_promotions` membership — a pin
        // can land in that list while still genuinely blocked (see its own
        // comment above), and the sorter does not guarantee the extreme slot
        // for such a pin.
        for (leaf_id, tier) in &pins_with_no_attributed_promotions {
            let Some(explanation) = placements_outcome.placements.get(leaf_id) else {
                continue;
            };
            let has_region_internal_edge = match tier {
                rim_resolve::sort::Tier::Top => !explanation.lower_bounds.is_empty(),
                _ => !explanation.upper_bounds.is_empty(),
            };
            if has_region_internal_edge {
                continue;
            }
            let Some(leaf_pos) = placements_outcome.order.position(leaf_id) else {
                continue;
            };
            for other in &promoted_elsewhere {
                let Some(other_pos) = placements_outcome.order.position(other) else {
                    continue;
                };
                match tier {
                    rim_resolve::sort::Tier::Top => assert!(
                        leaf_pos < other_pos,
                        "leaf Top pin {leaf_id} (pos {leaf_pos}) must load before promoted mod \
                         {other} (pos {other_pos}) under {tie_break:?}"
                    ),
                    _ => assert!(
                        leaf_pos > other_pos,
                        "leaf Bottom pin {leaf_id} (pos {leaf_pos}) must load after promoted mod \
                         {other} (pos {other_pos}) under {tie_break:?}"
                    ),
                }
            }
        }

        // The closure block move's own invariant, asserted
        // against the real install in both tie-breaks — within the
        // `Bottom` region, every mod positioned after any explicit
        // `Bottom` pin is itself such a pin or lies in some pin's own
        // successor closure. The closure is rebuilt here from
        // `PlacementExplanation::upper_bounds` (the same accepted
        // outgoing `Real` edge set `sort::emit::closure_of` walks),
        // restricted to mods at or after the first `Bottom` pin — i.e.
        // to the region `apply_placement_extremes` actually reorders.
        let bottom_pins: Vec<ModId> = expected_tiers
            .iter()
            .filter(|(_, tier)| *tier == rim_resolve::sort::Tier::Bottom)
            .map(|(mod_id, _)| mod_id.clone())
            .filter(|id| placements_outcome.order.position(id).is_some())
            .collect();
        if let Some(region_start) = bottom_pins
            .iter()
            .filter_map(|id| placements_outcome.order.position(id))
            .min()
        {
            let total = placements_outcome.order.as_slice().len();
            let region: BTreeSet<ModId> = placements_outcome
                .order
                .as_slice()
                .iter()
                .skip(region_start)
                .cloned()
                .collect();
            let mut in_some_closure: BTreeSet<ModId> = BTreeSet::new();
            for pin in &bottom_pins {
                let mut closure: BTreeSet<ModId> = BTreeSet::new();
                closure.insert(pin.clone());
                let mut frontier = vec![pin.clone()];
                while let Some(current) = frontier.pop() {
                    let Some(explanation) = placements_outcome.placements.get(&current) else {
                        continue;
                    };
                    for edge in &explanation.upper_bounds {
                        if region.contains(&edge.after) && closure.insert(edge.after.clone()) {
                            frontier.push(edge.after.clone());
                        }
                    }
                }
                let position = placements_outcome.order.position(pin).unwrap_or(total);
                eprintln!(
                    "{tie_break:?}: Bottom pin {pin} at #{position} of {total}, closure {} mods, \
                     {} mods after it",
                    closure.len(),
                    total.saturating_sub(position + 1)
                );
                in_some_closure.extend(closure);
            }
            for (index, id) in placements_outcome
                .order
                .as_slice()
                .iter()
                .enumerate()
                .skip(region_start)
            {
                assert!(
                    in_some_closure.contains(id),
                    "{id} (#{index}) follows a Bottom pin under {tie_break:?} without being a \
                     Bottom pin or in some Bottom pin's own successor closure"
                );
            }
        }
    }
    // With placements off (this test's own `Rebuild`, no-imported-rules
    // `outcome` computed above) — documented, not asserted: the cost of
    // turning placements off is on record, not pinned as an invariant,
    // since nothing about the sorter
    // requires any particular landing spot once the one thing that placed
    // them is gone.
    for (mod_id, _) in &expected_tiers {
        let id = mod_id.clone();
        if let Some(pos) = outcome.order.position(&id) {
            eprintln!(
                "{mod_id} lands at position {pos} under Rebuild with placements off (no placement to hold it; not asserted)"
            );
        }
    }

    // `Rebuild` with no imported rules targets a 100 ms runtime; `elapsed`
    // above is the `Rebuild` run's own timing, checked here with headroom
    // (500ms) to stay CI-safe on a slower machine.
    if cfg!(debug_assertions) {
        eprintln!(
            "skipping the 500ms budget check in a debug build; run with --release to enforce it"
        );
    } else {
        assert!(
            elapsed.as_millis() < 500,
            "sort() (Rebuild) took {elapsed:?}, over the 500ms release-mode budget"
        );
    }

    // `PreserveCurrent`'s own timing: `sort::direction::edge_directions`
    // replaces the old per-node `BTreeSet` closures with rank-indexed
    // `fixedbitset`s, expected to be faster, not slower, than before — the
    // same 500ms release-mode budget, same debug-build skip, as `Rebuild`'s
    // own check above.
    if cfg!(debug_assertions) {
        eprintln!(
            "skipping the 500ms PreserveCurrent budget check in a debug build; run with \
             --release to enforce it"
        );
    } else {
        assert!(
            preserve_current_elapsed.as_millis() < 500,
            "sort() (PreserveCurrent) took {preserve_current_elapsed:?}, over the 500ms \
             release-mode budget"
        );
    }
}
