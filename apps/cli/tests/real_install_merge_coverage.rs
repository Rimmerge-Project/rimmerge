//! Real-install verification for `MergeCoverage` at full size.
//! Deliberately does **not** pin real, drifting counts — they change
//! between scan sessions the same way every other real-install number in
//! this codebase does, per this crate's own `CLAUDE.md` convention: a
//! real-install `#[ignore]`d test asserts invariants and bands, never a
//! pinned count. Instead asserts the structural invariants
//! [`rim_session::use_cases::DefOverrideCoverage`]/[`rim_session::use_cases::PatchCollisionCoverage`]'s
//! own fields must satisfy on *any* install: every def override is
//! exactly one of "planning failed", "guard fired (for some field)",
//! "guard did not fire", or "guard re-read failed" (never more than one,
//! never none); a def override can never promote to `Merge` 85 without
//! its own preview being `Complete { op_count > 0 }` —
//! `redecide_for_clean_merge` never promotes `op_count: 0`,
//! `NeedsFieldInput`, or `CannotMerge`; and both `SuggestionOutcomeTally`
//! axes on both
//! tables sum to the same total, that total never exceeds the finding
//! count it was tallied over, `merge_85` always equals
//! `promotes_to_merge_85` on the def-override side, and a
//! `PatchCollision`'s own `guarded` bucket stays `0` (the structural
//! guard never applies to one).
//!
//! `#[ignore]`d: needs the real game/workshop install (read-only) and a
//! *copy* of a real profile directory in `RIMMERGE_PERF_PROFILE_DIR`
//! (never the real one). Builds its own [`rim_session::Session`] directly
//! (rather than reusing `apps/cli/src/common.rs::build_session`, which is
//! private to the binary crate and unreachable from an integration test)
//! — the same duplication `tests/real_install_defs.rs`/
//! `tests/real_install_assign.rs` already accept for the same reason.
//! Read-only on the install and on `ModsConfig.xml`: `MergeCoverage`
//! never calls `Apply`/`ExportAssignment`/anything else that writes.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-cli --all-features --release --run-ignored ignored-only -E 'binary(real_install_merge_coverage)'`

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use rim_session::Session;
use rim_session::use_cases::MergeCoverage;

/// This file's own "Run with" invocation (this module's own doc comment,
/// above) — named in [`common::require_profile_dir`]'s panic message.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p \
     rimmerge-cli --all-features --release --run-ignored ignored-only -E \
     'binary(real_install_merge_coverage)'";

fn load_real_session(profile_dir: PathBuf) -> Session {
    let paths = common::real_install_paths(profile_dir, RERUN_COMMAND);
    let use_case = rim_session::use_cases::LoadProject::new(
        rim_io::AnalyzerScanner::new(),
        rim_io::ModsConfigFileStore::new(),
        rim_io::JsonDecisionStore::new(),
        rim_io::JsonRuleStore::new(),
        rim_io::JsonPatchProjectStore::new(),
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FsModKnowledgeStore::vendored(),
        true,
    );
    let mut session = use_case
        .execute(paths, &mut |_| {})
        .unwrap_or_else(|error| panic!("load project: {error}"));
    session.set_def_source_reader(Arc::new(rim_io::FileDefSourceReader::new()));
    session
}

#[test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
fn def_override_coverage_buckets_partition_the_total_and_promotion_never_exceeds_complete_with_ops()
{
    let Some(profile_dir) = common::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let mut session = load_real_session(profile_dir);

    let use_case = MergeCoverage::new(rim_io::FileDefSourceReader::new());
    let report = use_case.execute(&mut session);
    let overrides = &report.def_overrides;

    eprintln!(
        "def overrides: total={} planning_failures={} guard_fires={} \
         guard_does_not_fire={} guard_reread_failed={} \
         complete_zero_ops={} complete_with_ops={} needs_field_input={} \
         cannot_merge={} promotes_to_merge_85={}",
        overrides.total,
        overrides.planning_failures,
        overrides.guard_fires.values().sum::<usize>(),
        overrides.guard_does_not_fire,
        overrides.guard_reread_failed,
        overrides.preview.complete_zero_ops,
        overrides.preview.complete_with_ops,
        overrides.preview.needs_field_input,
        overrides.preview.cannot_merge,
        overrides.promotes_to_merge_85
    );

    let guard_fires_total: usize = overrides.guard_fires.values().sum();
    assert_eq!(
        guard_fires_total
            + overrides.guard_does_not_fire
            + overrides.guard_reread_failed
            + overrides.planning_failures,
        overrides.total,
        "every def override is exactly one of planning-failed, guard-fired, \
         guard-did-not-fire, or guard-re-read-failed: {overrides:#?}"
    );

    assert!(
        overrides.promotes_to_merge_85 <= overrides.preview.complete_with_ops,
        "redecide_for_clean_merge only ever promotes a Complete{{op_count > 0}} \
         preview to Merge 85 — it can never promote more findings than that \
         bucket holds: {overrides:#?}"
    );

    // Every previewed finding's own preview state is exactly one of the
    // four `PreviewStateTally` buckets — the same partition invariant,
    // restated over `preview` alone (independent of the guard tally).
    let previewable = overrides.total - overrides.planning_failures;
    assert_eq!(
        overrides.preview.complete_zero_ops
            + overrides.preview.complete_with_ops
            + overrides.preview.needs_field_input
            + overrides.preview.cannot_merge,
        previewable,
        "{overrides:#?}"
    );

    // A `SuggestionOutcomeTally` invariant that catches a bucket
    // overcounting rows `redecide_for_clean_merge` never touches (such as
    // `WinnerDeclaresRelation`/`SameAuthor` rows). Both axes
    // sum to the same total — the number of findings that had both a real
    // preview *and* a ledger entry (`<= previewable`: a finding whose
    // ledger has no entry for it at all is silently skipped, same as
    // `promotes_to_merge_85`'s own disclosed gap) — and `merge_85` must
    // equal `promotes_to_merge_85` exactly, since both are read off the
    // identical redecision.
    let redecision_total = overrides.suggestion_outcomes.accept_95
        + overrides.suggestion_outcomes.accept_80
        + overrides.suggestion_outcomes.guarded
        + overrides.suggestion_outcomes.merge_85
        + overrides.suggestion_outcomes.unchanged
        + overrides.suggestion_outcomes.other_rewrite;
    let inbox_total =
        overrides.suggestion_outcomes.auto + overrides.suggestion_outcomes.needs_input;
    eprintln!(
        "def override suggestion outcomes: accept_95={} accept_80={} guarded={} \
         merge_85={} unchanged={} other_rewrite={} auto={} needs_input={}",
        overrides.suggestion_outcomes.accept_95,
        overrides.suggestion_outcomes.accept_80,
        overrides.suggestion_outcomes.guarded,
        overrides.suggestion_outcomes.merge_85,
        overrides.suggestion_outcomes.unchanged,
        overrides.suggestion_outcomes.other_rewrite,
        overrides.suggestion_outcomes.auto,
        overrides.suggestion_outcomes.needs_input
    );
    assert!(
        redecision_total <= previewable,
        "every bucketed finding must have had a real preview: {overrides:#?}"
    );
    assert_eq!(
        redecision_total, inbox_total,
        "both SuggestionOutcomeTally axes tally the same set of findings, \
         one increment each: {overrides:#?}"
    );
    // A DefOverride is never promoted to `Action::Merge` outright
    // (`Merge85` is `PatchCollision` only), so both counts compared here
    // are expected to read `0`.
    assert_eq!(
        overrides.suggestion_outcomes.merge_85, overrides.promotes_to_merge_85,
        "both are read off the identical redecision: {overrides:#?}"
    );
}

#[test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
fn patch_collision_suggestion_outcomes_never_exceed_contested_collisions() {
    let Some(profile_dir) = common::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let mut session = load_real_session(profile_dir);

    let use_case = MergeCoverage::new(rim_io::FileDefSourceReader::new());
    let report = use_case.execute(&mut session);
    let patch = &report.patch_collisions;

    eprintln!(
        "patch collision suggestion outcomes: accept_95={} accept_80={} guarded={} \
         merge_85={} unchanged={} other_rewrite={} auto={} needs_input={}",
        patch.suggestion_outcomes.accept_95,
        patch.suggestion_outcomes.accept_80,
        patch.suggestion_outcomes.guarded,
        patch.suggestion_outcomes.merge_85,
        patch.suggestion_outcomes.unchanged,
        patch.suggestion_outcomes.other_rewrite,
        patch.suggestion_outcomes.auto,
        patch.suggestion_outcomes.needs_input
    );

    let redecision_total = patch.suggestion_outcomes.accept_95
        + patch.suggestion_outcomes.accept_80
        + patch.suggestion_outcomes.guarded
        + patch.suggestion_outcomes.merge_85
        + patch.suggestion_outcomes.unchanged
        + patch.suggestion_outcomes.other_rewrite;
    let inbox_total = patch.suggestion_outcomes.auto + patch.suggestion_outcomes.needs_input;
    assert!(
        redecision_total <= patch.contested_collisions,
        "every bucketed finding must have had a real preview and a ledger \
         entry: {patch:#?}"
    );
    assert_eq!(
        redecision_total, inbox_total,
        "both SuggestionOutcomeTally axes tally the same set of findings, \
         one increment each: {patch:#?}"
    );
    // The structural guard never applies to a `PatchCollision` — `guarded` must stay `0`.
    assert_eq!(patch.suggestion_outcomes.guarded, 0, "{patch:#?}");
}
