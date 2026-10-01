//! Shared CLI plumbing: resolving project paths, building a real
//! [`rim_session::Session`] via `rim-io` adapters, and the small
//! JSON-in-only helpers `sort`/`ledger`/`fixture` use to derive tagging
//! from a bare [`rim_analyzer::domain::Report`] (no live scan data).

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Args, ValueEnum};
use rim_analyzer::domain::Report;
use rim_resolve::domain::{OrderSource, RuleSet, Tagging};
use rim_resolve::sort::TieBreak;
use rim_session::{ProjectPaths, Session};

/// Mirrors [`TieBreak`] for `clap`'s `ValueEnum` derive (which needs
/// `clap`, a dependency `rim-resolve` deliberately never takes). Shared by
/// `sort`/`ledger`'s own `--tie-break` flag.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TieBreakArg {
    /// See [`TieBreak::Rebuild`].
    Rebuild,
    /// See [`TieBreak::PreserveCurrent`].
    PreserveCurrent,
}

impl From<TieBreakArg> for TieBreak {
    fn from(value: TieBreakArg) -> Self {
        match value {
            TieBreakArg::Rebuild => Self::Rebuild,
            TieBreakArg::PreserveCurrent => Self::PreserveCurrent,
        }
    }
}

/// `--game-dir`/`--workshop-dir`/`--mods-config`/`--profile-dir`, shared
/// by every command that needs a real, live project (`load`/`import`/
/// `apply`) — `sort`/`ledger`/`fixture trim` are JSON-in only and never
/// take these.
#[derive(Debug, Args)]
pub struct PathsArgs {
    /// RimWorld install directory (contains `Data/` and `Mods/`).
    #[arg(long)]
    pub game_dir: Option<PathBuf>,
    /// Steam workshop content folder for RimWorld (app id `294100`).
    #[arg(long)]
    pub workshop_dir: Option<PathBuf>,
    /// Path to `ModsConfig.xml`.
    #[arg(long)]
    pub mods_config: Option<PathBuf>,
    /// This project's profile directory. Defaults to
    /// `%LOCALAPPDATA%\rimmerge\profiles\<hash of the ModsConfig.xml path>`.
    #[arg(long)]
    pub profile_dir: Option<PathBuf>,
}

/// Where `config.json`, `profiles/` and `databases/` live:
/// `RIMMERGE_PROFILE_DIR` when set, else `%LOCALAPPDATA%\rimmerge`.
///
/// The variable is honoured here for the same reason the desktop's own
/// `profile_base()` honours it (that function's doc comment has the
/// original rationale): the two interfaces read and write the *same*
/// files, so a redirect that moved only one of them would put a
/// `config.json` somewhere the other never looks — and a test or smoke
/// run pointing the desktop at a scratch base while the CLI still wrote
/// to the real one is exactly the asymmetry that produces "it worked
/// when I ran it the other way".
pub(crate) fn default_profile_base() -> anyhow::Result<PathBuf> {
    if let Some(base) = rim_analyzer::infra::paths::path_var("RIMMERGE_PROFILE_DIR") {
        return Ok(base);
    }
    let local_app_data =
        std::env::var("LOCALAPPDATA").context("LOCALAPPDATA environment variable is not set")?;
    Ok(PathBuf::from(local_app_data).join("rimmerge"))
}

/// Reads the app-global [`rim_session::NetworkPolicy`] through
/// [`rim_io::JsonAppSettingsStore`], against [`default_profile_base`].
/// **Never fails outright**: a base that fails to resolve (no
/// `LOCALAPPDATA`) reads as [`rim_session::AppSettings::default`], the
/// same "degrade to shipped behaviour" rule [`build_session`]'s own
/// mod-knowledge fallback follows — see [`AppSettingsLoad::settings`]
/// for the missing/corrupt/recovered cases this then also covers.
pub(crate) fn read_network_policy() -> rim_session::NetworkPolicy {
    read_app_settings().network
}

/// The app-global [`rim_session::AppSettings`] in full — [`read_network_policy`]'s
/// own superset, for callers that also need `reminders` (e.g. `db
/// status`'s own stale threshold). Same "never fails outright" contract:
/// a base that fails to resolve reads as
/// [`rim_session::AppSettings::default`].
pub(crate) fn read_app_settings() -> rim_session::AppSettings {
    read_app_settings_load().settings()
}

/// [`read_app_settings`]'s own superset: the raw
/// [`rim_session::ports::AppSettingsLoad`], for a caller that needs to
/// tell a [`rim_session::ports::AppSettingsLoad::Recovered`] file apart
/// from an ordinary one (`db status`'s and `network status`'s own
/// failed-closed line). Same "never fails outright" contract as
/// [`read_app_settings`]: a base that fails to resolve reads as
/// [`rim_session::ports::AppSettingsLoad::Missing`], which is exactly
/// what a fresh machine with no `LOCALAPPDATA` should show.
pub(crate) fn read_app_settings_load() -> rim_session::ports::AppSettingsLoad {
    use rim_session::ports::AppSettingsStore;
    let Ok(base) = default_profile_base() else {
        return rim_session::ports::AppSettingsLoad::Missing;
    };
    rim_io::JsonAppSettingsStore::new().load(&base)
}

/// The one line every CLI surface prints when `app-settings.json`
/// failed closed (`AppSettingsLoad::Recovered`) — shared by `db status`
/// and `network status` so the wording can't drift between the two.
/// Prints nothing for [`rim_session::ports::AppSettingsLoad::Loaded`]/
/// [`rim_session::ports::AppSettingsLoad::Missing`].
pub(crate) fn print_recovered_notice_if_any(load: &rim_session::ports::AppSettingsLoad) {
    if let rim_session::ports::AppSettingsLoad::Recovered { reason } = load {
        println!(
            "app-settings.json couldn't be read ({}) — internet access is off until you \
             save settings again",
            TerminalSafe::line(reason)
        );
    }
}

/// Resolves the rule-database cache directory: `explicit` when
/// given (every `db`/`import` command's own `--cache-dir` override, needed
/// so tests can seed a temp cache without touching the real profile base or
/// the network), else `rim_io::databases_dir` under the same
/// `%LOCALAPPDATA%\rimmerge` base [`resolve_paths`] uses for `profile_dir`.
pub(crate) fn resolve_cache_dir(explicit: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match explicit {
        Some(dir) => Ok(dir),
        None => Ok(rim_io::databases_dir(&default_profile_base()?)),
    }
}

/// Resolves `args` into a full [`ProjectPaths`] through
/// [`rim_io::resolve_project_paths`] — the single implementation of the
/// precedence ladder,
/// shared verbatim with the desktop's `get_default_paths`. This function
/// only turns `clap`'s own flags into that function's
/// [`rim_io::PathOverrides`]; every default, every environment variable
/// and `config.json` itself are the ladder's business, not the CLI's.
/// A non-fatal complaint about the resolved paths (a `--game-dir` that
/// isn't an install) goes to stderr, not into the returned value: every
/// caller is about to scan, and a scan that then fails says *why* far
/// better than this could. Printing it first is what makes the eventual
/// failure legible rather than mysterious.
pub fn resolve_paths(args: &PathsArgs) -> anyhow::Result<ProjectPaths> {
    let resolved = rim_io::resolve_project_paths(overrides_from(args, default_profile_base()?))?;
    if let Some(warning) = &resolved.warning {
        eprintln!("warning: {}", TerminalSafe::line(warning));
    }
    Ok(resolved.paths)
}

/// `args` as [`rim_io::PathOverrides`]' rung 1, against `base`.
pub(crate) fn overrides_from(args: &PathsArgs, base: PathBuf) -> rim_io::PathOverrides {
    rim_io::PathOverrides {
        base,
        game_dir: args.game_dir.clone(),
        workshop_dir: args.workshop_dir.clone(),
        mods_config: args.mods_config.clone(),
        profile_dir: args.profile_dir.clone(),
    }
}

/// Scans, analyzes, and loads persisted decisions/rules into a fresh
/// [`Session`], printing a single self-overwriting progress line to
/// stderr as it goes (real per-mod ticks during the scanning stage, via
/// [`rim_io::AnalyzerScanner`]'s use of
/// `rim_analyzer::infra::scan_with_progress`).
pub fn build_session(paths: ProjectPaths) -> anyhow::Result<Session> {
    let use_case = rim_session::use_cases::LoadProject::new(
        rim_io::AnalyzerScanner::new(),
        rim_io::ModsConfigFileStore::new(),
        rim_io::JsonDecisionStore::new(),
        rim_io::JsonRuleStore::new(),
        rim_io::JsonPatchProjectStore::new(),
        rim_io::JsonAssignmentProjectStore::new(),
        // The mod-knowledge store reads the same app-global cache
        // directory `db refresh` writes into. `vendored()` on a machine
        // with no resolvable profile base: the vendored defaults are
        // always available, so a missing `%LOCALAPPDATA%` degrades to
        // "shipped behaviour", never to a failed load.
        resolve_cache_dir(None).map_or_else(
            |_| rim_io::FsModKnowledgeStore::vendored(),
            rim_io::FsModKnowledgeStore::new,
        ),
        read_network_policy().fetch_rimmerge_rules,
    );
    let mut session = use_case
        .execute(paths, &mut |progress| {
            eprint!(
                "\r[{:>4}/{:<4}] {:<24}",
                progress.done,
                progress.total,
                progress.stage.english_label()
            );
            let _ = std::io::stderr().flush();
        })
        .context("loading the project")?;
    eprintln!();
    for warning in session.rule_load_warnings() {
        eprintln!("warning: {}", TerminalSafe::line(warning));
    }
    // Gives the session a real
    // `DefSourceReader` so `Session::redecide_clean_merge_at`'s lazy
    // clean-merge preview pass can compute a missing preview instead of
    // only reusing whatever's already cached.
    session.set_def_source_reader(std::sync::Arc::new(rim_io::FileDefSourceReader::new()));
    Ok(session)
}

/// Builds the rules/tagging a JSON-in (`--report`-only) command sorts or
/// evaluates with: an empty [`RuleSet`] (`sort`/`ledger` have no live
/// profile to load pair/placement/tag rules from — the two import
/// toggles, `--use-imported-pairs`/`--no-imported-placements`, are
/// therefore inert here, filtering a set that's already empty; they
/// exist on these commands for parity with `Settings`/`apply`'s live
/// session, and stop being a no-op the moment either command gains a way
/// to load a real rules file) plus real tag inference from the report's
/// own evidence (there is no shipped default tag rule, so an empty
/// `tag_rules`/`manual_tags` list yields an empty [`Tagging`], but through
/// the real pipeline rather than a hardcoded default).
#[must_use]
pub fn build_rules_and_tagging(report: &Report) -> (RuleSet, Tagging) {
    let evidence = rim_resolve::tags::evidence_from_report(report);
    let tagging = rim_resolve::tags::infer_tags(&evidence, &[], &[]);
    (RuleSet::default(), tagging)
}

/// Reads and parses a [`Report`] from `path`.
pub fn read_report(path: &Path) -> anyhow::Result<Report> {
    let bytes =
        std::fs::read(path).with_context(|| format!("reading report at {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing report JSON at {}", path.display()))
}

/// Prints `value` with every control character removed, so text that came
/// from a mod, a report or a log can never inject an escape sequence into
/// the user's terminal. This is the one terminal-safe output boundary of the
/// CLI: every text-mode print of a string that is not a compile-time
/// constant of this crate goes through it. `--json` output needs no
/// wrapper, since serde escapes every control character.
///
/// Removes every `char::is_control` character (C0 including ESC, DEL and C1
/// such as U+009B) except `\t`; [`TerminalSafe::line`] also drops `\n`, so a
/// single-line field can never be split into forged extra lines, while
/// [`TerminalSafe::block`] keeps the newlines of genuinely multi-line text.
/// Wraps any [`std::fmt::Display`] value, a `ModId` or a path included.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TerminalSafe<T> {
    value: T,
    keeps_newlines: bool,
}

impl<T: std::fmt::Display> TerminalSafe<T> {
    /// A single-line field: tabs survive, newlines do not.
    pub(crate) fn line(value: T) -> Self {
        Self {
            value,
            keeps_newlines: false,
        }
    }

    /// Multi-line text (a mod description): tabs and newlines survive.
    pub(crate) fn block(value: T) -> Self {
        Self {
            value,
            keeps_newlines: true,
        }
    }
}

impl<T: std::fmt::Display> std::fmt::Display for TerminalSafe<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write as _;

        struct Filter<'a, 'b> {
            sink: &'a mut std::fmt::Formatter<'b>,
            keeps_newlines: bool,
        }
        impl std::fmt::Write for Filter<'_, '_> {
            fn write_str(&mut self, text: &str) -> std::fmt::Result {
                text.chars()
                    .filter(|c| {
                        !c.is_control() || *c == '\t' || (self.keeps_newlines && *c == '\n')
                    })
                    .try_for_each(|c| self.sink.write_char(c))
            }
        }
        write!(
            Filter {
                sink: formatter,
                keeps_newlines: self.keeps_newlines,
            },
            "{}",
            self.value
        )
    }
}

/// An order's name as the text outputs print it (`Current`, `Suggested`),
/// a stable spelling that does not ride on the type's derived `Debug`.
#[must_use]
pub(crate) fn format_order_source(source: OrderSource) -> &'static str {
    match source {
        OrderSource::Current => "Current",
        OrderSource::Suggested => "Suggested",
    }
}

/// A tie-break mode's name as `apply --dry-run` prints it (`Rebuild`,
/// `PreserveCurrent`).
#[must_use]
pub(crate) fn format_tie_break(tie_break: TieBreak) -> &'static str {
    match tie_break {
        TieBreak::Rebuild => "Rebuild",
        TieBreak::PreserveCurrent => "PreserveCurrent",
    }
}

/// `= unchanged`, `+N` moved N slots earlier, `-N` moved N slots later,
/// `NEW` absent from the comparison order — the position-change marker
/// shared by `sort`'s suggested-order listing and `apply --dry-run`'s
/// diff.
#[must_use]
pub fn position_marker(new_position: usize, old_position: Option<usize>) -> String {
    match old_position {
        None => "NEW".to_string(),
        Some(old) if old == new_position => "=".to_string(),
        Some(old) if old > new_position => format!("+{}", old - new_position),
        Some(old) => format!("-{}", new_position - old),
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalSafe;

    #[test]
    fn line_drops_escape_c1_and_newline_but_keeps_tab() {
        let hostile = "a\u{1b}[31mb\u{9b}c\nd\te\u{7f}f";
        assert_eq!(TerminalSafe::line(hostile).to_string(), "a[31mbcd\tef");
    }

    #[test]
    fn block_keeps_newlines_and_tabs_only() {
        let hostile = "a\u{1b}b\nc\td\r\u{9b}";
        assert_eq!(TerminalSafe::block(hostile).to_string(), "ab\nc\td");
    }

    #[test]
    fn wraps_any_display_value() {
        assert_eq!(TerminalSafe::line(42).to_string(), "42");
    }
}
