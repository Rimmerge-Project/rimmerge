//! `rimmerge apply`: writes the selected order to `ModsConfig.xml` (with a
//! backup) and saves decisions/rules — or, with `--dry-run`, only prints
//! what would change.

use anyhow::{Context, bail};
use clap::{Args, ValueEnum};
use rim_io::GameProcessProbe;
use rim_resolve::domain::OrderSource;
use rim_resolve::preflight::{Availability, HardProblem, MissingModOutcome};
use rim_session::use_cases::{Apply, ApplyOptions, ApplyPreflight, PreflightApply};

use crate::common::{
    PathsArgs, TerminalSafe, build_session, format_order_source, format_tie_break, position_marker,
    resolve_paths,
};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SourceArg {
    Current,
    Suggested,
}

impl From<SourceArg> for OrderSource {
    fn from(value: SourceArg) -> Self {
        match value {
            SourceArg::Current => OrderSource::Current,
            SourceArg::Suggested => OrderSource::Suggested,
        }
    }
}

#[derive(Debug, Args)]
pub struct ApplyArgs {
    #[command(flatten)]
    paths: PathsArgs,
    /// Print the `ModsConfig.xml` diff without writing anything.
    #[arg(long)]
    dry_run: bool,
    /// Which order to apply.
    #[arg(long, value_enum, default_value_t = SourceArg::Suggested)]
    source: SourceArg,
    /// Write `ModsConfig.xml` even if `RimWorldWin64.exe` looks like it's
    /// running.
    #[arg(long)]
    force: bool,
    /// Also render the generated merge mod into the game's `Mods/`
    /// folder (or remove it when no `Merge`/`ShipAsset` decision
    /// remains), atomically.
    #[arg(long)]
    write_merge_mod: bool,
}

pub fn run(args: &ApplyArgs) -> anyhow::Result<()> {
    run_with_probe(args, &rim_io::SysinfoGameProcessProbe::new())
}

/// Shares the exact same process-probe trait `apps/desktop` refuses a
/// write against ([`rim_io::GameProcessProbe`]), so "is the game running"
/// means one thing across every interface, and so this can be exercised
/// with a fake probe in a unit test rather than needing a real
/// `RimWorldWin64.exe` process on the test machine.
fn run_with_probe(args: &ApplyArgs, probe: &dyn GameProcessProbe) -> anyhow::Result<()> {
    let project_paths = resolve_paths(&args.paths)?;
    let mods_config_path = project_paths.mods_config.clone();
    let mut session = build_session(project_paths)?;
    session.select(args.source.into());
    print_preflight(&PreflightApply::new().execute(&mut session, args.source.into()));

    if args.dry_run {
        print_dry_run_diff(&session, &mods_config_path)?;
        return Ok(());
    }

    if !args.force && probe.is_running() {
        bail!(
            "RimWorldWin64.exe is running; close the game or retry with --force (RimWorld overwrites ModsConfig.xml on exit, so a concurrent write is likely to be lost or corrupt the running game's own copy)"
        );
    }

    let use_case = Apply::new(
        rim_io::ModsConfigFileStore::new(),
        rim_io::JsonDecisionStore::new(),
        rim_io::JsonRuleStore::new(),
        rim_io::MergeModFolderWriter::new(),
        rim_io::FileDefSourceReader::new(),
        rim_io::FileAssetLocator::new(),
    );
    let outcome = use_case
        .execute(
            &mut session,
            ApplyOptions {
                source: args.source.into(),
                write_mods_config: true,
                write_merge_mod: args.write_merge_mod,
            },
        )
        .context("applying the session")?;

    println!("wrote ModsConfig.xml: {}", outcome.wrote_mods_config);
    if let Some(backup) = &outcome.backup_path {
        println!("backup: {}", backup.display());
    }
    if let Some(merge_mod_path) = &outcome.merge_mod_path {
        println!("wrote merge mod: {}", merge_mod_path.display());
        if let Some(backup) = &outcome.merge_mod_backup_path {
            println!("merge mod backup: {}", backup.display());
        }
    }
    if !outcome.skipped_merges.is_empty() {
        println!(
            "skipped {} incomplete merge decision(s) (not yet a complete plan):",
            outcome.skipped_merges.len()
        );
        for key in &outcome.skipped_merges {
            println!("  {}", TerminalSafe::line(key));
        }
    }
    Ok(())
}

/// Prints the hard problems in the order about to be written, before
/// anything is diffed or written. Informational only: the exit code never
/// depends on it, and nothing is printed when there is nothing to say.
fn print_preflight(preflight: &ApplyPreflight) {
    for line in format_preflight(preflight) {
        println!("{line}");
    }
}

fn format_preflight(preflight: &ApplyPreflight) -> Vec<String> {
    if preflight.items.is_empty() {
        return Vec::new();
    }
    let decided = preflight
        .items
        .iter()
        .filter(|item| item.acknowledged)
        .count();
    let counts = if decided == 0 {
        preflight.items.len().to_string()
    } else {
        format!("{}, {decided} already decided", preflight.items.len())
    };
    let mut lines = vec![format!(
        "hard problems in the {} order ({counts}):",
        format_order_source(preflight.source)
    )];
    lines.extend(preflight.items.iter().map(|item| {
        let marker = if item.acknowledged { "[decided] " } else { "" };
        format!("  {marker}{}", format_problem(&item.problem))
    }));
    lines
}

fn format_problem(problem: &HardProblem) -> String {
    match problem {
        HardProblem::MissingDependency {
            mod_id,
            dependency,
            display_name,
            availability,
        } => {
            let named = display_name.as_deref().map_or_else(String::new, |name| {
                format!(" ({})", TerminalSafe::line(name))
            });
            let state = match availability {
                Availability::InstalledInactive => "installed, not active",
                Availability::NotInstalled => "not installed",
            };
            format!(
                "missing dependency: {} requires {}{named} ({state})",
                TerminalSafe::line(mod_id),
                TerminalSafe::line(dependency)
            )
        }
        HardProblem::IncompatiblePair { a, b } => format!(
            "incompatible: {} and {}",
            TerminalSafe::line(a),
            TerminalSafe::line(b)
        ),
        HardProblem::MissingMod { mod_id, outcome } => {
            let result = match outcome {
                MissingModOutcome::RemovedFromActiveList => "removed from the active list",
                MissingModOutcome::KeptInActiveList => "kept in the active list; the game skips it",
            };
            format!(
                "missing mod: {} is not installed; {result}",
                TerminalSafe::line(mod_id)
            )
        }
        HardProblem::LoadRequirementViolated { after, before, .. } => {
            format!(
                "load requirement violated: {} must load after {}",
                TerminalSafe::line(after),
                TerminalSafe::line(before)
            )
        }
        HardProblem::AnyOfUnsatisfied { after, candidates } => {
            let names: Vec<String> = candidates
                .iter()
                .map(|candidate| TerminalSafe::line(candidate).to_string())
                .collect();
            format!(
                "unmet any-of: {} needs one of {} to load first",
                TerminalSafe::line(after),
                names.join(", ")
            )
        }
    }
}

fn print_dry_run_diff(
    session: &rim_session::Session,
    mods_config_path: &std::path::Path,
) -> anyhow::Result<()> {
    use rim_session::ports::ModsConfigStore;

    let on_disk = rim_io::ModsConfigFileStore::new()
        .read(mods_config_path)
        .context("reading the on-disk ModsConfig.xml")?;
    let old_order = rim_analyzer::domain::LoadOrder::new(on_disk.active_mods);
    let new_order = session.orders().get(session.selected());

    let provenance = session.sort_provenance();
    println!(
        "provenance: tie_break={}, use_imported_pairs={}, use_imported_placements={} ",
        format_tie_break(provenance.tie_break),
        provenance.use_imported_pairs,
        provenance.use_imported_placements
    );

    let mut changed = 0usize;
    println!(
        "Dry run: diff between the on-disk ModsConfig.xml and the {} order (+N = moved N slots earlier, -N = later):",
        format_order_source(session.selected())
    );
    for (new_position, id) in new_order.as_slice().iter().enumerate() {
        let old_position = old_order.position(id);
        if old_position == Some(new_position) {
            continue;
        }
        changed += 1;
        println!(
            "{:>4}. {:<45} {}",
            new_position + 1,
            TerminalSafe::line(id).to_string(),
            position_marker(new_position, old_position)
        );
    }
    println!(
        "{changed} of {} mod(s) would change position (nothing written — this is a dry run)",
        new_order.as_slice().len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use super::*;
    use crate::test_fixtures::copy_dir_recursive;

    struct FixedProbe(bool);

    impl GameProcessProbe for FixedProbe {
        fn is_running(&self) -> bool {
            self.0
        }
    }

    fn sample_game_fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("crates")
            .join("rim-analyzer")
            .join("tests")
            .join("fixtures")
            .join("sample_game")
    }

    /// A scratch copy of `rim-analyzer`'s checked-in fixture game tree —
    /// never the real game install or the real `ModsConfig.xml`.
    fn scratch_args(temp_dir: &Path, force: bool) -> ApplyArgs {
        let game_dir = temp_dir.join("game");
        copy_dir_recursive(&sample_game_fixture(), &game_dir).expect("copy fixture game tree");
        fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590").expect("write Version.txt");

        ApplyArgs {
            paths: PathsArgs {
                game_dir: Some(game_dir.clone()),
                workshop_dir: Some(temp_dir.join("workshop_does_not_exist")),
                mods_config: Some(game_dir.join("ModsConfig.xml")),
                profile_dir: Some(temp_dir.join("profile")),
            },
            dry_run: false,
            source: SourceArg::Suggested,
            force,
            write_merge_mod: false,
        }
    }

    #[test]
    fn refuses_to_apply_when_the_probe_reports_the_game_running_and_not_forced() {
        let temp_dir = tempdir().expect("tempdir");
        let args = scratch_args(temp_dir.path(), false);
        let game_dir = args
            .paths
            .game_dir
            .clone()
            .expect("game_dir was set by scratch_args");

        let result = run_with_probe(&args, &FixedProbe(true));

        let error = result.expect_err("must refuse when the game looks like it's running");
        assert!(error.to_string().contains("--force"));
        let backups: Vec<_> = fs::read_dir(&game_dir)
            .expect("read game dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bak-"))
            .collect();
        assert!(
            backups.is_empty(),
            "a refused apply must never write a backup"
        );
    }

    /// A `.bak-*` file directly next to `ModsConfig.xml` in `game_dir`,
    /// found by actually reading the directory rather than trusting a
    /// return value — proves the backup is a real file on disk, the way
    /// a user recovering from a bad apply would go looking for it.
    fn find_backup_file(game_dir: &Path) -> Option<PathBuf> {
        fs::read_dir(game_dir)
            .expect("read game dir")
            .filter_map(Result::ok)
            .find(|entry| entry.file_name().to_string_lossy().contains(".bak-"))
            .map(|entry| entry.path())
    }

    #[test]
    fn applies_when_forced_despite_the_game_running() {
        let temp_dir = tempdir().expect("tempdir");
        let args = scratch_args(temp_dir.path(), true);
        let game_dir = args
            .paths
            .game_dir
            .clone()
            .expect("game_dir was set by scratch_args");
        let profile_dir = args
            .paths
            .profile_dir
            .clone()
            .expect("profile_dir was set by scratch_args");

        run_with_probe(&args, &FixedProbe(true)).expect("forced apply must succeed");

        assert!(
            find_backup_file(&game_dir).is_some(),
            "a forced, non-dry-run apply must leave a ModsConfig.xml.bak-* file next to the original"
        );
        assert!(
            profile_dir.join("decisions.json").exists(),
            "decisions.json must be saved alongside the ModsConfig.xml write"
        );
        assert!(
            profile_dir.join("rules.json").exists(),
            "rules.json must be saved alongside the ModsConfig.xml write"
        );
    }

    #[test]
    fn applies_normally_when_the_game_is_not_running() {
        let temp_dir = tempdir().expect("tempdir");
        let args = scratch_args(temp_dir.path(), false);
        let game_dir = args
            .paths
            .game_dir
            .clone()
            .expect("game_dir was set by scratch_args");
        let profile_dir = args
            .paths
            .profile_dir
            .clone()
            .expect("profile_dir was set by scratch_args");

        run_with_probe(&args, &FixedProbe(false)).expect("apply must succeed");

        assert!(
            find_backup_file(&game_dir).is_some(),
            "a non-dry-run apply must leave a ModsConfig.xml.bak-* file next to the original"
        );
        assert!(
            profile_dir.join("decisions.json").exists(),
            "decisions.json must be saved alongside the ModsConfig.xml write"
        );
        assert!(
            profile_dir.join("rules.json").exists(),
            "rules.json must be saved alongside the ModsConfig.xml write"
        );
    }

    fn item(problem: HardProblem, acknowledged: bool) -> rim_resolve::preflight::PreflightItem {
        rim_resolve::preflight::PreflightItem {
            problem,
            acknowledged,
        }
    }

    #[test]
    fn a_clean_order_prints_no_preflight_lines() {
        let preflight = ApplyPreflight {
            source: OrderSource::Suggested,
            items: Vec::new(),
        };

        assert!(format_preflight(&preflight).is_empty());
    }

    #[test]
    fn the_preflight_header_counts_problems_and_decided_ones_and_marks_them() {
        let id = rim_analyzer::domain::ModId::new;
        let preflight = ApplyPreflight {
            source: OrderSource::Suggested,
            items: vec![
                item(
                    HardProblem::MissingDependency {
                        mod_id: id("app"),
                        dependency: id("lib"),
                        display_name: None,
                        availability: Availability::InstalledInactive,
                    },
                    false,
                ),
                item(
                    HardProblem::IncompatiblePair {
                        a: id("a"),
                        b: id("b"),
                    },
                    true,
                ),
            ],
        };

        assert_eq!(
            format_preflight(&preflight),
            [
                "hard problems in the Suggested order (2, 1 already decided):",
                "  missing dependency: app requires lib (installed, not active)",
                "  [decided] incompatible: a and b",
            ]
        );
    }

    #[test]
    fn preflight_lines_never_carry_a_control_character_from_a_mod() {
        let hostile = "evil\u{1b}[2J\u{9b}x\nforged";
        let id = rim_analyzer::domain::ModId::new;
        let problems = [
            HardProblem::MissingDependency {
                mod_id: id(hostile),
                dependency: id(hostile),
                display_name: Some(hostile.to_string()),
                availability: Availability::NotInstalled,
            },
            HardProblem::IncompatiblePair {
                a: id(hostile),
                b: id(hostile),
            },
            HardProblem::MissingMod {
                mod_id: id(hostile),
                outcome: MissingModOutcome::KeptInActiveList,
            },
            HardProblem::AnyOfUnsatisfied {
                after: id(hostile),
                candidates: [id(hostile)].into_iter().collect(),
            },
        ];
        let preflight = ApplyPreflight {
            source: OrderSource::Suggested,
            items: problems.into_iter().map(|p| item(p, false)).collect(),
        };

        let lines = format_preflight(&preflight);

        assert_eq!(lines.len(), 5, "{lines:?}");
        for line in &lines {
            assert!(!line.contains('\u{1b}'), "{line:?}");
            assert!(!line.contains('\u{9b}'), "{line:?}");
            assert!(!line.contains('\n'), "{line:?}");
        }
    }
}
