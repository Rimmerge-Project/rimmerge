//! Wall-clock timing of the rules page's commands against the real
//! install — `#[ignore]`d because it needs the real game/workshop
//! directories (read-only) and a *copy* of a real profile directory
//! pointed at by `RIMMERGE_PERF_PROFILE_DIR` (never the real one: the
//! session may write `rules.json`/`decisions.json` there).
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-desktop --all-features --release --run-ignored ignored-only -E 'test(real_install_timing)'`

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::commands::rules::{list_orphaned_decisions_inner, list_rules_inner};
use crate::commands::tags::list_tags_inner;
use crate::dto::common::RuleOriginDto;
use crate::dto::rule::RuleFilterDto;
use crate::state::AppState;

/// This file's own "Run with" invocation (the module doc comment above),
/// named in every guard message so it points at the exact command for
/// this tier rather than a generic one.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-desktop \
     --all-features --release --run-ignored ignored-only";

/// The ceiling on `ContributesNothing`'s own flagged set. A band, never a pinned
/// mod list: the finding suggests disabling a mod at confidence 70, so
/// the thing worth asserting is that the pass stays selective, not which
/// particular Workshop subscriptions it happens to land on today.
///
/// **10, not 50.** The reference install flags only a couple of mods. A band
/// wide enough to swallow a 25x regression would let a bug like an unlowercased
/// `name_map` — which flags whole mods as inert because every
/// `PatchOperationFindMod` gate reads closed — pass
/// deterministically and unnoticed. 10 still leaves room for real
/// Workshop churn (a few more retexture mods losing every key) while a
/// pass that suddenly decides a tenth of the install does nothing fails.
/// `RIMMERGE_EXPECTED_CONTRIBUTES_NOTHING` pins the exact set for a
/// maintainer whose install is known — see `docs/testing.md`'s pins-file
/// pattern for how a maintainer sets it.
const MAX_CONTRIBUTES_NOTHING: usize = 10;

const REPEATS: usize = 5;

/// The budget for one [`rim_session::use_cases::VerifyOrder::execute`]
/// (counterfactual phase included) over a real install of about 1,000
/// active mods, in a release build. Measured at about 5 s per order on the
/// reference install when run after the install was loaded on the global
/// pool (it was about 12 s before each file's parse was cached); a pass that went back to
/// rebuilding the def index for every def took 74 to 108 s and fails here.
/// The margin absorbs the other tests in this tier running beside it, so
/// this guards the index, not the thread count.
const VERIFY_BUDGET: Duration = Duration::from_secs(60);

/// `<def type>/<def name>` of a real, live `DefOverride` finding this
/// machine's install carries that the identical-copy pass must promote
/// to `Accept` 99 — an exact def case has no honest weaker claim to fall
/// back to (see `docs/testing.md`'s pins-file pattern), so
/// `require_pin_var` panics rather than soft-skipping once the machine
/// is already confirmed to be in the tier.
const IDENTICAL_COPY_DEF_VAR: &str = "RIMMERGE_EXPECTED_IDENTICAL_COPY_DEF";

fn json_len<T: Serialize>(value: &T) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(0)
}

fn report(label: &str, samples: &[Duration], rows: usize, bytes: usize) {
    let mut sorted = samples.to_vec();
    sorted.sort();
    let min = sorted.first().copied().unwrap_or_default();
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or_default();
    eprintln!(
        "{label:<32} rows={rows:>6} json={bytes:>9} B  min={:>8.3} ms  median={:>8.3} ms",
        min.as_secs_f64() * 1e3,
        median.as_secs_f64() * 1e3
    );
}

#[tokio::test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
async fn rules_page_commands_against_the_real_install() {
    let Some(profile_dir) = crate::real_install_support::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let paths = crate::real_install_support::real_install_paths(profile_dir, RERUN_COMMAND);

    let state = AppState::default();
    let adapters = state.adapters.clone();
    let use_case = rim_session::use_cases::LoadProject::new(
        adapters.scanner,
        adapters.config_store,
        adapters.decision_store,
        adapters.rule_store,
        adapters.patch_store,
        adapters.assignment_store,
        rim_io::FsModKnowledgeStore::vendored(),
        true,
    );
    let start = Instant::now();
    let session = use_case.execute(paths, &mut |_| {}).expect("load project");
    eprintln!(
        "LoadProject: {:.1} ms, {} mods",
        start.elapsed().as_secs_f64() * 1e3,
        session.report().mods.len()
    );
    *state.session.write().expect("lock") = Some(session);

    let origins = [
        None,
        Some(RuleOriginDto::UserDecision),
        Some(RuleOriginDto::RimSortUser),
        Some(RuleOriginDto::RimSortCommunity),
        Some(RuleOriginDto::SteamDb),
    ];
    for origin in origins {
        let mut samples = Vec::with_capacity(REPEATS);
        let mut rows = 0;
        let mut bytes = 0;
        for _ in 0..REPEATS {
            let start = Instant::now();
            let rules = list_rules_inner(&state, RuleFilterDto { origin })
                .await
                .expect("list_rules");
            samples.push(start.elapsed());
            rows = rules.pairs.len() + rules.placements.len() + rules.incompatibles.len();
            bytes = json_len(&rules);
        }
        report(&format!("list_rules {origin:?}"), &samples, rows, bytes);
    }

    let mut samples = Vec::with_capacity(REPEATS);
    let mut rows = 0;
    let mut bytes = 0;
    for _ in 0..REPEATS {
        let start = Instant::now();
        let tagging = list_tags_inner(&state).await.expect("list_tags");
        samples.push(start.elapsed());
        rows = tagging.assignments.len();
        bytes = json_len(&tagging);
    }
    report("list_tags", &samples, rows, bytes);

    // First call builds both ledgers (cold); the rest hit the cache.
    let start = Instant::now();
    let orphaned = list_orphaned_decisions_inner(&state)
        .await
        .expect("list_orphaned_decisions");
    report(
        "list_orphaned_decisions (cold)",
        &[start.elapsed()],
        orphaned.len(),
        json_len(&orphaned),
    );
    let mut samples = Vec::with_capacity(REPEATS);
    for _ in 0..REPEATS {
        let start = Instant::now();
        let orphaned = list_orphaned_decisions_inner(&state)
            .await
            .expect("list_orphaned_decisions");
        samples.push(start.elapsed());
        rows = orphaned.len();
        bytes = json_len(&orphaned);
    }
    report("list_orphaned_decisions (warm)", &samples, rows, bytes);
}

/// Wall-clock cost of the clean-merge redecision pass ("merge-first
/// suggestions for clean previews") against the real install.
///
/// The pass is lazy (`Session::redecide_clean_merge_at`, run per finding from
/// `Session::resolution` — see its own doc comment) rather than run eagerly
/// inside `Session::ensure_ledger` for every finding on every ledger build,
/// which on a large install exceeds the 2s budget. This test measures that
/// shape:
///
/// 1. A cold `Session::ledger` build, with the setting off and on — must
///    be identical (and well under budget) either way, since building
///    the ledger itself never runs the pass.
/// 2. Resolving one inbox page's worth of findings
///    (`FindingIndex::MAX_PAGE_SIZE`, the worst single page the frontend
///    ever requests) through `Session::resolution` — the pass's actual
///    per-request cost, budgeted at 2s same as a ledger build (it's what
///    one `list_findings` call pays).
/// 3. Resolving *every* finding — the pass's full cost, equal to what
///    `RenderMergeMod::execute` (and so `apply`) pays eagerly across the
///    whole ledger. No budget asserted here: apply is a deliberate,
///    occasional action outside the inbox's own hot path (see
///    `RenderMergeMod::execute`'s own doc comment) — only recorded, so a
///    future regression here is visible.
///
/// Run with:
/// `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-desktop --all-features --release --run-ignored ignored-only -E 'test(real_install_timing::ledger_build)'`
#[tokio::test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
async fn ledger_build_with_and_without_the_clean_merge_pass() {
    let Some(profile_dir) = crate::real_install_support::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let paths = crate::real_install_support::real_install_paths(profile_dir, RERUN_COMMAND);

    let state = AppState::default();
    let adapters = state.adapters.clone();
    let use_case = rim_session::use_cases::LoadProject::new(
        adapters.scanner,
        adapters.config_store,
        adapters.decision_store,
        adapters.rule_store,
        adapters.patch_store,
        adapters.assignment_store,
        rim_io::FsModKnowledgeStore::vendored(),
        true,
    );
    let mut session = use_case.execute(paths, &mut |_| {}).expect("load project");
    session.set_def_source_reader(adapters.def_reader.clone());
    let source = rim_resolve::domain::OrderSource::Current;

    let mut off = session.settings();
    off.suggest_merge_when_clean = false;
    session.update_settings(off);
    let start = Instant::now();
    let baseline = session.ledger(source);
    let baseline_elapsed = start.elapsed();
    let finding_count = baseline.entries.len();

    let mut on = session.settings();
    on.suggest_merge_when_clean = true;
    session.update_settings(on); // invalidates the cached ledger
    let start = Instant::now();
    let with_pass = session.ledger(source);
    let cold_build_elapsed = start.elapsed();
    let with_pass_finding_count = with_pass.entries.len();
    let all_keys: Vec<rim_resolve::domain::FindingKey> = with_pass
        .entries
        .iter()
        .map(|entry| entry.key.clone())
        .collect();

    let page_keys = &all_keys[..rim_session::MAX_PAGE_SIZE.min(all_keys.len())];
    let start = Instant::now();
    for key in page_keys {
        session.resolution(source, key);
    }
    let one_page_elapsed = start.elapsed();

    let start = Instant::now();
    for key in &all_keys {
        session.resolution(source, key);
    }
    let every_finding_elapsed = start.elapsed();

    eprintln!(
        "ledger build, clean-merge redecide off:              {:>8.1} ms  ({finding_count} findings)",
        baseline_elapsed.as_secs_f64() * 1e3
    );
    eprintln!(
        "ledger build, clean-merge redecide on (cold):         {:>8.1} ms  ({with_pass_finding_count} findings)",
        cold_build_elapsed.as_secs_f64() * 1e3
    );
    eprintln!(
        "resolve one page ({} findings):      {:>8.1} ms",
        page_keys.len(),
        one_page_elapsed.as_secs_f64() * 1e3
    );
    eprintln!(
        "resolve every finding (apply-time): {:>8.1} ms",
        every_finding_elapsed.as_secs_f64() * 1e3
    );

    assert_eq!(
        with_pass_finding_count, finding_count,
        "the clean-merge pass must never add or remove findings, only re-derive suggestions"
    );
    assert!(
        cold_build_elapsed < Duration::from_secs(2),
        "a ledger build itself must stay under the 2s budget regardless of the setting: took {cold_build_elapsed:?}"
    );
    assert!(
        one_page_elapsed < Duration::from_secs(2),
        "resolving one inbox page must stay under the 2s budget: took {one_page_elapsed:?}"
    );
}

/// The first-open inbox cost: a `NeedsInput`-filtered inbox page runs
/// through `Session::findings`'s own fixpoint loop (page, redecide every item
/// on it, rebuild `FindingIndex` and re-page whenever anything changed).
/// Real-install findings can flip to `Auto` mid-page once the clean-merge
/// redecision runs, so the loop can iterate more than once before it
/// settles — this is exactly that cost,
/// isolated from [`ledger_build_with_and_without_the_clean_merge_pass`]'s
/// own `session.resolution()`-per-key measurement (which calls
/// `redecide_clean_merge_at` once per key but never exercises
/// `Session::findings`'s own re-page-until-settled loop at all). Mirrors
/// the desktop's own first-open inbox request exactly: `status:
/// NeedsInput`, no kind/mod/search filter, `limit: 50`
/// (`InboxPage.vue`'s own `DEFAULT_FILTER`/`pageSize`).
///
/// Run with:
/// `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-desktop --all-features --release --run-ignored ignored-only -E 'test(real_install_timing::first_open_inbox)'`
#[tokio::test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
async fn first_open_inbox_findings_fixpoint_cost_against_the_real_install() {
    let Some(profile_dir) = crate::real_install_support::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let paths = crate::real_install_support::real_install_paths(profile_dir, RERUN_COMMAND);

    let state = AppState::default();
    let adapters = state.adapters.clone();
    let use_case = rim_session::use_cases::LoadProject::new(
        adapters.scanner,
        adapters.config_store,
        adapters.decision_store,
        adapters.rule_store,
        adapters.patch_store,
        adapters.assignment_store,
        rim_io::FsModKnowledgeStore::vendored(),
        true,
    );
    let mut session = use_case.execute(paths, &mut |_| {}).expect("load project");
    session.set_def_source_reader(adapters.def_reader.clone());
    let source = rim_resolve::domain::OrderSource::Current;

    // The desktop inbox's own first-open request.
    let filter = rim_session::FindingFilter {
        status: Some(rim_resolve::domain::ResolutionStatus::NeedsInput),
        kinds: None,
        mod_id: None,
        search: None,
        offset: 0,
        limit: 50,
    };

    let mut off = session.settings();
    off.suggest_merge_when_clean = false;
    session.update_settings(off);
    let start = Instant::now();
    let baseline_page = session.findings(source, &filter);
    let baseline_elapsed = start.elapsed();

    let mut on = session.settings();
    on.suggest_merge_when_clean = true;
    session.update_settings(on); // invalidates the cached ledger
    let start = Instant::now();
    let cold_page = session.findings(source, &filter);
    let cold_elapsed = start.elapsed();

    // Warm: the fixpoint already settled on the call just above, so an
    // otherwise-identical call must converge on its very first pass —
    // isolates the *first-open* cost from the steady-state one.
    let start = Instant::now();
    let warm_page = session.findings(source, &filter);
    let warm_elapsed = start.elapsed();

    eprintln!(
        "first-open inbox, clean-merge redecide off:               {:>8.1} ms  (page {} of {})",
        baseline_elapsed.as_secs_f64() * 1e3,
        baseline_page.items.len(),
        baseline_page.total
    );
    eprintln!(
        "first-open inbox, clean-merge redecide on (cold):         {:>8.1} ms  (page {} of {})",
        cold_elapsed.as_secs_f64() * 1e3,
        cold_page.items.len(),
        cold_page.total
    );
    eprintln!(
        "first-open inbox, clean-merge redecide on (warm/settled): {:>8.1} ms  (page {} of {})",
        warm_elapsed.as_secs_f64() * 1e3,
        warm_page.items.len(),
        warm_page.total
    );

    assert!(
        cold_elapsed < Duration::from_secs(2),
        "the first-open inbox fixpoint must stay under the 2s budget even while findings are \
         still flipping to Auto mid-page: took {cold_elapsed:?}"
    );
    // Semantic assertions, mirroring
    // `ledger_build_with_and_without_the_clean_merge_pass`'s
    // own `with_pass_finding_count == finding_count` check just above in
    // this file: a wall-clock budget alone can't tell "the fixpoint
    // settled quickly because the redecision is working" apart from "the fixpoint
    // never iterated at all because `redecide_clean_merge_at` silently
    // became a no-op" — the second is a real regression that would
    // otherwise report itself as a *faster*, passing test.
    assert!(
        cold_page.total <= baseline_page.total,
        "the clean-merge redecision only ever moves findings out of NeedsInput, never into it: {} -> {}",
        baseline_page.total,
        cold_page.total
    );
    assert_eq!(
        warm_page.total, cold_page.total,
        "the fixpoint must have already settled on the cold call — a second, otherwise-identical \
         call finding a different total means it didn't"
    );
}

/// Wall-clock cost of the identical-copy content-check pass ("compare the
/// copies before calling a def override contested") against the real
/// install, and how much of it the pass actually promotes — mirroring
/// [`ledger_build_with_and_without_the_clean_merge_pass`]'s own method
/// exactly (same 2s-per-page budget reasoning), with the clean-merge pass
/// turned off so this test isolates the identical-copy pass's cost.
///
/// Also verifies a maintainer-pinned acceptance case
/// (`RIMMERGE_EXPECTED_IDENTICAL_COPY_DEF`, `<def type>/<def name>` of
/// two real content mods shipping a byte-identical copy of the same def
/// — a harmless flip the pass must call harmless) reads `Accept` 99
/// after this pass — if that def isn't a live `DefOverride` finding on
/// this machine's own install today (the Workshop content it depends on
/// could be gone or updated), the check is skipped rather than failing
/// the whole test on an environment fact this pass doesn't control.
///
/// Run with:
/// `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-desktop --all-features --release --run-ignored ignored-only -E 'test(real_install_timing::identical_copies)'`
#[tokio::test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
async fn identical_copies_pass_against_the_real_install() {
    let Some(profile_dir) = crate::real_install_support::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let paths = crate::real_install_support::real_install_paths(profile_dir, RERUN_COMMAND);
    let expected_identical_copy_def: Option<(String, String)> =
        crate::real_install_support::require_pin_var(IDENTICAL_COPY_DEF_VAR, RERUN_COMMAND).map(
            |raw| {
                let (def_type, def_name) = raw.split_once('/').unwrap_or_else(|| {
                    panic!("{IDENTICAL_COPY_DEF_VAR} must be '<def type>/<def name>', got {raw}")
                });
                (def_type.to_string(), def_name.to_string())
            },
        );

    let state = AppState::default();
    let adapters = state.adapters.clone();
    let use_case = rim_session::use_cases::LoadProject::new(
        adapters.scanner,
        adapters.config_store,
        adapters.decision_store,
        adapters.rule_store,
        adapters.patch_store,
        adapters.assignment_store,
        rim_io::FsModKnowledgeStore::vendored(),
        true,
    );
    let mut session = use_case.execute(paths, &mut |_| {}).expect("load project");
    session.set_def_source_reader(adapters.def_reader.clone());
    let source = rim_resolve::domain::OrderSource::Current;

    // Isolate the identical-copy pass: the clean-merge pass shares the same
    // `Session::resolution` call sites, so turn it off here rather than
    // measuring both passes combined.
    let mut settings = session.settings();
    settings.suggest_merge_when_clean = false;
    session.update_settings(settings);

    let baseline = session.ledger(source);
    let def_override_count = baseline
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.finding,
                rim_resolve::domain::Finding::DefOverride { .. }
            )
        })
        .count();
    let all_keys: Vec<rim_resolve::domain::FindingKey> = baseline
        .entries
        .iter()
        .map(|entry| entry.key.clone())
        .collect();

    let page_keys = &all_keys[..rim_session::MAX_PAGE_SIZE.min(all_keys.len())];
    let start = Instant::now();
    for key in page_keys {
        session.resolution(source, key);
    }
    let one_page_elapsed = start.elapsed();

    let start = Instant::now();
    for key in &all_keys {
        session.resolution(source, key);
    }
    let every_finding_elapsed = start.elapsed();

    let report = session.report().clone();
    let mods_by_id: std::collections::BTreeMap<
        rim_analyzer::domain::ModId,
        &rim_analyzer::domain::Mod,
    > = report.mods.iter().map(|m| (m.id.clone(), m)).collect();
    let mut promoted = 0usize;
    let mut promoted_same_author = 0usize;
    let mut promoted_unknown = 0usize;
    let mut pinned_def_promoted = false;
    let mut pinned_def_found = false;
    for key in &all_keys {
        let Some(resolution) = session.resolution(source, key) else {
            continue;
        };
        let rim_resolve::domain::Finding::DefOverride {
            key: def_key,
            owners,
            winner,
        } = &resolution.finding
        else {
            continue;
        };
        let is_identical_copies_promotion = resolution.suggestion.confidence.percent() == 99
            && resolution.suggestion.rationale.to_string()
                == "identical copies; order cannot change the outcome";
        if is_identical_copies_promotion {
            promoted += 1;
            match rim_resolve::ledger::def_override_direction(
                &report,
                def_key,
                owners,
                winner,
                &mods_by_id,
            ) {
                rim_resolve::ledger::DefOverrideDirection::SameAuthor => promoted_same_author += 1,
                rim_resolve::ledger::DefOverrideDirection::Unknown => promoted_unknown += 1,
                other => {
                    panic!("a promoted entry's own direction must be SameAuthor/Unknown: {other:?}")
                }
            }
        }
        if let Some((expected_type, expected_name)) = &expected_identical_copy_def
            && def_key.def_type == *expected_type
            && def_key.def_name == *expected_name
        {
            pinned_def_found = true;
            pinned_def_promoted = is_identical_copies_promotion;
        }
    }

    eprintln!(
        "resolve one page ({} findings), clean-merge redecide off:      {:>8.1} ms",
        page_keys.len(),
        one_page_elapsed.as_secs_f64() * 1e3
    );
    eprintln!(
        "resolve every finding (apply-time), clean-merge redecide off:  {:>8.1} ms  ({def_override_count} def_override findings total)",
        every_finding_elapsed.as_secs_f64() * 1e3
    );
    eprintln!(
        "identical-copy promotion promoted {promoted} of {def_override_count} def_override findings to Accept 99 ({promoted_same_author} SameAuthor, {promoted_unknown} Unknown)"
    );
    if let Some((expected_type, expected_name)) = &expected_identical_copy_def {
        eprintln!("{expected_type}/{expected_name} promoted to Accept 99: {pinned_def_promoted}");
        if pinned_def_found {
            assert!(
                pinned_def_promoted,
                "the pinned identical-copy def {expected_type}/{expected_name} is a live \
                 def_override finding but did not promote to Accept 99"
            );
        } else {
            eprintln!(
                "skipping the pinned-def assertion: {expected_type}/{expected_name} is not a \
                 live def_override finding on this machine's own install today"
            );
        }
    }

    assert!(
        one_page_elapsed < Duration::from_secs(2),
        "resolving one inbox page must stay under the 2s budget: took {one_page_elapsed:?}"
    );
}

/// Wall-clock cost
/// of [`rim_session::use_cases::VerifyOrder::execute`] against the real
/// install, for both `Current` and `Suggested` — this pass is never part
/// of `Session::compute`/the ledger build, so it gets its own, looser
/// budget ([`VERIFY_BUDGET`]) rather than the 2s every other pass in this
/// file meets. A second pass over the same order must produce the same
/// report: the def keys are replayed on several threads, and their
/// outcomes must still come out in key order.
/// Also reports the real cause breakdown (`RemovedBy`/`NotYetInjected`/
/// `DeadTarget`/`Unknown`), since a high `Unknown` count is itself a
/// signal the classifier is missing a case
/// (`PatchFailureCause::Unknown`'s own doc comment), and splits
/// `DeadTarget` further into the cheap zero-owner fast path (a patch
/// targeting a def that exists nowhere at all — a real, similarly-shaped xenotype def)
/// versus a genuine post-replay absence, since the two mean very
/// different things to a user (a stale patch versus a real load-order-
/// independent conflict).
///
/// Run with:
/// `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-desktop --all-features --release --run-ignored ignored-only -E 'test(real_install_timing::verify_order)'`
#[tokio::test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
async fn verify_order_against_the_real_install() {
    let Some(profile_dir) = crate::real_install_support::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let paths = crate::real_install_support::real_install_paths(profile_dir, RERUN_COMMAND);

    let state = AppState::default();
    let adapters = state.adapters.clone();
    let use_case = rim_session::use_cases::LoadProject::new(
        adapters.scanner,
        adapters.config_store,
        adapters.decision_store,
        adapters.rule_store,
        adapters.patch_store,
        adapters.assignment_store,
        rim_io::FsModKnowledgeStore::vendored(),
        true,
    );
    let mut session = use_case.execute(paths, &mut |_| {}).expect("load project");
    session.set_def_source_reader(adapters.def_reader.clone());

    let verify = rim_session::use_cases::VerifyOrder::new(adapters.def_reader.clone());

    for source in [
        rim_resolve::domain::OrderSource::Current,
        rim_resolve::domain::OrderSource::Suggested,
    ] {
        // The counterfactual phase's own budget, measured rather than
        // estimated: the same pass twice, once with the counterfactual
        // phase off and once with it on, so the delta is exactly that
        // phase's cost and nothing else (the scan, the sort and the
        // 10k-def main loop are identical in both).
        let start = Instant::now();
        let without = verify.execute_with_options(
            &session,
            source,
            rim_session::use_cases::VerifyOptions {
                counterfactual: false,
            },
        );
        let without_elapsed = start.elapsed();

        let start = Instant::now();
        let report = verify.execute(&session, source);
        let elapsed = start.elapsed();

        let mut removed_by = 0usize;
        let mut not_yet_injected = 0usize;
        let mut dead_target = 0usize;
        let mut dead_target_zero_owner = 0usize;
        let mut unknown = 0usize;
        for finding in &report.findings {
            let rim_resolve::domain::Finding::PatchWillFail {
                def_key,
                selector,
                cause,
                ..
            } = finding
            else {
                continue;
            };
            match cause {
                rim_resolve::domain::PatchFailureCause::RemovedBy(_) => removed_by += 1,
                rim_resolve::domain::PatchFailureCause::NotYetInjected(_) => not_yet_injected += 1,
                rim_resolve::domain::PatchFailureCause::DeadTarget => {
                    dead_target += 1;
                    // Not part of `VerifyOrderReport` itself (re-derived
                    // here from `session.sources()` instead): how much of
                    // `DeadTarget` is the cheap "this def has no owner
                    // anywhere" fast path (a real, similarly-shaped xenotype def) versus
                    // a genuine post-replay absence — see this test's own
                    // doc comment for why the split matters.
                    let key = (def_key.def_type.clone(), def_key.def_name.clone());
                    let has_owner = match selector {
                        rim_analyzer::domain::Selector::DefName => session
                            .sources()
                            .owners_by_def
                            .get(&key)
                            .is_some_and(|o| !o.is_empty()),
                        rim_analyzer::domain::Selector::NameAttr => session
                            .sources()
                            .templates
                            .get(&key)
                            .is_some_and(|r| !r.is_empty()),
                    };
                    if !has_owner {
                        dead_target_zero_owner += 1;
                    }
                }
                rim_resolve::domain::PatchFailureCause::Unknown => unknown += 1,
            }
        }

        eprintln!(
            "VerifyOrder {source:?}: {:>8.1} ms  ({} defs checked, {} skipped, {} predicted failures — {removed_by} RemovedBy, {not_yet_injected} NotYetInjected, {dead_target} DeadTarget [{dead_target_zero_owner} zero-owner fast path, {} post-replay], {unknown} Unknown)",
            elapsed.as_secs_f64() * 1e3,
            report.defs_checked,
            report.skipped.len(),
            report.findings.len(),
            dead_target - dead_target_zero_owner
        );
        let stats = &report.counterfactual;
        eprintln!(
            "  counterfactual {source:?}: {:>8.1} ms without, {:>8.1} ms with ({:+.1}%) — \n             {} jobs, {} Unknown resolved, {} DeadTarget demoted, {} alternatives \n             replayed, {} rejected for regression, {} for truncation, {} ties, \n             {} non-contiguous refusals; skipped {} too-many-mods / \n             {} too-many-subjects / {} job-ceiling / {} not-replayable / \n             {} co-owner / {} subject-not-found; K distribution {:?}",
            without_elapsed.as_secs_f64() * 1e3,
            elapsed.as_secs_f64() * 1e3,
            (elapsed.as_secs_f64() / without_elapsed.as_secs_f64() - 1.0) * 100.0,
            stats.jobs,
            stats.resolved,
            stats.demoted_dead_targets,
            stats.attempts,
            stats.rejected_for_regression,
            stats.rejected_for_truncation,
            stats.ties,
            stats.refused_non_contiguous,
            stats.skipped_too_many_mods,
            stats.skipped_too_many_subjects,
            stats.skipped_job_ceiling,
            stats.skipped_not_replayable,
            stats.skipped_co_owner,
            stats.skipped_subject_not_found,
            stats.mods_per_def
        );

        // Semantic, not wall-clock: the phase only ever *reclassifies*
        // surviving rows — it must never add, drop or re-key one. A
        // regression that made the phase emit or suppress a finding
        // would fail here; a slow machine would not.
        // Not just the count: the identity multiset. A phase that
        // dropped one row and emitted another would keep `len()` equal
        // while silently re-keying the population it is only allowed to
        // *reclassify* — the exact failure a length check cannot see.
        // `cause` is deliberately excluded from the key: changing it is
        // this phase's whole job.
        let identities = |findings: &[rim_resolve::domain::Finding]| {
            let mut keys: Vec<(String, String, String, String)> = findings
                .iter()
                .filter_map(|finding| match finding {
                    rim_resolve::domain::Finding::PatchWillFail {
                        mod_id,
                        def_key,
                        operation,
                        ..
                    } => Some((
                        mod_id.as_str().to_string(),
                        def_key.def_type.clone(),
                        def_key.def_name.clone(),
                        operation.clone(),
                    )),
                    _ => None,
                })
                .collect();
            keys.sort();
            keys
        };
        assert_eq!(
            identities(&report.findings),
            identities(&without.findings),
            "the counterfactual phase must never change the finding population"
        );
        assert_eq!(
            report.findings.len(),
            without.findings.len(),
            "including any non-PatchWillFail rows the identity key above ignores"
        );
        assert_eq!(report.defs_checked, without.defs_checked);
        assert_eq!(report.skipped.len(), without.skipped.len());
        assert!(
            elapsed < VERIFY_BUDGET,
            "VerifyOrder {source:?} took {elapsed:?}, over its {VERIFY_BUDGET:?} budget"
        );
        assert!(
            verify.execute(&session, source) == report,
            "a second VerifyOrder {source:?} pass over the same session produced a different report"
        );
        let reorder_bearing = |findings: &[rim_resolve::domain::Finding]| {
            findings
                .iter()
                .filter(|finding| {
                    matches!(
                        finding,
                        rim_resolve::domain::Finding::PatchWillFail {
                            cause: rim_resolve::domain::PatchFailureCause::RemovedBy(_)
                                | rim_resolve::domain::PatchFailureCause::NotYetInjected(_),
                            ..
                        }
                    )
                })
                .count()
        };
        assert_eq!(
            reorder_bearing(&report.findings),
            reorder_bearing(&without.findings) + stats.resolved + stats.demoted_dead_targets,
            "every resolution and demotion must land on a Reorder-bearing cause, and nothing else may move"
        );

        for (def_key, selector, reason) in &report.skipped {
            eprintln!("  skipped {def_key} ({selector:?}): {reason}");
        }

        // Optional, scratch-measurement only (never read by CI or any
        // other test): dumps every finding as one tab-separated line, so
        // a real run's own predictions can be cross-referenced against
        // a real `Player.log`'s own "Patch operation ... failed"
        // lines for a precision/recall measurement — the
        // aggregate counts above alone can't answer "does finding N
        // correspond to real log line M".
        if let Ok(dump_dir) = std::env::var("RIMMERGE_VERIFY_DUMP_DIR") {
            let path = PathBuf::from(&dump_dir).join(format!("verify_{source:?}.tsv"));
            let mut out = String::new();
            for finding in &report.findings {
                if let rim_resolve::domain::Finding::PatchWillFail {
                    mod_id,
                    def_key,
                    operation,
                    leaf_xpath,
                    cause,
                    ..
                } = finding
                {
                    // A patch author's own xpath text can carry literal
                    // newlines/tabs (long `defName="A" or defName="B" or
                    // ...` lists wrapped for readability in the source
                    // XML) — flattened to spaces so one finding is
                    // reliably one TSV line.
                    let flatten = |text: &str| -> String {
                        text.chars()
                            .map(|c| {
                                if c == '\n' || c == '\t' || c == '\r' {
                                    ' '
                                } else {
                                    c
                                }
                            })
                            .collect()
                    };
                    let flat_operation = flatten(operation);
                    let flat_leaf = leaf_xpath.as_deref().map(flatten).unwrap_or_default();
                    out.push_str(&format!(
                        "{cause:?}\t{mod_id}\t{}\t{}\t{flat_operation}\t{flat_leaf}\n",
                        def_key.def_type, def_key.def_name
                    ));
                }
            }
            std::fs::write(&path, out).expect("write verify dump");
            eprintln!(
                "  dumped {} findings to {}",
                report.findings.len(),
                path.display()
            );
        }
    }
}

/// Real-install check of the `ContributesNothing` pass, with its expectation
/// re-derived from a passing run rather than assumed (see
/// `crates/rim-session/src/use_cases/contributes_nothing.rs`'s own doc
/// comment for the rules the pass follows).
/// Soft-skips only outside the real-install tier — `RIMMERGE_GAME_DIR`
/// unset; inside it, an unset `RIMMERGE_PERF_PROFILE_DIR` panics instead,
/// naming the variable and the rerun command
/// ([`crate::real_install_support::require_profile_dir`]), never a
/// silent vacuous pass — see `docs/testing.md`'s three-state rule.
///
/// **The flagged set is asserted as a band and a determinism check, not
/// as a list of mod ids**: a list of survivors reads as a maintenance log of
/// one machine's Workshop subscriptions, and it goes stale the moment anyone
/// subscribes to anything. What is durable is recorded instead:
///
/// - **Every entry is hand-checked against the real mod files**, never
///   trusted from the pass's own output. The surviving entries on the
///   reference install are retexture mods whose every texture key loses
///   to a later-loading owner under the current order — evidence that
///   comes from `Conflict::TextureOverride` rows, so it never passes
///   through `patch_op_active`/`gate_open` at all.
/// - **Known false-positive shapes**: a mod shipping defName-less
///   `SongDef`s, and texture-only mods with uncontested content, which a
///   `Conflict::TextureOverride`-only texture check would wrongly catch.
/// - **Gate names must be lowercased**, and this is the one worth
///   remembering: `contributes_nothing.rs`'s `name_map` (fed to
///   `rim_analyzer::analysis::indices::patch_op_active`/`gate_open` via
///   `active_top_level_operations`) is lowercased because `gate_open`
///   lowercases the gate's named mod first, so a verbatim map would never
///   match a mixed-case display name and **every** `PatchOperationFindMod`
///   gate on the install would read permanently closed. A mod whose ops are
///   all gated would then fall straight through to `Ok(true)` ("inert")
///   having examined zero of them. **An indexing reconciliation does not
///   catch this, and the reason matters**: it only proves *indexing*
///   (`examined_locators.len() == cost.patch_ops`), which stays true
///   either way — it says nothing about whether the gate check then
///   admits any indexed op into `top_level`. Evidence about a target's
///   XML shape is not evidence about what this pass actually executed.
///
/// Which is why what this test asserts is the pass's *behaviour*:
/// non-empty, every member active, at most [`MAX_CONTRIBUTES_NOTHING`],
/// and identical across two runs over the same session. A maintainer who
/// wants the exact set pinned sets
/// `RIMMERGE_EXPECTED_CONTRIBUTES_NOTHING` in their own gitignored pins
/// file (`docs/testing.md`'s pins-file pattern).
///
/// Run with:
/// `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-desktop --all-features --release --run-ignored ignored-only -E 'test(real_install_timing::contributes_nothing)'`
#[tokio::test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
async fn contributes_nothing_against_the_real_install() {
    let Some(profile_dir) = crate::real_install_support::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let paths = crate::real_install_support::real_install_paths(profile_dir, RERUN_COMMAND);

    let state = AppState::default();
    let adapters = state.adapters.clone();
    let use_case = rim_session::use_cases::LoadProject::new(
        adapters.scanner,
        adapters.config_store,
        adapters.decision_store,
        adapters.rule_store,
        adapters.patch_store,
        adapters.assignment_store,
        rim_io::FsModKnowledgeStore::vendored(),
        true,
    );
    let mut session = use_case.execute(paths, &mut |_| {}).expect("load project");
    session.set_def_source_reader(adapters.def_reader.clone());

    let pass = rim_session::use_cases::ContributesNothing::new(adapters.def_reader.clone());
    let start = Instant::now();
    let result = pass
        .execute(&session, rim_resolve::domain::OrderSource::Current)
        .expect("OrderSource::Current is always supported");
    let elapsed = start.elapsed();

    let flagged: Vec<String> = result
        .findings
        .iter()
        .map(|finding| match finding {
            rim_resolve::domain::Finding::ContributesNothing { mod_id } => {
                mod_id.as_str().to_string()
            }
            other => panic!(
                "ContributesNothing::execute produced a non-ContributesNothing finding: {other:?}"
            ),
        })
        .collect();

    eprintln!(
        "ContributesNothing {:?}: {:>8.1} ms  ({} flagged: {flagged:?})",
        result.source,
        elapsed.as_secs_f64() * 1e3,
        flagged.len()
    );

    // Shape, band, and determinism — never a pinned mod list, which would
    // both name real mods and fail for the wrong reason the moment anyone
    // unsubscribes.
    assert!(
        !flagged.is_empty(),
        "a real modded install always has at least one mod contributing nothing under the \
         current order; an empty set means the pass stopped examining anything"
    );
    assert!(
        flagged.len() <= MAX_CONTRIBUTES_NOTHING,
        "{} mods flagged as contributing nothing, above the {MAX_CONTRIBUTES_NOTHING} ceiling -- \
         this finding's suggestion is \"safe to disable this mod\" at confidence 70, so a pass \
         that flags a large fraction of the install is a bug, not a discovery: {flagged:?}",
        flagged.len()
    );
    let active: std::collections::BTreeSet<rim_analyzer::domain::ModId> = session
        .report()
        .mods
        .iter()
        .map(|mod_info| mod_info.id.base())
        .collect();
    for mod_id in &flagged {
        assert!(
            active.contains(&rim_analyzer::domain::ModId::new(mod_id).base()),
            "{mod_id} was flagged but is not an active mod on this install"
        );
    }

    // The part that still has teeth: a second pass over the same session
    // must produce the identical set. `ContributesNothing` reads cached
    // previews and a def-source reader, so a non-deterministic pass (an
    // iteration-order leak, a cache that changes what it answers once
    // warm) would show up here and nowhere else in this tier.
    let second = pass
        .execute(&session, rim_resolve::domain::OrderSource::Current)
        .expect("OrderSource::Current is always supported");
    let second_flagged: Vec<String> = second
        .findings
        .iter()
        .map(|finding| match finding {
            rim_resolve::domain::Finding::ContributesNothing { mod_id } => {
                mod_id.as_str().to_string()
            }
            other => panic!(
                "ContributesNothing::execute produced a non-ContributesNothing finding: {other:?}"
            ),
        })
        .collect();
    assert_eq!(
        flagged, second_flagged,
        "two runs of the same pass over the same session must flag the same mods in the same \
         order (the workspace's determinism contract)"
    );

    // The maintainer's own escape hatch: an exact expected set, for the
    // one machine whose install is known. Comma-separated mod ids; unset
    // everywhere else, which is why it is an opt-in rather than a pin.
    if let Ok(expected) = std::env::var("RIMMERGE_EXPECTED_CONTRIBUTES_NOTHING") {
        let expected: Vec<String> = expected
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
        assert_eq!(
            flagged, expected,
            "RIMMERGE_EXPECTED_CONTRIBUTES_NOTHING was set, so the flagged set is pinned to it"
        );
    }
}
