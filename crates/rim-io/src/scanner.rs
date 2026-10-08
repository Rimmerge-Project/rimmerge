//! [`AnalyzerScanner`]: wraps `rim_analyzer::infra::scan_with_progress`,
//! tag-evidence collection, and `rim_analyzer::analysis::build_ref` behind
//! the [`ModScanner`] port.

use rim_analyzer::domain::{FolderPolicy, ModId, ScanProgress as AnalyzerProgress, ScanStage};
use rim_analyzer::{analysis, infra};
use rim_resolve::tags;
use rim_session::ModInventory;
use rim_session::ProjectPaths;
use rim_session::ports::{ModScanner, ScanArtifacts, ScanError, ScanProgress, ScanProgressStage};

/// Scans a RimWorld install and runs the analyzer over it, using the
/// game's own `Version.txt` for the game version and RimWorld's real
/// folder-resolution rule (`LoadFolders.xml`, never the `--all-folders`
/// diagnostic override).
#[derive(Debug, Default, Clone, Copy)]
pub struct AnalyzerScanner;

impl AnalyzerScanner {
    /// Builds the scanner. Stateless — every call re-scans from disk.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

/// Maps the analyzer's own [`ScanStage`] onto the `ModScanner` port's
/// [`ScanProgressStage`], so callers (a CLI progress line, a Tauri event
/// payload) never need to depend on `rim_analyzer::domain::ScanStage`.
fn port_stage(stage: ScanStage) -> ScanProgressStage {
    match stage {
        ScanStage::Discovering => ScanProgressStage::Discovering,
        ScanStage::Scanning => ScanProgressStage::Scanning,
        ScanStage::Analyzing => ScanProgressStage::Analyzing,
    }
}

/// The port tick for one of the analyzer's own, or `None` for the one this adapter holds back.
///
/// The analyzer closes its `Analyzing` unit (the small vanilla index) with a 1-of-1 tick. This
/// adapter keeps that bracket open instead and closes it itself after the slow analysis, so the
/// stage never reads as finished while work remains.
fn forwarded(tick: AnalyzerProgress) -> Option<ScanProgress> {
    if tick.stage == ScanStage::Analyzing && tick.done >= tick.total {
        return None;
    }
    Some(ScanProgress {
        stage: port_stage(tick.stage),
        done: tick.done,
        total: tick.total,
    })
}

impl ModScanner for AnalyzerScanner {
    /// Forwards `rim_analyzer::infra::scan_with_progress`'s real
    /// discover/scan/analyze stages — including a tick per mod scanned — plus
    /// one `Analyzing` bracket (0 of 1, closed only after the slow
    /// `build_ref`/dangling-reference/source-index stretch), and two stages
    /// of this adapter's own (`CollectingTagEvidence`, `Done`) that happen
    /// after the analyzer's own work is complete. Builds the
    /// [`analysis::SourceIndex`] right after `build_ref`, before
    /// `scan_output` is dropped — nothing downstream can find a def's XML
    /// again otherwise.
    fn scan_and_analyze(
        &self,
        paths: &ProjectPaths,
        active_override: Option<&[ModId]>,
        progress: &mut dyn FnMut(ScanProgress),
    ) -> Result<ScanArtifacts, ScanError> {
        self.scan_and_analyze_observed(paths, active_override, progress, &mut || {})
    }
}

impl AnalyzerScanner {
    /// [`ModScanner::scan_and_analyze`] with a recording seam for tests: `analysis_finished` runs
    /// once the slow analysis is done and before its closing `Analyzing` tick, so a test can
    /// pin *where* that tick falls relative to the work, which the tick values alone cannot show.
    /// Production passes a no-op.
    fn scan_and_analyze_observed(
        &self,
        paths: &ProjectPaths,
        active_override: Option<&[ModId]>,
        progress: &mut dyn FnMut(ScanProgress),
        analysis_finished: &mut dyn FnMut(),
    ) -> Result<ScanArtifacts, ScanError> {
        // `{:#}` (anyhow's alternate `Display`) renders the full
        // `cause: cause: cause` chain in one line — more useful in a
        // `ScanError`'s message than either the bare top-level `{}` or
        // the multi-line `{:?}` debug chain.
        let game_version = infra::paths::default_game_version(&paths.game_dir)
            .map_err(|e| ScanError(format!("{e:#}")))?;
        let scan_config = infra::ScanConfig {
            game_dir: paths.game_dir.clone(),
            workshop_dir: paths.workshop_dir.clone(),
            mods_config_path: paths.mods_config.clone(),
            game_version,
            folder_policy: FolderPolicy::LoadFolders,
            active_mods: active_override.map(<[ModId]>::to_vec),
        };

        let scan_output = infra::scan_with_progress(&scan_config, &mut |scan_progress| {
            if let Some(tick) = forwarded(scan_progress) {
                progress(tick);
            }
        })
        .map_err(|e| ScanError(format!("{e:#}")))?;

        let context = analysis::RunContext {
            game_dir: paths.game_dir.clone(),
            workshop_dir: paths.workshop_dir.clone(),
            mods_config_path: paths.mods_config.clone(),
            game_version,
        };
        // `build_ref` only borrows `scan_output`, so `scanned_mods` below
        // reads the same scan `collect_evidence` needs without a clone,
        // and `source_index::build` (also borrow-only) still has
        // `scan_output` to read afterward.
        //
        // The analysis below is the slowest stretch of the whole scan (tens of
        // seconds on a large install) and has no per-unit ticks of its own. It runs
        // inside the one `Analyzing` unit the analyzer opened (0 of 1) and the tick
        // after it closes, so a consumer sees "working" instead of a bar frozen at
        // its last scanning tick.
        let mut report = analysis::build_ref(&scan_output, &context);
        // The lazy IO half of the dangling-reference explanation — see
        // `infra::explain_dangling_references`'s own doc comment; needs `scan_output` still alive, which
        // `build_ref` (unlike `build`) leaves it.
        infra::explain_dangling_references(&mut report.conflicts, &scan_output);
        let sources = analysis::source_index::build(&scan_output);
        analysis_finished();
        progress(ScanProgress {
            stage: ScanProgressStage::Analyzing,
            done: 1,
            total: 1,
        });

        progress(ScanProgress {
            stage: ScanProgressStage::CollectingTagEvidence,
            done: 0,
            total: 1,
        });
        let evidence = tags::collect_evidence(&scan_output.scanned_mods, &report);

        progress(ScanProgress {
            stage: ScanProgressStage::Done,
            done: 1,
            total: 1,
        });

        Ok(ScanArtifacts {
            report,
            evidence,
            sources,
        })
    }
}

/// Discovery-only inventory: wraps `rim_analyzer::infra::inventory` — no full
/// mod scan, just identity and declared dependencies for every mod on disk,
/// far faster than [`AnalyzerScanner`]'s own
/// [`ModScanner::scan_and_analyze`]. Returns the built [`ModInventory`]
/// alongside the active list it was built against, so a caller (`apps/cli`'s
/// `mods` subcommands) has everything [`rim_session::ActiveSet::new`] needs
/// with no live [`rim_session::Session`] at all.
///
/// `active_override`, mirroring [`ModScanner::scan_and_analyze`]'s own
/// second parameter: `Some` scans that exact list instead of re-reading
/// `paths.mods_config`'s own `<activeMods>` a second time. A caller that
/// is about to write back to the same file (every `mods` subcommand)
/// should already have read it once, through
/// [`rim_session::ports::ModsConfigStore::read`], to get `version`/
/// `knownExpansions` for the write — passing that same read's own
/// `active_mods` here closes the window a second, independent read would
/// open: the game can rewrite `ModsConfig.xml` on exit between two reads,
/// and planning against one snapshot while writing another back on top
/// of a *different* one is exactly the inconsistency a single read
/// avoids. `None` re-reads the file itself, for a caller with no reason
/// to read it first.
///
/// # Errors
///
/// Returns [`ScanError`] when `paths.game_dir` isn't a directory or
/// `paths.mods_config` isn't a file — the same preconditions
/// [`ModScanner::scan_and_analyze`] enforces, reported the same way
/// (`{:#}`, anyhow's full cause chain in one line).
pub fn discover_inventory(
    paths: &ProjectPaths,
    active_override: Option<&[ModId]>,
) -> Result<(ModInventory, Vec<ModId>), ScanError> {
    let game_version = infra::paths::default_game_version(&paths.game_dir)
        .map_err(|e| ScanError(format!("{e:#}")))?;
    let scan_config = infra::ScanConfig {
        game_dir: paths.game_dir.clone(),
        workshop_dir: paths.workshop_dir.clone(),
        mods_config_path: paths.mods_config.clone(),
        game_version,
        folder_policy: FolderPolicy::LoadFolders,
        active_mods: active_override.map(<[ModId]>::to_vec),
    };
    let output = infra::inventory(&scan_config).map_err(|e| ScanError(format!("{e:#}")))?;
    let inventory = ModInventory::from_inventory_output(&output.discovered, &output.missing);
    Ok((inventory, output.active))
}

#[cfg(test)]
#[path = "../tests/support/scratch.rs"]
mod scratch;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use tempfile::tempdir;

    use super::scratch;
    use super::*;

    fn analyzer_tick(stage: ScanStage, done: usize, total: usize) -> AnalyzerProgress {
        AnalyzerProgress { stage, done, total }
    }

    #[test]
    fn forwarded_holds_back_the_analyzers_closing_analyzing_tick() {
        let closing = analyzer_tick(ScanStage::Analyzing, 1, 1);

        assert_eq!(forwarded(closing), None);
    }

    #[test]
    fn forwarded_passes_the_analyzers_opening_analyzing_tick() {
        let opening = analyzer_tick(ScanStage::Analyzing, 0, 1);

        assert_eq!(
            forwarded(opening),
            Some(ScanProgress {
                stage: ScanProgressStage::Analyzing,
                done: 0,
                total: 1,
            })
        );
    }

    #[test]
    fn forwarded_passes_scanning_ticks_even_when_the_stage_is_complete() {
        let last_mod = analyzer_tick(ScanStage::Scanning, 3, 3);

        assert_eq!(
            forwarded(last_mod),
            Some(ScanProgress {
                stage: ScanProgressStage::Scanning,
                done: 3,
                total: 3,
            })
        );
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Tick(ScanProgressStage, usize, usize),
        AnalysisFinished,
    }

    #[test]
    fn the_analyzing_bracket_closes_only_after_the_analysis_has_finished()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch_dir = tempdir()?;
        let paths = scratch::scratch_paths(scratch_dir.path())?;
        let events = RefCell::new(Vec::new());

        AnalyzerScanner::new().scan_and_analyze_observed(
            &paths,
            None,
            &mut |tick| {
                events
                    .borrow_mut()
                    .push(Event::Tick(tick.stage, tick.done, tick.total));
            },
            &mut || events.borrow_mut().push(Event::AnalysisFinished),
        )?;

        let events = events.into_inner();
        let opened = events
            .iter()
            .position(|event| *event == Event::Tick(ScanProgressStage::Analyzing, 0, 1))
            .ok_or("the Analyzing bracket never opened")?;
        assert_eq!(
            events[opened..],
            [
                Event::Tick(ScanProgressStage::Analyzing, 0, 1),
                Event::AnalysisFinished,
                Event::Tick(ScanProgressStage::Analyzing, 1, 1),
                Event::Tick(ScanProgressStage::CollectingTagEvidence, 0, 1),
                Event::Tick(ScanProgressStage::Done, 1, 1),
            ]
        );
        Ok(())
    }
}
