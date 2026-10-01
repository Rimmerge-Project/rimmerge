//! `rimmerge import`: imports RimSort's three database files into the
//! current profile's rules.
//!
//! Community rules and the Steam dependency database (`communityRules.json`/
//! `steamDB.json`) have two sources: an explicit `--rimsort-dir` wins when
//! given; otherwise this command reads whichever of the two is both enabled
//! in the app-global `NetworkPolicy` and actually present in the global
//! rule-database cache
//! (`crate::common::resolve_cache_dir`, `--cache-dir` to override it) —
//! resolution order **explicit `--rimsort-dir` > the global cache >
//! nothing**. `userRules.json` has no remote (RimSort remains the tool that
//! edits it, Rimmerge only ever reads it live) and is therefore resolved
//! independently of both: `--user-rules <path>` points at it directly;
//! absent that, it comes from `--rimsort-dir` when that was given, and is
//! otherwise not part of this import at all (`RimSortPaths::user_rules:
//! None`). That flag combination — `import --from-cache` with neither
//! `--user-rules` nor `--rimsort-dir` — is why `Session::apply_import`
//! treats a `None` user-rules source as "leave any previously-imported
//! `RimSortUser` rules in `rules.json` untouched", never "delete them".
//!
//! The actual resolution (`should_import_from_cache`/`resolve_from_cache`/
//! `resolve_from_rimsort_dir`) lives in `rim_session::import_sources`, not
//! here — it's shared with `apps/desktop`'s own import command, so both
//! interfaces can import from the cache either one fetches. This module
//! only adds the two things that are genuinely this composition root's
//! own: the `--cache-dir`/`--from-cache` CLI flags, and the "nothing was in
//! the cache" warning below.

use std::path::PathBuf;

use anyhow::Context;
use clap::Args;
use rim_resolve::domain::Rule;
use rim_session::NetworkPolicy;
use rim_session::ports::RimSortPaths;
use rim_session::use_cases::ImportRimSort;

use crate::common::{
    PathsArgs, build_session, read_network_policy, resolve_cache_dir, resolve_paths,
};

#[derive(Debug, Args)]
pub struct ImportArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// Import community/steam rules from a local RimSort install instead
    /// of the global cache — a directory containing `userRules.json`
    /// directly, plus `Community-Rules-Database`/`Steam-Workshop-Database`
    /// subfolders holding `communityRules.json`/`steamDB.json`. RimSort's
    /// own default location is `%LOCALAPPDATA%\RimSort\dbs`. Also
    /// supplies `userRules.json` unless `--user-rules` overrides it.
    #[arg(long = "rimsort-dir", conflicts_with = "from_cache")]
    rimsort_dir: Option<PathBuf>,
    /// Import community/steam rules from the global cache (`rimmerge db
    /// refresh`'s own target) instead of a local RimSort install. This is
    /// already the default whenever `--rimsort-dir` isn't given — pass
    /// this to say so explicitly and get a warning if the cache turns out
    /// to have nothing usable in it.
    #[arg(long, conflicts_with = "rimsort_dir")]
    from_cache: bool,
    /// Reads `userRules.json` from this path directly. RimSort's hand-
    /// edited rules have no cache to fetch from, so this is the one
    /// source you always point at explicitly if you want it from
    /// somewhere other than `--rimsort-dir`. Without either flag, user
    /// rules are simply not part of this import (never a silent zero —
    /// the summary says "not imported").
    #[arg(long = "user-rules")]
    user_rules: Option<PathBuf>,
    /// See `db status`'s own `--cache-dir`.
    #[arg(long)]
    cache_dir: Option<PathBuf>,
}

/// Resolves the full [`RimSortPaths`] this import uses, per this
/// module's own doc comment: `--rimsort-dir` wins outright for
/// community/steam (and, absent `--user-rules`, for user rules too)
/// when given; otherwise every source comes from
/// [`rim_session::resolve_from_cache`].
///
/// The cache directory (`--cache-dir`, defaulting to
/// [`crate::common::resolve_cache_dir`]'s own `%LOCALAPPDATA%`-derived
/// path) is resolved lazily, only when `--rimsort-dir` was *not* given —
/// an `import --rimsort-dir <dir>` invocation never needs it and must
/// not fail just because `LOCALAPPDATA` happens to be unset.
fn resolve_rimsort_paths(
    args: &ImportArgs,
    policy: &NetworkPolicy,
) -> anyhow::Result<RimSortPaths> {
    if let Some(dir) = &args.rimsort_dir {
        return Ok(rim_session::resolve_from_rimsort_dir(
            dir,
            args.user_rules.as_deref(),
        ));
    }

    let cache_dir = resolve_cache_dir(args.cache_dir.clone())?;
    let fetcher = rim_io::GithubRuleDatabaseFetcher::new();
    let paths =
        rim_session::resolve_from_cache(&fetcher, policy, &cache_dir, args.user_rules.as_deref());
    if args.from_cache && paths.community_rules.is_none() && paths.steam_db.is_none() {
        eprintln!(
            "warning: --from-cache was given but no enabled source's cache file was found \
             under {} — nothing imported for community/steam rules",
            cache_dir.display()
        );
    }
    Ok(paths)
}

/// Renders one source's own imported-rule count — `None` (not part of
/// this import) prints a distinguishable `not imported`, never `0`
/// (a missing `userRules.json`
/// must never read as an imported zero). Takes `Option<&[Rule]>` (the
/// idiomatic borrow), not `&Option<Vec<Rule>>` — the parameter is a list
/// of rules to count, not a count itself, and a caller with an owned
/// `Option<Vec<Rule>>` still passes it in with `.as_deref()`.
fn format_count(rules: Option<&[Rule]>) -> String {
    match rules {
        Some(rules) => rules.len().to_string(),
        None => "not imported".to_string(),
    }
}

pub fn run(args: &ImportArgs) -> anyhow::Result<()> {
    let project_paths = resolve_paths(&args.paths)?;
    let mut session = build_session(project_paths)?;
    let policy = read_network_policy();
    let rimsort_paths = resolve_rimsort_paths(args, &policy)?;

    let importer = rim_io::RimSortImporter::new(session.paths().profile_dir.clone());
    let use_case = ImportRimSort::new(
        importer,
        rim_io::JsonRuleStore::new(),
        rim_io::JsonImportManifestStore::new(),
    );
    let imported = use_case
        .execute(&mut session, &rimsort_paths)
        .context("importing RimSort rules")?;

    println!(
        "user rules            : {}",
        format_count(imported.user_rules.as_deref())
    );
    println!(
        "community rules       : {}",
        format_count(imported.community_rules.as_deref())
    );
    println!(
        "steam dependencies    : {}",
        format_count(imported.steam_dependencies.as_deref())
    );
    println!(
        "skipped (rules, inactive): {}",
        imported.skipped_inactive_rules
    );
    println!(
        "skipped (steamDB, inactive): {}",
        imported.skipped_inactive_steam
    );
    println!(
        "snapshots copied to: {}",
        session.paths().profile_dir.join("imports").display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(fetch_community: bool, fetch_steam: bool) -> NetworkPolicy {
        NetworkPolicy {
            fetch_community_rules: fetch_community,
            fetch_steam_workshop: fetch_steam,
            ..NetworkPolicy::default()
        }
    }

    fn args(
        rimsort_dir: Option<&str>,
        from_cache: bool,
        user_rules: Option<&str>,
        cache_dir: Option<&str>,
    ) -> ImportArgs {
        ImportArgs {
            paths: PathsArgs {
                game_dir: None,
                workshop_dir: None,
                mods_config: None,
                profile_dir: None,
            },
            rimsort_dir: rimsort_dir.map(PathBuf::from),
            from_cache,
            user_rules: user_rules.map(PathBuf::from),
            cache_dir: cache_dir.map(PathBuf::from),
        }
    }

    #[test]
    fn format_count_reports_a_real_zero_distinctly_from_not_imported() {
        assert_eq!(format_count(Some(&[])), "0");
        assert_eq!(format_count(None), "not imported");
    }

    #[test]
    fn format_count_reports_the_rule_count() {
        use rim_analyzer::domain::ModId;
        use rim_resolve::domain::{PairRule, RuleOrigin};

        let rules = [
            Rule::Pair(PairRule {
                after: ModId::new("a"),
                before: ModId::new("b"),
                origin: RuleOrigin::RimSortCommunity,
                comment: None,
                overrides_declared: false,
            }),
            Rule::Pair(PairRule {
                after: ModId::new("b"),
                before: ModId::new("c"),
                origin: RuleOrigin::RimSortCommunity,
                comment: None,
                overrides_declared: false,
            }),
        ];
        assert_eq!(format_count(Some(&rules)), "2");
    }

    // -- `resolve_rimsort_paths`: source precedence, delegating to
    // `rim_session::resolve_from_rimsort_dir`/`resolve_from_cache` (unit
    // tested exhaustively in `rim-session` itself) --

    #[test]
    fn explicit_rimsort_dir_wins_for_community_and_steam_and_supplies_user_rules() {
        let args = args(Some("C:/rimsort"), false, None, None);
        let paths = resolve_rimsort_paths(&args, &policy(true, true))
            .expect("rimsort-dir mode never touches the cache directory");
        assert_eq!(
            paths.community_rules,
            Some(PathBuf::from(
                "C:/rimsort/Community-Rules-Database/communityRules.json"
            ))
        );
        assert_eq!(
            paths.steam_db,
            Some(PathBuf::from(
                "C:/rimsort/Steam-Workshop-Database/steamDB.json"
            ))
        );
        assert_eq!(
            paths.user_rules,
            Some(PathBuf::from("C:/rimsort/userRules.json"))
        );
    }

    #[test]
    fn explicit_user_rules_overrides_rimsort_dirs_own_user_rules_path() {
        let args = args(
            Some("C:/rimsort"),
            false,
            Some("C:/elsewhere/userRules.json"),
            None,
        );
        let paths = resolve_rimsort_paths(&args, &policy(true, false))
            .expect("rimsort-dir mode never touches the cache directory");
        assert_eq!(
            paths.user_rules,
            Some(PathBuf::from("C:/elsewhere/userRules.json"))
        );
    }

    #[test]
    fn no_rimsort_dir_and_no_user_rules_flag_means_user_rules_is_not_part_of_the_import() {
        // A nonexistent directory: `resolve_from_cache` must tolerate this
        // (empty cache, not an error) so this test stays hermetic — no
        // dependency on `LOCALAPPDATA` or any real cache directory.
        let args = args(
            None,
            true,
            None,
            Some("C:/nonexistent-cache-dir-for-this-test"),
        );
        let paths = resolve_rimsort_paths(&args, &policy(true, false))
            .expect("an absent cache directory is not an error, just an empty one");
        assert_eq!(
            paths.user_rules, None,
            "no --rimsort-dir and no --user-rules flag means user rules stay out of the import \
             entirely — this must stay None, not Some(missing path)"
        );
    }

    #[test]
    fn no_rimsort_dir_reads_community_and_steam_from_the_cache_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let community = dir.path().join("communityRules.json");
        let steam = dir.path().join("steamDB.json");
        std::fs::write(&community, b"{}").expect("write community fixture");
        std::fs::write(&steam, b"{}").expect("write steam fixture");

        let args = args(
            None,
            false,
            None,
            Some(dir.path().to_str().expect("utf8 path")),
        );
        let paths = resolve_rimsort_paths(&args, &policy(true, true)).expect("must resolve");
        assert_eq!(paths.community_rules, Some(community));
        assert_eq!(paths.steam_db, Some(steam));
    }

    #[test]
    fn a_disabled_source_is_skipped_even_when_its_cache_file_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("communityRules.json"), b"{}").expect("write fixture");
        std::fs::write(dir.path().join("steamDB.json"), b"{}").expect("write fixture");

        let args = args(
            None,
            false,
            None,
            Some(dir.path().to_str().expect("utf8 path")),
        );
        let steam_off = NetworkPolicy {
            fetch_steam_workshop: false,
            ..NetworkPolicy::default()
        };
        let paths = resolve_rimsort_paths(&args, &steam_off).expect("must resolve");
        assert!(paths.community_rules.is_some());
        assert_eq!(
            paths.steam_db, None,
            "a disabled source is skipped even when its cache file exists"
        );

        let defaults =
            resolve_rimsort_paths(&args, &NetworkPolicy::default()).expect("must resolve");
        assert!(
            defaults.steam_db.is_some(),
            "fetch_steam_workshop defaults to true"
        );
    }

    #[test]
    fn an_empty_cache_directory_imports_neither_community_nor_steam() {
        let dir = tempfile::tempdir().expect("tempdir");
        let args = args(
            None,
            true,
            None,
            Some(dir.path().to_str().expect("utf8 path")),
        );
        let paths = resolve_rimsort_paths(&args, &policy(true, true)).expect("must resolve");
        assert_eq!(paths.community_rules, None);
        assert_eq!(paths.steam_db, None);
    }

    /// This module's own remaining logic: the "nothing usable" warning.
    /// Can't assert on stderr text from a unit test cheaply, so this pins
    /// the condition it fires under instead — `resolve_rimsort_paths`
    /// must still return successfully (`Ok`) with both sources `None`
    /// rather than erroring.
    #[test]
    fn from_cache_with_an_empty_cache_directory_still_succeeds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let args = args(
            None,
            true,
            None,
            Some(dir.path().to_str().expect("utf8 path")),
        );
        let paths = resolve_rimsort_paths(&args, &policy(true, true));
        assert!(paths.is_ok());
    }
}
