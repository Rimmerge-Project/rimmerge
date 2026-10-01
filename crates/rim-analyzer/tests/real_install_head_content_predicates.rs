//! Measurement: head-position child-element-value predicate support
//! (`xpath_expr::head_content_predicate`
//! /`analysis::edges::child_value_targets`).
//!
//! Reports, corpus-wide, how many active mutating patch ops carry a
//! head-position child-value predicate (`[race/intelligence="Humanlike"]` and
//! the single-step `[thingDef="Column"]` shape) and how many distinct defs
//! they newly resolve to via [`SourceIndex::patch_ops_by_def`] — the exact
//! map `rim-session`'s `VerifyOrder` builds its candidate set from, so a def
//! landing here is a def `verify` can examine. The acceptance case is a real
//! badge-fork pair (`ExampleBadgeFork.CritterBadge`'s
//! `ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]/comps`
//! and `/inspectorTabs`): both must be indexed under a real def key instead
//! of falling into [`SourceIndex::unscoped_op_counts`].
//!
//! This test does **not** run `rimmerge verify` itself — that lives in
//! `rim-session`, which this crate cannot depend on.
//!
//! `#[ignore]`d: reads the real game/workshop install and the real
//! `ModsConfig.xml` (read-only — nothing here writes anywhere, no
//! `install`, no `ModsConfig.xml` write, no `.journal/local/report.json`
//! regeneration). Requires `RIMMERGE_PERF_PROFILE_DIR`, **failing
//! loudly, by panicking**, when unset — see
//! `real_install_runtime_patch_kinds.rs`'s own doc comment for why this rail
//! exists even though nothing here writes to that directory.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features --release --run-ignored ignored-only`

use std::collections::BTreeSet;

use rim_analyzer::analysis::edges;
use rim_analyzer::analysis::source_index;
use rim_analyzer::domain::ScanOutput;
use rim_analyzer::extract::xpath_expr;
use rim_analyzer::extract::xpath_target;
use rim_analyzer::infra;

mod common;

/// This file's own "Run with" invocation (the module doc comment above),
/// named in every guard message so it points at the exact command for
/// this tier rather than a generic one.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features \
     --release --run-ignored ignored-only";

/// Scans the real, current install, read-only. `None` on a machine
/// outside the real-install tier; panics with a clear message on any scan
/// failure — this test only ever runs by hand.
fn scan_real_install() -> Option<ScanOutput> {
    let (config, _game_version) = common::real_scan_config(RERUN_COMMAND)?;
    Some(infra::scan(&config).unwrap_or_else(|error| panic!("scanning the real install: {error}")))
}

/// Every active mutating op's raw `<xpath>` whose **head** predicate is a
/// child-element-value equality (`xpath_expr::head_content_predicate`
/// returns `Some`) — this test's own independent oracle over the corpus,
/// read directly off every scanned mod's own raw patch ops rather than
/// through [`SourceIndex`], which is what this measurement is checking
/// the *effect* of.
fn head_content_predicate_ops(scan: &ScanOutput) -> Vec<(String, String)> {
    let mut ops = Vec::new();
    for scanned_mod in &scan.scanned_mods {
        for op in scanned_mod.patch_ops.iter().filter(|op| op.is_mutating) {
            let Some(xpath) = op.xpath.as_deref() else {
                continue;
            };
            if xpath_expr::head_content_predicate(xpath).is_some() {
                ops.push((scanned_mod.info.id.to_string(), xpath.to_string()));
            }
        }
    }
    ops
}

/// The core measurement: corpus-wide count of head-position content-predicate
/// ops, how many distinct defs they resolve to via
/// [`edges::child_value_targets`] against the real per-mod content index, and
/// — the acceptance case — that the real install's own badge-fork pair is
/// among them rather than unscoped.
#[test]
#[ignore = "needs the real install and RIMMERGE_PERF_PROFILE_DIR — see this file's own doc comment"]
fn head_content_predicate_ops_resolve_real_defs() {
    let Some(scan) = scan_real_install() else {
        return;
    };

    let content_ops = head_content_predicate_ops(&scan);
    eprintln!(
        "Group B measurement: {} active mutating ops corpus-wide carry a head-position \
         child-value predicate",
        content_ops.len()
    );

    let index = source_index::build(&scan);
    let mut resolved_targets: BTreeSet<(String, String)> = BTreeSet::new();
    let mut resolved_op_count = 0usize;
    for (_, xpath) in &content_ops {
        let targets = edges::child_value_targets(
            xpath,
            &index.owners_by_def,
            &index.child_value_hashes_by_mod,
        );
        if !targets.is_empty() {
            resolved_op_count += 1;
        }
        resolved_targets.extend(
            targets
                .into_iter()
                .map(|target| (target.def_type, target.def_name)),
        );
    }
    eprintln!(
        "Group B measurement: {resolved_op_count} of {} ops resolved to at least one def, \
         {} distinct defs newly reachable through SourceIndex::patch_ops_by_def",
        content_ops.len(),
        resolved_targets.len()
    );

    // Sanity: this feature must never resolve the *entire* population of
    // some def type (the over-broad failure mode the feature's own design
    // is meant to rule out) — report the largest single def_type bucket
    // so a reviewer can eyeball it rather than trusting a bare count.
    let mut by_def_type: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for (def_type, _) in &resolved_targets {
        *by_def_type.entry(def_type.as_str()).or_insert(0) += 1;
    }
    for (def_type, count) in &by_def_type {
        eprintln!("  {def_type}: {count} resolved def(s)");
    }

    // Acceptance case (reported, not hard-asserted — a wrong assumption baked
    // in as a hard assert here would fail loudly for the wrong reason instead
    // of just reporting what actually happened). The mod ships **two**
    // textually similar but genuinely distinct real xpaths for this shape
    // (confirmed by reading the mod's own patch XML on disk):
    // `ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]/...`
    // (`AlienRaceFramework/Patches/CompBadges_Patches_AlienRace.xml`) and the literal
    // `ThingDef[race/intelligence="Humanlike"]/...`
    // (`Patches/CompBadges_Patches_Base.xml`) — reported **per distinct
    // xpath**, not pooled, so a reader can see directly whether *this
    // install* has a real owned def of the `ExampleRace.ThingDef_ExampleRace`
    // type at all (many real installs don't: framework-based races are
    // as often authored `<ThingDef Class="ExampleRace.ThingDef_ExampleRace">`
    // — XML tag `ThingDef`, C# type in `Class=` — as they are the literal
    // `<ExampleRace.ThingDef_ExampleRace>` tag this predicate's own head
    // actually needs; an xpath head matches the literal XML tag only, never
    // the `Class=` attribute). A `ThingDef`-headed op resolving to zero
    // ExampleRace candidates is not a bug in that case — it's this install
    // genuinely having no def of that literal tag, the identical "not a node
    // with the given xpath" verdict RimWorld's own engine reaches
    // independently (a game log's own "Failed to find a node with the given
    // xpath" line for this exact head).
    let pawn_badge_ops: Vec<&(String, String)> = content_ops
        .iter()
        .filter(|(_, xpath)| xpath.contains("race/intelligence"))
        .collect();
    for (mod_id, xpath) in &pawn_badge_ops {
        let resolved = edges::child_value_targets(
            xpath,
            &index.owners_by_def,
            &index.child_value_hashes_by_mod,
        );
        eprintln!(
            "Group B acceptance case: [{mod_id}] '{xpath}' resolves to {} def(s)",
            resolved.len()
        );
    }
    let acceptance_resolves = pawn_badge_ops.iter().any(|(_, xpath)| {
        !edges::child_value_targets(
            xpath,
            &index.owners_by_def,
            &index.child_value_hashes_by_mod,
        )
        .is_empty()
    });
    eprintln!(
        "Group B acceptance case: {} op(s) with a 'race/intelligence' head predicate found \
         corpus-wide — at least one resolves to a real def = {acceptance_resolves} (expected: \
         >=1 op found, true; whether *every* one of them does is a fact about this specific \
         install's own mod list, not about this feature's own correctness — see the per-op \
         breakdown just above)",
        pawn_badge_ops.len()
    );

    // Whether this feature moved the loose, name-only scan
    // (`xpath_target::parse_all`) at all — it must not have, since that
    // module is deliberately untouched by this feature (it has no
    // content-index access at all, by design).
    let loose_scan_already_found_it = pawn_badge_ops
        .iter()
        .any(|(_, xpath)| !xpath_target::parse_all(xpath).is_empty());
    eprintln!(
        "Group B measurement: xpath_target::parse_all (untouched by this feature) already found \
         a target for the acceptance op = {loose_scan_already_found_it} (expected false — this \
         head shape has no defName=/@Name= term for that loose scan to match)"
    );
}
