//! Real-network, real-install verification for the `db`/`import`
//! surface: `db refresh` then `import --from-cache` against the real
//! RimWorld install, read-only, comparing the resulting rule counts
//! against the maintainer's own pinned baseline.
//!
//! **Deliberately does not follow this crate's own
//! `real_install_pair_rules.rs`/`real_install_assign.rs` skip-when-unset
//! convention.** Both of those early-return with a `skipping:` line when
//! `RIMMERGE_PERF_PROFILE_DIR` is unset — exactly the vacuous-green hazard
//! the root `CLAUDE.md` names by name. This test hits the real network
//! *and* reads the real install; a silent no-op here would be the worst
//! place in this crate for that failure mode, so it panics instead, naming
//! the missing variable, the moment it's asked to run without one.
//!
//! `#[ignore]`d so `cargo nextest run --workspace --all-features` (the
//! default gate) never depends on network access, GitHub's own
//! availability, or a real RimWorld install — it only ever counts as one
//! more skipped test. Run explicitly:
//!
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rimmerge-cli --all-features --release --run-ignored ignored-only -E 'binary(real_install_databases)'`
//!
//! Reads the real install (`ModsConfig.xml`, the game/workshop
//! directories, and RimSort's own live `userRules.json`) strictly
//! read-only, through the CLI's own default path resolution — no
//! `--game-dir`/`--workshop-dir`/`--mods-config` override, same as this
//! crate's other real-install tests. Every write (the rule-database cache
//! and the imported `rules.json`/`imports/` snapshot) lands under
//! `RIMMERGE_PERF_PROFILE_DIR`, never the real profile store, never the
//! real RimSort folder.

use std::path::PathBuf;

use assert_cmd::Command;

/// The maintainer's own real re-baseline (from a real `rimmerge import
/// --rimsort-dir "%LOCALAPPDATA%\RimSort\dbs"` run against their own
/// install), named only through
/// `RIMMERGE_EXPECTED_DB_COUNTS=user:<n>,community:<n>,steam:<n>` so no
/// install-specific number is hardcoded here — these counts are exactly
/// as install-specific as any other real-install measurement in this
/// workspace. `None` (the var unset) means there's nothing to compare
/// the real counts against; the caller prints them and skips the
/// tolerance check rather than asserting against a baseline that isn't
/// this machine's own.
fn expected_db_counts() -> Option<(usize, usize, usize)> {
    let raw = std::env::var("RIMMERGE_EXPECTED_DB_COUNTS").ok()?;
    let mut user = None;
    let mut community = None;
    let mut steam = None;
    for entry in raw.split(',') {
        let (name, value) = entry.split_once(':').unwrap_or_else(|| {
            panic!("RIMMERGE_EXPECTED_DB_COUNTS entries must be '<name>:<count>', got {entry}")
        });
        let value: usize = value.parse().unwrap_or_else(|_| {
            panic!("RIMMERGE_EXPECTED_DB_COUNTS's {name} count must be a number, got {value}")
        });
        match name {
            "user" => user = Some(value),
            "community" => community = Some(value),
            "steam" => steam = Some(value),
            other => panic!(
                "RIMMERGE_EXPECTED_DB_COUNTS names an unknown field {other:?} -- expected user/community/steam"
            ),
        }
    }
    let user = user.unwrap_or_else(|| panic!("RIMMERGE_EXPECTED_DB_COUNTS must name 'user'"));
    let community =
        community.unwrap_or_else(|| panic!("RIMMERGE_EXPECTED_DB_COUNTS must name 'community'"));
    let steam = steam.unwrap_or_else(|| panic!("RIMMERGE_EXPECTED_DB_COUNTS must name 'steam'"));
    Some((user, community, steam))
}

/// How far a real count may drift from the pinned baseline and still
/// count as "matches within tolerance". A pinned baseline is a specific
/// point in this install's own history, not a stable ground truth — real
/// counts drift day to day as active mods update their own rule
/// coverage, and community/Steam counts additionally depend on which
/// snapshot the GitHub mirror happens to be serving when this runs. 50%
/// is wide enough to absorb ordinary drift, narrow enough that a genuine
/// regression — the cache/`--rimsort-dir` resolution picking the wrong
/// source, a parser silently dropping most entries, `--user-rules`
/// pointed at the wrong file — would still fail it.
const TOLERANCE: f64 = 0.5;

#[allow(
    clippy::cast_precision_loss,
    reason = "these baselines are small (tens to low thousands); no precision is lost at f64"
)]
fn assert_within_tolerance(label: &str, actual: usize, baseline: usize) {
    let baseline_f = baseline as f64;
    let lower = (baseline_f * (1.0 - TOLERANCE)).floor() as usize;
    let upper = (baseline_f * (1.0 + TOLERANCE)).ceil() as usize;
    assert!(
        (lower..=upper).contains(&actual),
        "{label}: {actual} is outside the {lower}..={upper} band around the pinned \
         baseline of {baseline} — see this file's own doc comment on `TOLERANCE` before widening \
         it further"
    );
}

/// The tier's gate, in this file's own three-state shape:
/// `Some(game_dir)` when `RIMMERGE_GAME_DIR` names a real install,
/// `None` (after an honest `skipping:` line) when it is unset, and a
/// **panic** when it is set to something that isn't an install — a typo
/// must never quietly skip the one test that hits the real network.
///
/// Duplicated rather than taken from `tests/common/mod.rs` because this
/// binary deliberately does *not* share that module's
/// soft-skip-on-unset-profile-dir policy (see this file's own doc
/// comment), and including that module just for one predicate would
/// invite a future edit to route this file's own guard through it too.
fn require_game_dir() -> Option<PathBuf> {
    let var = rim_analyzer::infra::paths::GAME_DIR_VAR;
    match rim_analyzer::infra::paths::path_var(var) {
        Some(dir) if rim_analyzer::infra::paths::is_game_dir(&dir) => Some(dir),
        Some(dir) => panic!(
            "{var} is set to {} but that is not a RimWorld install (no Version.txt and \
             Data/Core/). A typo here must fail loudly, not skip the tier. Run: \
             {var}=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p \
             rimmerge-cli --all-features --release --run-ignored ignored-only -E \
             'binary(real_install_databases)'",
            dir.display()
        ),
        None => {
            eprintln!(
                "skipping: {var} is not set, so this machine is not in the real-install tier. \
                 Run: {var}=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest \
                 run -p rimmerge-cli --all-features --release --run-ignored ignored-only -E \
                 'binary(real_install_databases)'"
            );
            None
        }
    }
}

/// `RIMMERGE_PERF_PROFILE_DIR`, or a hard failure naming it — never a
/// silent skip. See this file's own doc comment.
///
/// Reached only after [`require_game_dir`] has already said yes:
/// "asked to run and misconfigured" must stay a loud failure (this file's
/// long-standing policy, and the reason it deviates from the rest of the
/// tier), but "not in the tier at all" is a skip like everywhere else, so
/// `--run-ignored ignored-only` on a machine with no `RIMMERGE_GAME_DIR`
/// doesn't fail on this one test alone.
fn scratch_dir() -> PathBuf {
    let base = std::env::var("RIMMERGE_PERF_PROFILE_DIR").unwrap_or_else(|_| {
        panic!(
            "RIMMERGE_PERF_PROFILE_DIR must be set to a scratch directory (never the real \
             profile) to run this test. This test hits the real network and reads the real \
             install, so it must never silently report a vacuous pass when misconfigured — see \
             this file's own doc comment. Run: RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo \
             nextest run -p rimmerge-cli --all-features --release --run-ignored ignored-only -E \
             'binary(real_install_databases)'"
        )
    });
    PathBuf::from(base)
}

/// RimSort's own live `userRules.json`, read-only — the same real,
/// never-copied location RimSort itself keeps it in
/// and `apps/desktop`'s `get_default_rimsort_paths` prefills from.
fn real_user_rules_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| panic!("LOCALAPPDATA must be set on this machine"));
    PathBuf::from(local_app_data)
        .join("RimSort")
        .join("dbs")
        .join("userRules.json")
}

/// Seeds a fresh scratch `rules.json`. Never touches the real profile:
/// `profile_dir` is always this test's own scratch directory.
fn seed_settings(profile_dir: &std::path::Path) {
    std::fs::create_dir_all(profile_dir)
        .unwrap_or_else(|error| panic!("create scratch profile dir: {error}"));
    std::fs::write(
        profile_dir.join("rules.json"),
        r#"{
            "version": 2,
            "pairs": [],
            "placements": [],
            "incompatibles": [],
            "tag_rules": [],
            "manual_tags": [],
            "settings": {
                "threshold": 80,
                "enforce_soft": false,
                "enforce_awareness": false,
                "suggest_merge_when_clean": true,
                "tie_break": "rebuild",
                "use_imported_pairs": false,
                "use_imported_placements": true
            }
        }"#,
    )
    .unwrap_or_else(|error| panic!("seed rules.json: {error}"));
}

/// Seeds a fresh scratch `app-settings.json` with both fetch toggles and
/// the network switch on — a brand-new machine otherwise defaults
/// `fetch_steam_workshop` to `false`, which would make `db
/// refresh --all` silently skip the one source this test most needs to
/// compare (a large share of the baseline's own rules). Never touches the
/// real `%LOCALAPPDATA%\rimmerge`: `base` is always this test's own
/// scratch directory, routed into the CLI via `RIMMERGE_PROFILE_DIR` (see
/// each `Command`'s own `.env(...)` below) — the network policy this file
/// used to seed on `rules.json` moved to this app-global file.
fn seed_app_settings(base: &std::path::Path) {
    std::fs::create_dir_all(base)
        .unwrap_or_else(|error| panic!("create scratch base dir: {error}"));
    std::fs::write(
        base.join("app-settings.json"),
        r#"{
            "schema": 1,
            "network": {
                "allow_network": true,
                "check_for_updates": true,
                "auto_refresh_rule_databases": true,
                "fetch_community_rules": true,
                "fetch_steam_workshop": true,
                "fetch_rimmerge_rules": true
            },
            "reminders": {
                "rule_databases_stale_after_days": 30
            }
        }"#,
    )
    .unwrap_or_else(|error| panic!("seed app-settings.json: {error}"));
}

/// Reads one `import`'s own `"<label>            : <value>"` output line
/// and parses its count — panics (not `None`) on anything else, including
/// the honest `not imported` text a `None` source prints, since this test
/// only ever expects real counts out of every source it explicitly asked
/// for.
fn parse_count(stdout: &str, label: &str) -> usize {
    let line = stdout
        .lines()
        .find(|line| line.trim_start().starts_with(label))
        .unwrap_or_else(|| panic!("no {label:?} line in:\n{stdout}"));
    let value = line
        .split(':')
        .nth(1)
        .unwrap_or_else(|| panic!("no ':' separator in line {line:?}"))
        .trim();
    value.parse().unwrap_or_else(|_| {
        panic!("{label:?}'s value {value:?} is not a plain count (a real install must never print \"not imported\" for a source this test explicitly requested)")
    })
}

#[test]
#[ignore = "hits the real network and reads the real install; run explicitly, see this file's own doc comment"]
fn refresh_then_import_from_cache_matches_the_pinned_baseline_within_tolerance() {
    if require_game_dir().is_none() {
        return;
    }
    let scratch = scratch_dir();
    let cache_dir = scratch.join("m7-verify-cache");
    let profile_dir = scratch.join("m7-verify-profile");
    let base_dir = scratch.join("m7-verify-base");
    seed_settings(&profile_dir);
    seed_app_settings(&base_dir);

    let user_rules_path = real_user_rules_path();
    assert!(
        user_rules_path.is_file(),
        "expected a real RimSort install with userRules.json at {} — without one, this test \
         can't compare the user-rules baseline; if this machine genuinely has no RimSort \
         install, that is itself a real finding to report, not a reason to fake a pass",
        user_rules_path.display()
    );

    let mut refresh = Command::cargo_bin("rimmerge").expect("binary must build");
    refresh
        .arg("db")
        .arg("refresh")
        .arg("--all")
        .arg("--cache-dir")
        .arg(&cache_dir)
        .arg("--profile-dir")
        .arg(&profile_dir)
        .env("RIMMERGE_PROFILE_DIR", &base_dir);
    let refresh_assert = refresh.assert().success();
    let refresh_stdout = String::from_utf8_lossy(&refresh_assert.get_output().stdout).into_owned();
    println!("db refresh output:\n{refresh_stdout}");
    assert!(
        refresh_stdout.contains("community: updated")
            || refresh_stdout.contains("community: unchanged"),
        "expected the community database to actually fetch (not be skipped), got:\n{refresh_stdout}"
    );
    assert!(
        refresh_stdout.contains("steam: updated") || refresh_stdout.contains("steam: unchanged"),
        "expected the steam workshop database to actually fetch (not be skipped), got:\n{refresh_stdout}"
    );

    let mut import = Command::cargo_bin("rimmerge").expect("binary must build");
    import
        .arg("import")
        .arg("--from-cache")
        .arg("--user-rules")
        .arg(&user_rules_path)
        .arg("--cache-dir")
        .arg(&cache_dir)
        .arg("--profile-dir")
        .arg(&profile_dir)
        .env("RIMMERGE_PROFILE_DIR", &base_dir);
    let import_assert = import.assert().success();
    let import_stdout = String::from_utf8_lossy(&import_assert.get_output().stdout).into_owned();
    println!("import output:\n{import_stdout}");

    let user_rules = parse_count(&import_stdout, "user rules");
    let community_rules = parse_count(&import_stdout, "community rules");
    let steam_dependencies = parse_count(&import_stdout, "steam dependencies");

    match expected_db_counts() {
        Some((baseline_user, baseline_community, baseline_steam)) => {
            println!(
                "real-install import counts: user_rules={user_rules} community_rules={community_rules} \
                 steam_dependencies={steam_dependencies} (pinned baseline: \
                 {baseline_user}/{baseline_community}/{baseline_steam})"
            );
            assert_within_tolerance("user rules", user_rules, baseline_user);
            assert_within_tolerance("community rules", community_rules, baseline_community);
            assert_within_tolerance("steam dependencies", steam_dependencies, baseline_steam);
        }
        None => {
            println!(
                "real-install import counts: user_rules={user_rules} community_rules={community_rules} \
                 steam_dependencies={steam_dependencies} (RIMMERGE_EXPECTED_DB_COUNTS not set -- not \
                 asserted against a baseline)"
            );
        }
    }
}
