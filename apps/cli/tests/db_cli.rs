//! `rimmerge db status`/`rimmerge db refresh` end to end, against a
//! pre-seeded temp cache directory — never the network, never the real
//! profile.
//!
//! **Deliberately not tested here**: "`db refresh` with no network exits
//! 0 and prints `failed:`". `crates/rim-io/src/databases/cache.rs`'s
//! `refresh` function calls `HttpGet::get` for real (unconditionally, no
//! host to fake it against from this crate); `status`, by contrast, never
//! calls it at all — only `Manifest::load`, which touches the cache
//! directory first and nothing else. Exercising a real `Failed` outcome
//! through this binary would therefore open a real socket — the one
//! thing the root `CLAUDE.md`'s hard network rule forbids in the default
//! `cargo nextest run --workspace --all-features` gate this test binary
//! belongs to. `format_refresh_line`'s `Failed` arm and
//! `refresh_has_failure`'s `--strict` decision are instead exhaustively
//! unit tested as pure functions in `src/commands/db.rs`'s own
//! `#[cfg(test)]` module — see that module's tests for the coverage this
//! file does not provide.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::tempdir;

mod common;
use common::{copy_dir_recursive, merge_game_fixture};

struct ScratchGame {
    game_dir: PathBuf,
    mods_config: PathBuf,
    workshop_dir: PathBuf,
    profile_dir: PathBuf,
    /// The app-global base (`<base>/app-settings.json`, `<base>/databases/`)
    /// — separate from `profile_dir`, since the network policy tested here
    /// moved out of the per-profile `rules.json` into this app-global file.
    /// Routed into the subprocess via `RIMMERGE_PROFILE_DIR`
    /// ([`path_args`]), the same env var `apps/cli/src/common.rs`'s
    /// `default_profile_base` already honours.
    base: PathBuf,
}

fn scratch_game(temp_dir: &Path) -> ScratchGame {
    let game_dir = temp_dir.join("game");
    copy_dir_recursive(&merge_game_fixture(), &game_dir)
        .unwrap_or_else(|error| panic!("copy merge_game fixture: {error}"));
    ScratchGame {
        mods_config: game_dir.join("ModsConfig.xml"),
        workshop_dir: temp_dir.join("workshop_does_not_exist"),
        profile_dir: temp_dir.join("profile"),
        base: temp_dir.join("base"),
        game_dir,
    }
}

fn path_args(cmd: &mut Command, game: &ScratchGame) {
    cmd.arg("--game-dir")
        .arg(&game.game_dir)
        .arg("--workshop-dir")
        .arg(&game.workshop_dir)
        .arg("--mods-config")
        .arg(&game.mods_config)
        .arg("--profile-dir")
        .arg(&game.profile_dir)
        .env("RIMMERGE_PROFILE_DIR", &game.base);
}

/// Seeds `<base>/app-settings.json` with `allow_network` **off** and
/// **every** fetch toggle hardcoded `true` (written out rather than
/// left to the shipped defaults, so the two tests using this helper keep
/// seeing the network-off skip apply to *every* source at once, rather
/// than one source's own toggle silently doing the work instead, even if
/// a default changes again).
///
/// The master switch is deliberately not a parameter: with every source on,
/// `allow_network: true` would reach the real `GithubRuleDatabaseFetcher` —
/// see [`seed_settings_every_source_disabled`]'s own doc comment for the
/// live request that shape would otherwise produce.
fn seed_settings_network_off(game: &ScratchGame) {
    fs::create_dir_all(&game.base).unwrap_or_else(|error| panic!("create base dir: {error}"));
    fs::write(
        game.base.join("app-settings.json"),
        r#"{
                "schema": 1,
                "network": {
                    "allow_network": false,
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

/// Seeds `<base>/app-settings.json` with **every** fetch toggle off and
/// `allow_network` on — the shape the source-disabled test needs: `db
/// refresh` must still print `skipped (source disabled)`, distinct from
/// `skipped (network refresh disabled)`.
///
/// **Hermetic only because every source is off, and that is load-bearing,
/// not incidental**. `RefreshRuleDatabases::execute` short-circuits at
/// `to_fetch.is_empty()` and never calls the port at all — but *one*
/// enabled source is enough to reach the real `GithubRuleDatabaseFetcher`
/// and open a live HTTPS connection from inside the default
/// `cargo nextest run --workspace` gate. A new source whose fetch toggle
/// defaults to `true`, added without updating this seed, does exactly
/// that — a real request (surfacing as, for example, a `failed:
/// unexpected HTTP status` line) in the hermetic gate, from a test whose
/// whole point is that no source is requested. **Any new `RuleDatabase`
/// variant must be turned off here
/// too**; the assertions below name each source explicitly so a new one
/// that silently stays on fails loudly rather than quietly dialling out.
fn seed_settings_every_source_disabled(game: &ScratchGame) {
    fs::create_dir_all(&game.base).unwrap_or_else(|error| panic!("create base dir: {error}"));
    fs::write(
        game.base.join("app-settings.json"),
        r#"{
            "schema": 1,
            "network": {
                "allow_network": true,
                "check_for_updates": true,
                "auto_refresh_rule_databases": true,
                "fetch_community_rules": false,
                "fetch_steam_workshop": false,
                "fetch_rimmerge_rules": false
            },
            "reminders": {
                "rule_databases_stale_after_days": 30
            }
        }"#,
    )
    .unwrap_or_else(|error| panic!("seed app-settings.json: {error}"));
}

/// The two skip wordings must be *distinct at the command line*, which a
/// unit test on `format_refresh_line` alone doesn't show end to end.
#[test]
fn refresh_with_every_source_disabled_prints_source_disabled_not_network_disabled() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    seed_settings_every_source_disabled(&game);
    let cache_dir = temp_dir.path().join("cache");

    let mut cmd = Command::cargo_bin("rimmerge").expect("binary must build");
    cmd.arg("db")
        .arg("refresh")
        .arg("--all")
        .arg("--cache-dir")
        .arg(&cache_dir);
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout).into_owned();
    assert!(
        stdout.contains("community: skipped (source disabled)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("steam: skipped (source disabled)"),
        "{stdout}"
    );
    // This line is what proves the third source was *requested and
    // skipped*, not requested and fetched — without it, a real HTTPS
    // request for that source would go unnoticed.
    assert!(
        stdout.contains("rimmerge: skipped (source disabled)"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("network refresh disabled"),
        "a disabled source must not print the network-disabled wording: {stdout}"
    );
    // Nothing may have been fetched: a `failed:`/`updated`/`unchanged`
    // line here means a source escaped the disabled gate and the gate is
    // no longer hermetic.
    assert!(
        !stdout.contains("failed:") && !stdout.contains("updated") && !stdout.contains("unchanged"),
        "no source may reach the network in this test: {stdout}"
    );
}

/// One line per source, each naming its own enabled state — the third
/// source included, so a source dropped from `db status` entirely fails
/// a test. Each line
/// is asserted with its own `enabled`/`disabled` word rather than a bare
/// substring, since `never fetched` alone would match whichever line
/// happened to be printed.
#[test]
fn status_on_an_empty_cache_prints_never_fetched_for_every_source_and_exits_zero() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let cache_dir = temp_dir.path().join("empty_cache");

    let mut cmd = Command::cargo_bin("rimmerge").expect("binary must build");
    cmd.arg("db")
        .arg("status")
        .arg("--cache-dir")
        .arg(&cache_dir);
    path_args(&mut cmd, &game);
    let output = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout).into_owned();

    // The three shipped defaults are all on (every source is
    // recommended). The automatic/manual label follows the app-global
    // auto-refresh toggle (on by default) plus each source's own
    // eligibility — Steam (49 MB) is never auto-refresh eligible,
    // regardless of that toggle, so it reads "manual".
    assert!(
        stdout.contains("community: enabled, automatic, never fetched"),
        "{stdout}"
    );
    assert!(
        stdout.contains("steam: enabled, manual, never fetched"),
        "{stdout}"
    );
    assert!(
        stdout.contains("rimmerge: enabled, automatic, never fetched"),
        "{stdout}"
    );
}

#[test]
fn status_on_a_seeded_cache_prints_the_sha12_and_size() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    let cache_dir = temp_dir.path().join("cache");
    fs::create_dir_all(&cache_dir).expect("create cache dir");
    let body = br#"{"rules":{}}"#;
    fs::write(cache_dir.join("communityRules.json"), body).expect("write community fixture");
    // sha256 of the exact bytes above (`sha256sum` on the same literal),
    // and its own byte count — the manifest below must agree, since `db
    // status` reads the manifest, not the file itself, for its
    // sha/byte-count columns.
    let sha256 = "b62ef528699f71d3972869cf01849f62c596ee92f37083eb82c11385f6b2c352";
    fs::write(
        cache_dir.join("manifest.json"),
        format!(
            r#"{{
                "version": 1,
                "sources": {{
                    "community_rules": {{
                        "sha256": "{sha256}",
                        "etag": null,
                        "bytes": {bytes},
                        "fetched_at": "2026-09-09T00:00:00Z",
                        "last_failure": null
                    }}
                }}
            }}"#,
            bytes = body.len()
        ),
    )
    .expect("write manifest");

    let mut cmd = Command::cargo_bin("rimmerge").expect("binary must build");
    cmd.arg("db")
        .arg("status")
        .arg("--cache-dir")
        .arg(&cache_dir);
    path_args(&mut cmd, &game);
    let sha12 = &sha256[..12];
    cmd.assert()
        .success()
        .stdout(predicates::str::contains(sha12))
        .stdout(predicates::str::contains(format!("{} bytes", body.len())));
}

#[test]
fn refresh_with_network_disabled_skips_every_source_with_the_network_wording_and_exits_zero() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    seed_settings_network_off(&game);
    let cache_dir = temp_dir.path().join("cache");

    let mut cmd = Command::cargo_bin("rimmerge").expect("binary must build");
    cmd.arg("db")
        .arg("refresh")
        .arg("--all")
        .arg("--cache-dir")
        .arg(&cache_dir);
    path_args(&mut cmd, &game);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains(
            "community: skipped (network refresh disabled)",
        ))
        .stdout(predicates::str::contains(
            "steam: skipped (network refresh disabled)",
        ))
        .stdout(predicates::str::contains(
            "rimmerge: skipped (network refresh disabled)",
        ));
}

#[test]
fn refresh_with_network_disabled_and_strict_still_exits_zero() {
    let temp_dir = tempdir().expect("tempdir");
    let game = scratch_game(temp_dir.path());
    seed_settings_network_off(&game);
    let cache_dir = temp_dir.path().join("cache");

    let mut cmd = Command::cargo_bin("rimmerge").expect("binary must build");
    cmd.arg("db")
        .arg("refresh")
        .arg("--all")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(&cache_dir);
    path_args(&mut cmd, &game);
    cmd.assert().success();
}
