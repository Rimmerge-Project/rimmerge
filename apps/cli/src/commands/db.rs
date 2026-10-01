//! `rimmerge db`: manual rule-database cache maintenance — `status` reads
//! the cache manifest with no network; `refresh` fetches whichever sources
//! are enabled, honouring both `NetworkPolicy::allow_network` (the hard
//! offline switch) and each source's own fetch toggle.
//!
//! Neither subcommand calls [`crate::common::build_session`]: that
//! performs a full mod scan, which is exactly what
//! [`rim_session::use_cases::RefreshRuleDatabases::execute`]'s own
//! `&NetworkPolicy` signature (rather than `&mut Session`) exists to
//! avoid paying for. The policy is app-global, loaded on its own through
//! [`crate::common::read_network_policy`].

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use rim_session::notifications::NotificationKind;
use rim_session::ports::{NotificationStateStore as _, RefreshOutcome, RuleDatabase, SkipReason};
use rim_session::use_cases::{RefreshRuleDatabases, RuleDatabaseView};

use crate::common::{
    PathsArgs, TerminalSafe, default_profile_base, print_recovered_notice_if_any,
    read_app_settings_load, read_network_policy, resolve_cache_dir, resolve_paths,
};

#[derive(Debug, Subcommand)]
pub enum DbCommand {
    /// Shows what's cached for each rule database: size, sha, how long
    /// ago it was fetched, or "never fetched". Reads no network.
    Status(DbStatusArgs),
    /// Fetches the community rules, Steam Workshop and/or rimmerge-rules
    /// databases from GitHub into the shared cache `import --from-cache`
    /// reads from.
    /// Prints `skipped (source disabled)` for a source whose own fetch
    /// toggle is off (`rimmerge network on --community-rules|
    /// --steam-workshop|--rimmerge-rules on`, or the desktop's Databases
    /// card), or `skipped (network refresh disabled)` for every source
    /// when `allow_network` is off (`rimmerge network on` to allow any
    /// refresh at all).
    Refresh(DbRefreshArgs),
}

/// Mirrors [`RuleDatabase`] for `clap`'s `ValueEnum` derive (which needs
/// `clap`, a dependency `rim-session` deliberately never takes) —
/// `common.rs`'s own `TieBreakArg` does the identical thing for
/// [`rim_resolve::sort::TieBreak`].
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum RuleDatabaseArg {
    /// The Community Rules Database (small; fetched by default).
    Community,
    /// The Steam Workshop dependency database (~49 MB; on by default, fetched only by an explicit refresh).
    Steam,
    /// This project's own rimmerge-rules file (small; fetched by
    /// default) — precedence rules, patch-operation behaviours,
    /// def-cache carriers, tag rules.
    Rimmerge,
}

impl From<RuleDatabaseArg> for RuleDatabase {
    fn from(value: RuleDatabaseArg) -> Self {
        match value {
            RuleDatabaseArg::Community => Self::CommunityRules,
            RuleDatabaseArg::Steam => Self::SteamWorkshop,
            RuleDatabaseArg::Rimmerge => Self::RimmergeRules,
        }
    }
}

#[derive(Debug, Args)]
pub struct DbStatusArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// Overrides where the rule-database cache lives (default:
    /// `%LOCALAPPDATA%\rimmerge\databases`, shared by every profile).
    /// Doesn't change what gets fetched or from where, only where the
    /// downloaded bytes are stored/read.
    #[arg(long)]
    cache_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DbRefreshArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// See `db status`'s own `--cache-dir`.
    #[arg(long)]
    cache_dir: Option<PathBuf>,
    /// Refresh only this source (`community`, `steam` or `rimmerge`),
    /// instead of every enabled one. Without it, every enabled source is
    /// fetched, including the Steam Workshop database (about 49 MB).
    #[arg(long, value_enum, conflicts_with = "all")]
    source: Option<RuleDatabaseArg>,
    /// Refresh every source explicitly. This is already the default when
    /// neither this nor `--source` is given.
    #[arg(long, conflicts_with = "source")]
    all: bool,
    /// Exit with a non-zero status if any requested source failed to
    /// refresh (e.g. the network is unreachable). Being offline or having
    /// a source turned off is never treated as a failure, with or without
    /// this flag — pass it only to catch a genuine fetch error in a
    /// script.
    #[arg(long)]
    strict: bool,
}

pub fn run(command: &DbCommand) -> anyhow::Result<()> {
    match command {
        DbCommand::Status(args) => run_status(args),
        DbCommand::Refresh(args) => run_refresh(args),
    }
}

/// Resolves just the profile directory — no scan, no
/// [`crate::common::build_session`] — which the needs-reimport badge
/// (`RefreshRuleDatabases::status`) needs to read this profile's own
/// import manifest. The network policy itself is app-global, read
/// separately through [`read_network_policy`].
fn profile_dir(paths: &PathsArgs) -> anyhow::Result<PathBuf> {
    Ok(resolve_paths(paths)?.profile_dir)
}

fn run_status(args: &DbStatusArgs) -> anyhow::Result<()> {
    let profile_dir = profile_dir(&args.paths)?;
    let load = read_app_settings_load();
    let app_settings = load.settings();
    let cache_dir = resolve_cache_dir(args.cache_dir.clone())?;
    let use_case = RefreshRuleDatabases::new(
        rim_io::GithubRuleDatabaseFetcher::new(),
        rim_io::JsonImportManifestStore::new(),
    );

    let now = jiff::Timestamp::now();
    let threshold = app_settings.reminders.rule_databases_stale_after_days;
    let views = use_case.status(&app_settings.network, &cache_dir, &profile_dir);
    for view in &views {
        println!(
            "{}",
            TerminalSafe::line(format_status_line(
                view,
                now,
                threshold,
                &app_settings.network
            ))
        );
    }
    let any_stale = any_enabled_source_stale(&views, now, threshold);

    if any_stale && app_settings.network.allow_network && !rule_databases_stale_is_muted() {
        println!(
            "Rule databases not refreshed in over {} days — run `rimmerge db refresh`",
            threshold.get()
        );
    }

    print_recovered_notice_if_any(&load);
    Ok(())
}

/// Whether the app-global "don't remind me again" mute covers
/// [`NotificationKind::RuleDatabasesStale`] — `db status`'s own trailing
/// hint line honours the identical mute the desktop bell does, read
/// straight off `<base>/notifications.json` (fails open on a missing or
/// corrupt file, per `rim_io::notifications`'s own doc comment: a muted
/// hint reappearing is a much smaller cost than one that silently never
/// shows again).
fn rule_databases_stale_is_muted() -> bool {
    let Ok(base) = default_profile_base() else {
        return false;
    };
    rim_io::JsonNotificationStateStore::new()
        .load(&base)
        .muted
        .contains(&NotificationKind::RuleDatabasesStale)
}

fn run_refresh(args: &DbRefreshArgs) -> anyhow::Result<()> {
    let policy = read_network_policy();
    let cache_dir = resolve_cache_dir(args.cache_dir.clone())?;
    let use_case = RefreshRuleDatabases::new(
        rim_io::GithubRuleDatabaseFetcher::new(),
        rim_io::JsonImportManifestStore::new(),
    );

    // `--all` and the no-flag default both mean "every source" —
    // `execute` itself decides which of those are actually enabled, so
    // `--all` needs no separate handling beyond conflicting with
    // `--source` in the arg parser above.
    let requested: Vec<RuleDatabase> = match args.source {
        Some(source) => vec![source.into()],
        None => vec![
            RuleDatabase::CommunityRules,
            RuleDatabase::SteamWorkshop,
            RuleDatabase::RimmergeRules,
        ],
    };

    let outcomes = use_case.execute(&policy, &cache_dir, &requested);
    for (database, outcome) in &outcomes {
        println!(
            "{}",
            TerminalSafe::line(format_refresh_line(*database, outcome))
        );
    }

    if refresh_has_failure(&outcomes) && args.strict {
        anyhow::bail!("one or more sources failed to refresh (--strict)");
    }
    Ok(())
}

/// This source's own name in every `db` line — deliberately the same
/// vocabulary `--source community|steam|rimmerge` uses, so a value printed here
/// round-trips back into that flag.
fn database_name(database: RuleDatabase) -> &'static str {
    match database {
        RuleDatabase::CommunityRules => "community",
        RuleDatabase::SteamWorkshop => "steam",
        RuleDatabase::RimmergeRules => "rimmerge",
    }
}

/// The first 12 characters of a sha256 digest — this repo's existing
/// hash-display convention (`rim_io::profile_dir`'s own doc comment).
///
/// `sha256` comes from `manifest.json`, a file a user can
/// hand-edit or that can end up corrupted, with no format validation
/// before this display path reads it — a byte-index slice (`&sha256[..n]`)
/// panics on a multibyte character straddling that boundary, exactly the
/// failure class the workspace's `unwrap`/`expect` denial exists to
/// prevent, reached here by a different route. `.chars().take(12)` only
/// ever counts whole characters, so it can never land mid-character.
fn sha12(sha256: &str) -> String {
    sha256.chars().take(12).collect()
}

/// One `db status` line — a pure function of `view`, the injected `now`,
/// and `network`, so it (and the staleness/refresh-mode it renders) is
/// exhaustively unit tested with no clock or filesystem involved.
///
/// The leading `automatic`/`manual` label mirrors the desktop
/// Databases card's own per-row label
/// (`apps/desktop/src/utils/ruleDatabases.ts`'s `isAutoRefreshEligible`):
/// a source refreshes automatically only when the app-global toggle is
/// on *and* the source itself is eligible
/// ([`RuleDatabase::is_auto_refresh_eligible`] — never the Steam
/// Workshop database, regardless of the toggle).
///
/// A trailing `[needs reimport]` marker appears when
/// [`RuleDatabaseView::needs_reimport`] is true — the same signal the
/// desktop's Databases card shows as an inline "Re-import" hint, so a
/// CLI-only user gets the identical signal a desktop user does, from data
/// `status` already reads.
///
/// For [`RuleDatabase::RimmergeRules`] specifically, a trailing
/// `[bundled: <sha12>]` also names the embedded snapshot's own identity
/// (`rim_io::embedded_bundle_sha256`, hashed once at compile time) —
/// shown unconditionally, since this source always has a bundled
/// fallback even when nothing has ever been fetched, unlike the other
/// two sources' plain "never fetched".
/// Whether `db status`'s trailing "not refreshed in over N days" hint
/// should print — true only for a source that is both **enabled** and
/// stale. A disabled source with an old cached copy from before it was
/// turned off is not something the hint's own `rimmerge db refresh`
/// suggestion could ever fix (a disabled source is never refreshed), so
/// it must never trip the hint on its own.
fn any_enabled_source_stale(
    views: &[RuleDatabaseView],
    now: jiff::Timestamp,
    threshold: rim_session::app_settings::StaleAfterDays,
) -> bool {
    views
        .iter()
        .any(|view| view.status.enabled && view.status.is_stale(now, threshold))
}

fn format_status_line(
    view: &RuleDatabaseView,
    now: jiff::Timestamp,
    threshold: rim_session::StaleAfterDays,
    network: &rim_session::NetworkPolicy,
) -> String {
    let status = &view.status;
    let name = database_name(status.database);
    let enabled = if status.enabled {
        "enabled"
    } else {
        "disabled"
    };
    let refresh_mode =
        if network.auto_refresh_rule_databases && status.database.is_auto_refresh_eligible() {
            "automatic"
        } else {
            "manual"
        };
    let mut line = match &status.cached {
        None => format!("{name}: {enabled}, {refresh_mode}, never fetched"),
        Some(cached) => {
            let stale = if status.is_stale(now, threshold) {
                " [stale]"
            } else {
                ""
            };
            format!(
                "{name}: {enabled}, {refresh_mode}, {} ({} bytes, fetched {}){stale}",
                sha12(&cached.sha256),
                cached.bytes,
                cached.fetched_at
            )
        }
    };
    if let Some(failure) = &status.last_failure {
        line.push_str(&format!(", last refresh failed: {}", failure.detail));
    }
    if status.database == RuleDatabase::RimmergeRules {
        line.push_str(&format!(
            " [bundled: {}]",
            sha12(rim_io::embedded_bundle_sha256())
        ));
    }
    if view.needs_reimport {
        line.push_str(" [needs reimport]");
    }
    line
}

/// One `db refresh` line. The literal fragments (`updated`/`unchanged`/
/// `failed:`/`skipped (source disabled)`/`skipped (network refresh
/// disabled)`/`skipped (automatic refresh disabled)`/`skipped (not due
/// yet)`) are fixed wording, verbatim — the entire reason [`SkipReason`]
/// has four variants is that a user needs to know which switch to flip
/// (or that there's nothing to flip at all), so each skip line must stay
/// distinguishable text, not just distinguishable data.
fn format_refresh_line(database: RuleDatabase, outcome: &RefreshOutcome) -> String {
    let name = database_name(database);
    match outcome {
        RefreshOutcome::Updated { sha256, bytes } => {
            format!("{name}: updated ({bytes} bytes, sha {})", sha12(sha256))
        }
        RefreshOutcome::Unchanged { sha256 } => {
            format!("{name}: unchanged (sha {})", sha12(sha256))
        }
        RefreshOutcome::Failed { failure } => format!("{name}: failed: {}", failure.detail),
        RefreshOutcome::Skipped {
            reason: SkipReason::SourceDisabled,
        } => format!("{name}: skipped (source disabled)"),
        RefreshOutcome::Skipped {
            reason: SkipReason::NetworkDisabled,
        } => format!("{name}: skipped (network refresh disabled)"),
        RefreshOutcome::Skipped {
            reason: SkipReason::AutoRefreshDisabled,
        } => format!("{name}: skipped (automatic refresh disabled)"),
        RefreshOutcome::Skipped {
            reason: SkipReason::NotDue,
        } => format!("{name}: skipped (not due yet)"),
    }
}

/// Whether any requested source's outcome was [`RefreshOutcome::Failed`]
/// — a pure function of the outcomes alone, so `--strict`'s exit-code
/// contract is unit-tested without a socket or a process exit code in
/// sight. A [`RefreshOutcome::Skipped`] is never a failure, `--strict` or
/// not: being offline or having a source switched off is not a CLI error.
fn refresh_has_failure(outcomes: &[(RuleDatabase, RefreshOutcome)]) -> bool {
    outcomes
        .iter()
        .any(|(_, outcome)| matches!(outcome, RefreshOutcome::Failed { .. }))
}

#[cfg(test)]
mod tests {
    use rim_session::ports::{CachedDatabase, DatabaseStatus, FetchFailure};

    use super::*;

    fn cached(sha256: &str, bytes: usize, fetched_at: jiff::Timestamp) -> CachedDatabase {
        CachedDatabase {
            sha256: sha256.to_string(),
            bytes,
            fetched_at,
        }
    }

    fn status(
        database: RuleDatabase,
        enabled: bool,
        cached: Option<CachedDatabase>,
        last_failure: Option<&str>,
    ) -> DatabaseStatus {
        DatabaseStatus {
            database,
            enabled,
            path: PathBuf::from("communityRules.json"),
            cached,
            last_failure: last_failure.map(FetchFailure::unclassified),
            last_attempt_at: None,
        }
    }

    /// Wraps a bare [`DatabaseStatus`] into the [`RuleDatabaseView`]
    /// `format_status_line` actually takes, with `needs_reimport: false` —
    /// every `format_status_line` test below except the two dedicated to
    /// that marker doesn't care about it.
    fn view(status: DatabaseStatus) -> RuleDatabaseView {
        RuleDatabaseView {
            status,
            imported_sha256: None,
            needs_reimport: false,
        }
    }

    #[test]
    fn database_arg_maps_to_the_matching_rule_database() {
        assert_eq!(
            RuleDatabase::from(RuleDatabaseArg::Community),
            RuleDatabase::CommunityRules
        );
        assert_eq!(
            RuleDatabase::from(RuleDatabaseArg::Steam),
            RuleDatabase::SteamWorkshop
        );
        assert_eq!(
            RuleDatabase::from(RuleDatabaseArg::Rimmerge),
            RuleDatabase::RimmergeRules
        );
    }

    #[test]
    fn sha12_takes_the_first_12_hex_characters() {
        assert_eq!(sha12("b00fa152965643381ecedcf9"), "b00fa1529656");
    }

    #[test]
    fn sha12_does_not_panic_on_a_short_string() {
        assert_eq!(sha12("ab"), "ab");
    }

    /// A multibyte character straddling byte offset 12 would panic a
    /// plain `&sha256[..12]` byte-index slice — this never even reaches
    /// such a boundary, since `.chars().take(12)` counts whole characters.
    /// A hand-edited/corrupted `manifest.json` is exactly the
    /// untrusted-enough input this pins against.
    #[test]
    fn sha12_never_panics_on_a_multibyte_character_near_the_boundary() {
        let sha256 = "aaaaaaaaaaaé aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert_eq!(sha12(sha256), "aaaaaaaaaaaé");
    }

    // -- `format_status_line`: every reachable state --

    #[test]
    fn status_line_never_fetched() {
        let now = jiff::Timestamp::now();
        let line = format_status_line(
            &view(status(RuleDatabase::CommunityRules, true, None, None)),
            now,
            rim_session::StaleAfterDays::default(),
            &rim_session::NetworkPolicy::default(),
        );
        assert_eq!(line, "community: enabled, automatic, never fetched");
    }

    #[test]
    fn status_line_disabled_and_never_fetched() {
        let now = jiff::Timestamp::now();
        let line = format_status_line(
            &view(status(RuleDatabase::SteamWorkshop, false, None, None)),
            now,
            rim_session::StaleAfterDays::default(),
            &rim_session::NetworkPolicy::default(),
        );
        // Steam is never auto-refresh eligible, regardless of the
        // app-global toggle — see `RuleDatabase::is_auto_refresh_eligible`.
        assert_eq!(line, "steam: disabled, manual, never fetched");
    }

    /// The auto-refresh label follows both the app-global toggle *and*
    /// each source's own eligibility — never one alone.
    #[test]
    fn status_line_labels_manual_when_the_auto_refresh_toggle_is_off() {
        let now = jiff::Timestamp::now();
        let network = rim_session::NetworkPolicy {
            auto_refresh_rule_databases: false,
            ..rim_session::NetworkPolicy::default()
        };
        let line = format_status_line(
            &view(status(RuleDatabase::CommunityRules, true, None, None)),
            now,
            rim_session::StaleAfterDays::default(),
            &network,
        );
        assert_eq!(line, "community: enabled, manual, never fetched");
    }

    /// `RuleDatabase::RimmergeRules` carries a trailing `[bundled: ...]`
    /// marker naming the embedded snapshot's identity — the other two
    /// sources never do, since they have no compiled-in fallback of
    /// their own.
    #[test]
    fn status_line_for_rimmerge_rules_names_the_bundled_snapshot() {
        let now = jiff::Timestamp::now();
        let line = format_status_line(
            &view(status(RuleDatabase::RimmergeRules, true, None, None)),
            now,
            rim_session::StaleAfterDays::default(),
            &rim_session::NetworkPolicy::default(),
        );
        let expected_sha12 = &rim_io::embedded_bundle_sha256()[..12];
        assert_eq!(
            line,
            format!("rimmerge: enabled, automatic, never fetched [bundled: {expected_sha12}]")
        );
    }

    #[test]
    fn status_line_for_other_sources_never_carries_the_bundled_marker() {
        let now = jiff::Timestamp::now();
        let line = format_status_line(
            &view(status(RuleDatabase::CommunityRules, true, None, None)),
            now,
            rim_session::StaleAfterDays::default(),
            &rim_session::NetworkPolicy::default(),
        );
        assert!(!line.contains("[bundled:"), "{line}");
    }

    #[test]
    fn status_line_cached_and_fresh() {
        let now = jiff::Timestamp::UNIX_EPOCH + jiff::Span::new().hours(24);
        let fetched_at = jiff::Timestamp::UNIX_EPOCH;
        let line = format_status_line(
            &view(status(
                RuleDatabase::CommunityRules,
                true,
                Some(cached("b00fa152965643381ecedcf9", 393_897, fetched_at)),
                None,
            )),
            now,
            rim_session::StaleAfterDays::default(),
            &rim_session::NetworkPolicy::default(),
        );
        assert_eq!(
            line,
            format!(
                "community: enabled, automatic, b00fa1529656 (393897 bytes, fetched {fetched_at})"
            )
        );
        assert!(!line.contains("[stale]"));
    }

    #[test]
    fn status_line_cached_and_stale() {
        let fetched_at = jiff::Timestamp::UNIX_EPOCH;
        let now = fetched_at + jiff::Span::new().hours(31 * 24);
        let line = format_status_line(
            &view(status(
                RuleDatabase::CommunityRules,
                true,
                Some(cached("abc123", 10, fetched_at)),
                None,
            )),
            now,
            rim_session::StaleAfterDays::default(),
            &rim_session::NetworkPolicy::default(),
        );
        assert!(line.contains("[stale]"), "{line}");
    }

    #[test]
    fn any_enabled_source_stale_ignores_a_disabled_sources_stale_cache() {
        let fetched_at = jiff::Timestamp::UNIX_EPOCH;
        let now = fetched_at + jiff::Span::new().hours(31 * 24);
        let threshold = rim_session::StaleAfterDays::default();
        let disabled_but_stale = view(status(
            RuleDatabase::SteamWorkshop,
            false,
            Some(cached("abc123", 10, fetched_at)),
            None,
        ));

        assert!(
            !any_enabled_source_stale(&[disabled_but_stale], now, threshold),
            "a disabled source's own leftover cache must never trip the \
             'run `rimmerge db refresh`' hint — refreshing it wouldn't be honoured anyway"
        );
    }

    #[test]
    fn any_enabled_source_stale_fires_for_an_enabled_stale_source() {
        let fetched_at = jiff::Timestamp::UNIX_EPOCH;
        let now = fetched_at + jiff::Span::new().hours(31 * 24);
        let threshold = rim_session::StaleAfterDays::default();
        let enabled_and_stale = view(status(
            RuleDatabase::CommunityRules,
            true,
            Some(cached("abc123", 10, fetched_at)),
            None,
        ));

        assert!(any_enabled_source_stale(
            &[enabled_and_stale],
            now,
            threshold
        ));
    }

    #[test]
    fn status_line_cached_with_a_last_failure_still_shows_the_cached_facts() {
        let now = jiff::Timestamp::UNIX_EPOCH;
        let line = format_status_line(
            &view(status(
                RuleDatabase::CommunityRules,
                true,
                Some(cached("abc123", 10, now)),
                Some("connection timed out"),
            )),
            now,
            rim_session::StaleAfterDays::default(),
            &rim_session::NetworkPolicy::default(),
        );
        assert!(line.contains("abc123"), "{line}");
        assert!(
            line.contains("last refresh failed: connection timed out"),
            "{line}"
        );
    }

    #[test]
    fn status_line_never_fetched_with_a_last_failure() {
        let now = jiff::Timestamp::now();
        let line = format_status_line(
            &view(status(
                RuleDatabase::SteamWorkshop,
                true,
                None,
                Some("dns error"),
            )),
            now,
            rim_session::StaleAfterDays::default(),
            &rim_session::NetworkPolicy::default(),
        );
        assert_eq!(
            line,
            "steam: enabled, manual, never fetched, last refresh failed: dns error"
        );
    }

    #[test]
    fn status_line_needing_reimport_carries_the_marker() {
        let now = jiff::Timestamp::UNIX_EPOCH;
        let mut view = view(status(
            RuleDatabase::CommunityRules,
            true,
            Some(cached("abc123", 10, now)),
            None,
        ));
        view.needs_reimport = true;
        let line = format_status_line(
            &view,
            now,
            rim_session::StaleAfterDays::default(),
            &rim_session::NetworkPolicy::default(),
        );
        assert!(line.ends_with("[needs reimport]"), "{line}");
    }

    #[test]
    fn status_line_up_to_date_never_carries_the_reimport_marker() {
        let now = jiff::Timestamp::UNIX_EPOCH;
        let line = format_status_line(
            &view(status(
                RuleDatabase::CommunityRules,
                true,
                Some(cached("abc123", 10, now)),
                None,
            )),
            now,
            rim_session::StaleAfterDays::default(),
            &rim_session::NetworkPolicy::default(),
        );
        assert!(!line.contains("needs reimport"), "{line}");
    }

    // -- `format_refresh_line`: all five outcome states --

    #[test]
    fn refresh_line_updated() {
        let line = format_refresh_line(
            RuleDatabase::CommunityRules,
            &RefreshOutcome::Updated {
                sha256: "b00fa152965643381ecedcf9".to_string(),
                bytes: 393_897,
            },
        );
        assert_eq!(line, "community: updated (393897 bytes, sha b00fa1529656)");
    }

    #[test]
    fn refresh_line_unchanged() {
        let line = format_refresh_line(
            RuleDatabase::SteamWorkshop,
            &RefreshOutcome::Unchanged {
                sha256: "abc123def456abc123def456".to_string(),
            },
        );
        assert_eq!(line, "steam: unchanged (sha abc123def456)");
    }

    #[test]
    fn refresh_line_failed() {
        let line = format_refresh_line(
            RuleDatabase::CommunityRules,
            &RefreshOutcome::Failed {
                failure: FetchFailure::unclassified("connection timed out"),
            },
        );
        assert_eq!(line, "community: failed: connection timed out");
    }

    #[test]
    fn refresh_line_skipped_source_disabled() {
        let line = format_refresh_line(
            RuleDatabase::SteamWorkshop,
            &RefreshOutcome::Skipped {
                reason: SkipReason::SourceDisabled,
            },
        );
        assert_eq!(line, "steam: skipped (source disabled)");
    }

    #[test]
    fn refresh_line_skipped_network_disabled() {
        let line = format_refresh_line(
            RuleDatabase::CommunityRules,
            &RefreshOutcome::Skipped {
                reason: SkipReason::NetworkDisabled,
            },
        );
        assert_eq!(line, "community: skipped (network refresh disabled)");
    }

    #[test]
    fn every_skip_reason_prints_distinguishable_text() {
        let lines: Vec<String> = [
            SkipReason::SourceDisabled,
            SkipReason::NetworkDisabled,
            SkipReason::AutoRefreshDisabled,
            SkipReason::NotDue,
        ]
        .into_iter()
        .map(|reason| {
            format_refresh_line(
                RuleDatabase::CommunityRules,
                &RefreshOutcome::Skipped { reason },
            )
        })
        .collect();

        for (index, line) in lines.iter().enumerate() {
            for other in &lines[index + 1..] {
                assert_ne!(
                    line, other,
                    "each SkipReason must render its own distinguishable text"
                );
            }
        }
    }

    // -- `refresh_has_failure` / `--strict` exit-code contract --

    #[test]
    fn no_failure_among_updated_unchanged_and_skipped() {
        let outcomes = vec![
            (
                RuleDatabase::CommunityRules,
                RefreshOutcome::Updated {
                    sha256: "a".to_string(),
                    bytes: 1,
                },
            ),
            (
                RuleDatabase::SteamWorkshop,
                RefreshOutcome::Skipped {
                    reason: SkipReason::SourceDisabled,
                },
            ),
        ];
        assert!(!refresh_has_failure(&outcomes));
    }

    #[test]
    fn a_failed_outcome_is_a_failure_regardless_of_the_others() {
        let outcomes = vec![
            (
                RuleDatabase::CommunityRules,
                RefreshOutcome::Unchanged {
                    sha256: "a".to_string(),
                },
            ),
            (
                RuleDatabase::SteamWorkshop,
                RefreshOutcome::Failed {
                    failure: FetchFailure::unclassified("boom"),
                },
            ),
        ];
        assert!(refresh_has_failure(&outcomes));
    }

    #[test]
    fn skipped_is_never_a_failure_even_though_strict_would_still_exit_zero() {
        let outcomes = vec![(
            RuleDatabase::CommunityRules,
            RefreshOutcome::Skipped {
                reason: SkipReason::NetworkDisabled,
            },
        )];
        // `refresh_has_failure` alone decides this; `--strict` only
        // changes what happens once it's `true` (see `run_refresh`).
        assert!(!refresh_has_failure(&outcomes));
    }
}
