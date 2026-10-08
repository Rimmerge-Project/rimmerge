//! `AnalyzerScanner`'s progress stream against a scratch copy of
//! `rim-analyzer`'s checked-in fixture game tree: the overall shape of what an interface sees.
//! Where the closing `Analyzing` tick falls relative to the slow analysis, and the forwarding
//! filter itself, are pinned by the unit tests in `src/scanner.rs`, which can reach the
//! recording seam these tick values cannot show.

#[path = "support/scratch.rs"]
mod scratch;

use rim_io::AnalyzerScanner;
use rim_session::ports::{ModScanner, ScanProgressStage};
use scratch::scratch_paths;
use tempfile::tempdir;

type Tick = (ScanProgressStage, usize, usize);

/// Returns a `Result` so a setup or scan failure surfaces as a `?` error, not a panic.
fn recorded_ticks() -> Result<Vec<Tick>, Box<dyn std::error::Error>> {
    let scratch = tempdir()?;
    let paths = scratch_paths(scratch.path())?;
    let mut ticks: Vec<Tick> = Vec::new();
    AnalyzerScanner::new().scan_and_analyze(&paths, None, &mut |tick| {
        ticks.push((tick.stage, tick.done, tick.total));
    })?;
    Ok(ticks)
}

#[test]
fn the_analysis_after_the_last_scanning_tick_is_bracketed_by_analyzing_ticks()
-> Result<(), Box<dyn std::error::Error>> {
    let ticks = recorded_ticks()?;

    let last_scanning = ticks
        .iter()
        .rposition(|(stage, _, _)| *stage == ScanProgressStage::Scanning)
        .expect("the fixture has one active mod, so at least one Scanning tick");
    let after_scanning = &ticks[last_scanning + 1..];

    // One `Analyzing` bracket, opened by the analyzer and closed by the scanner after
    // `build_ref` + dangling explanation + source index, then the tag-evidence stage:
    // no 1/1 before the slow part, and no unticked heavy work between ticks.
    assert_eq!(
        after_scanning,
        [
            (ScanProgressStage::Analyzing, 0, 1),
            (ScanProgressStage::Analyzing, 1, 1),
            (ScanProgressStage::CollectingTagEvidence, 0, 1),
            (ScanProgressStage::Done, 1, 1),
        ]
    );
    Ok(())
}
