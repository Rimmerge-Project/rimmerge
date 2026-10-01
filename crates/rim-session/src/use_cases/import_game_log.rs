//! [`ImportGameLog`]: imports a real game log for comparison with predictions.
//! Reads and parses a game log (a `Player.log` or a console snapshot)
//! through a [`GameLogReader`] port
//! (`rim-io`'s `rim_io::game_log::parse` is pure and attribution-free;
//! this use case is the one place that raw text is joined against the
//! live scan's own active-mod list) and produces a [`GameLogSummary`].
//!
//! **Attribution rules**: primary — longest-prefix match of a
//! `file:`/block-trailer/texture path against each active mod's
//! `Mod::loaded_folders`/`Mod::path` (`attribution::PathIndex`, normalized
//! once per import); fallback — the `[Tag]` display name, lowercased,
//! against every active mod's own display name, then as a packageId, then
//! reduced to ASCII letters and digits (a fallback must match
//! exactly one mod: a packageId or compact name several mods claim
//! attributes nothing, [`FamilyAttribution::Ambiguous`] for a family).
//! Anything that resolves
//! to neither is [`LogAttribution::Unattributed`] with its own raw text
//! — never dropped, never guessed at.
//!
//! **Family attribution** ([`AttributedClass`]) resolves each family's
//! first-entry evidence once: a path, a name, a name and path, a type (a
//! stack's innermost non-engine frame, or the type a type-load error names)
//! or an assembly name. A type is attributed by
//! `SourceIndex::namespace_ownership`: the longest namespace prefix that names
//! an assembly some active mod ships, and only that one; it is never walked
//! outward to a caller's frame, so a mod whose namespace differs from its DLL
//! name stays unattributed rather than blaming a caller. The typed lists above
//! and the families share the path index, the display-name map and the `[Tag]`
//! reading (`ports::leading_bracket_tag`); a family's result additionally
//! tells "no owner" from "several owners" ([`FamilyAttribution`]).
//!
//! **Why not `rim_analyzer::analysis::edges::build_name_map`** for the
//! display-name fallback: that function takes
//! `&[rim_analyzer::domain::ScannedMod]` (scan-time data — every file this
//! scan touched, template lists, ...), which this use case never has; it
//! only has `session.report().mods: Vec<Mod>` (the already-built, much
//! smaller domain type). [`super::verify_order::build_gate_name_map`] is
//! the shared "lowercased display name -> id, first mod to claim a name
//! wins" resolution directly against `Mod` — shared so
//! `ImportGameLog`/`VerifyOrder`/`ContributesNothing` never re-derive the
//! map by hand, which is how a verbatim-keyed map slips in — without the
//! duplicate-name warning `build_name_map` also produces (this use case
//! only needs a lookup, not a warnings list).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rim_analyzer::domain::ModId;

use self::attribution::{Attributor, NameResolver, PathIndex, attribute_classes};
use crate::Session;
use crate::ports::{
    ClassTally, EngineInfoKind, EntryClass, Family, FamilyKey, GameLogError, GameLogReader,
    KindChoice, LoadEventKind, LogCoverage, LogKind, LogReadStats, LogTotals, LoggingGap,
    RawCrossReference, RawDdsFailure, RawDependencyWarning, RawPatchFailure, RawTimer,
    SentinelReport, StackTraceBlock, StackTraceOp, Tally, leading_bracket_tag, totals_of,
};

mod attribution;

/// What a log family's evidence resolved to. Unlike [`LogAttribution`] (the
/// typed lists', and the desktop's, shape) it tells a name no active mod
/// carries from one several do: a DLL shipped by three mods blames none of
/// them alone, but the candidates are actionable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FamilyAttribution {
    /// Resolved to this active mod.
    Mod(ModId),
    /// Resolved to no active mod; the raw text (a name, a path, a type, an
    /// assembly) is kept.
    Unattributed {
        /// What the log said.
        raw: String,
    },
    /// Several active mods carry what the log named (a shared DLL).
    Ambiguous {
        /// What the log said.
        raw: String,
        /// Every active mod that ships it (at least two).
        candidates: BTreeSet<ModId>,
    },
}

impl From<LogAttribution> for FamilyAttribution {
    fn from(attribution: LogAttribution) -> Self {
        match attribution {
            LogAttribution::Mod(id) => Self::Mod(id),
            LogAttribution::Unattributed(raw) => Self::Unattributed { raw },
        }
    }
}

/// One raw mod reference (a `[Tag]`, a `file:` path, ...), resolved
/// against the live scan or left as-is when it resolves to nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogAttribution {
    /// Resolved to this active mod.
    Mod(ModId),
    /// Resolved to no active mod — the raw text is kept so the user can
    /// still see what the log said.
    Unattributed(String),
}

/// One enclosing operation in a stack-trace block's own chain —
/// [`StackTraceDetail::enclosing_chain`]'s element, everything but the
/// leaf (which [`StackTraceDetail`]'s own fields already surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnclosingOp {
    /// The operation's own class.
    pub class: String,
    /// The raw parenthesized detail, when present.
    pub detail: Option<String>,
    /// Why the fold stopped here.
    pub reason: String,
    /// The branch marker, when present.
    pub branch: Option<String>,
}

/// The richer half of a patch failure — the leaf mutation that actually
/// failed, plus every enclosing operation between it and the top level,
/// innermost first ("the observed-vs-predicted join is far stronger on
/// the leaf than on the terse top-level `operation` text").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackTraceDetail {
    /// The leaf's own class.
    pub leaf_class: String,
    /// The leaf's own xpath, when its detail was one.
    pub leaf_xpath: Option<String>,
    /// The leaf's own failure reason.
    pub leaf_reason: String,
    /// Every enclosing operation, innermost first, outermost
    /// (top-level) last — empty when the leaf *is* the top level.
    pub enclosing_chain: Vec<EnclosingOp>,
}

/// One `[<Mod>] Patch operation <op> failed` line, attributed, with its
/// `file:` line and (when the parser found a block that belongs to it)
/// its own [`StackTraceDetail`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchFailureSummary {
    /// The mod this failure is attributed to.
    pub attribution: LogAttribution,
    /// The top-level operation's own RimWorld-log identity text.
    pub operation: String,
    /// The `file:` line's path.
    pub source_file: Option<String>,
    /// The richer stack-trace record, when one was paired with this
    /// failure.
    pub stack_trace: Option<StackTraceDetail>,
}

/// A stack-trace block the parser found with no terse failure line
/// paired to it — kept, attributed, never dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpairedStackTrace {
    /// The mod this block is attributed to.
    pub attribution: LogAttribution,
    /// The block's own trailer source-file path.
    pub source_file: Option<String>,
    /// The block's own leaf/chain detail — `None` only when not one of
    /// the block's own lines could be parsed as an operation, even after
    /// joining continuation lines (`rim_io::game_log`'s own join fix for
    /// a multi-line quoted-xpath rendering); not seen on any of the 5
    /// real logs checked. Kept (with `attribution`/`source_file` intact)
    /// rather than dropping the whole block: this field alone being
    /// unreadable is not a reason to also throw away the mod and file
    /// this block is otherwise known to belong to.
    pub detail: Option<StackTraceDetail>,
}

/// One texture-fallback size report, attributed by its own texture
/// path (the startup page's only source for the bad-DDS column).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdsFailureSummary {
    /// The mod this failure is attributed to.
    pub attribution: LogAttribution,
    /// The texture file's own path, as logged.
    pub path: String,
    /// The texture's width in pixels.
    pub width: u32,
    /// The texture's height in pixels.
    pub height: u32,
    /// The compressed format.
    pub format: String,
}

/// One `Mod ... dependency (...) needs to have <downloadUrl>` warning,
/// attributed by the dependent mod's own display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyWarningSummary {
    /// The dependent mod this warning is attributed to.
    pub attribution: LogAttribution,
    /// The missing dependency's own package id.
    pub dependency_id: String,
}

/// One timer, attributed by a `[Tag]` prefix in its own label when it
/// has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerSummary {
    /// The mod this timer is attributed to.
    pub attribution: LogAttribution,
    /// The timer's own context label.
    pub label: String,
    /// The timed duration, in whole milliseconds.
    pub milliseconds: u64,
    /// The startup pass the timer was logged in (`0` before the first
    /// `RimWorld <version>` banner).
    pub pass: u32,
}

/// One class's families (as the parser tallied them) and the mod each was
/// attributed to. Attribution is per family, from its first entry: a
/// family's evidence is the class's kind of input (see
/// [`AttributionInput`](crate::ports::AttributionInput)), and a class or an
/// entry with no evidence (a cross-reference, an engine message, a stack of
/// engine frames only) has no row in `attribution`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributedClass {
    /// The parser's tally, unchanged.
    pub tally: ClassTally,
    /// What each family with evidence was attributed to.
    pub attribution: BTreeMap<FamilyKey, FamilyAttribution>,
}

/// Everything [`ImportGameLog::execute`] recovers from a `Player.log`,
/// attributed against the session's own active-mod list. Does **not**
/// build the predicted-vs-observed view against
/// [`rim_resolve::domain::Finding::PatchWillFail`] — that is a separate
/// page's concern; this only exposes what such a join needs
/// (`operation`/`leaf_xpath`, joined through
/// [`rim_resolve::domain::normalize_log_text`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameLogSummary {
    /// Every terse patch failure, attributed.
    pub patch_failures: Vec<PatchFailureSummary>,
    /// Stack-trace blocks left over with no terse line paired to them.
    pub extra_stack_traces: Vec<UnpairedStackTrace>,
    /// Every cross-reference error, verbatim (no mod attribution is
    /// possible — RimWorld's own log never names which mod's patch
    /// caused one).
    pub cross_references: Vec<RawCrossReference>,
    /// Every DDS failure, attributed by its own texture path.
    pub dds_failures: Vec<DdsFailureSummary>,
    /// Every dependency-without-URL warning, attributed.
    pub dependency_warnings: Vec<DependencyWarningSummary>,
    /// Every recognized timer, attributed.
    pub timers: Vec<TimerSummary>,
    /// Every raw def-cache-plugin log line — surfaced for the apply-dialog note.
    pub def_cache_lines: Vec<String>,
    /// Every mod-list block the game logged on starting or loading a
    /// game, in file order. Empty when the log has no such block at all
    /// (a real log shape) — "can't compare orders", distinct from an
    /// event with no mods ("ran with zero mods").
    pub load_events: Vec<LoadEvent>,
    /// What the reader had to truncate, replace or drop to read the log
    /// at any size; all zero (bar `lines_read`) for an ordinary log.
    pub read_stats: LogReadStats,
    /// Every entry of the log, grouped by class and family (property P1),
    /// each family attributed to a mod where its first entry carried
    /// evidence.
    pub classes: BTreeMap<EntryClass, AttributedClass>,
    /// Blank lines that separate entries and belong to no entry.
    pub blank_separator_lines: u64,
    /// The loose-sentinel check of the parse (property P2).
    pub sentinels: SentinelReport,
    /// What the file covers: its kind and the facts that kind can state.
    pub coverage: LogCoverage,
    /// Every stretch where the engine stopped writing messages, in file
    /// order: what the game never wrote cannot be recovered.
    pub logging_gaps: Vec<LoggingGap>,
}

impl GameLogSummary {
    /// The kind of log this summary was read from.
    #[must_use]
    pub fn kind(&self) -> LogKind {
        self.coverage.kind()
    }

    /// The families whose count may be short because their line span
    /// overlaps a logging gap, in class then key order. Derived from the
    /// gaps and the spans, never stored. The two engine classes that report
    /// the gaps themselves are exempt: their counts are exact.
    #[must_use]
    pub fn lower_bound_families(&self) -> Vec<(EntryClass, &FamilyKey)> {
        self.classes
            .iter()
            .filter(|(class, _)| !is_gap_marker(**class))
            .flat_map(|(class, attributed)| {
                attributed
                    .tally
                    .families
                    .iter()
                    .filter(|(_, family)| self.is_lower_bound(family))
                    .map(move |(key, _)| (*class, key))
            })
            .collect()
    }

    /// Whether `family`'s line span overlaps any logging gap.
    fn is_lower_bound(&self, family: &Family) -> bool {
        self.logging_gaps
            .iter()
            .any(|gap| gap.overlaps(family.first_line, family.last_line))
    }

    /// Entries held by the [`Self::lower_bound_families`]: how much of the
    /// file the gaps put in doubt.
    #[must_use]
    pub fn lower_bound_entries(&self) -> u64 {
        self.classes
            .iter()
            .filter(|(class, _)| !is_gap_marker(**class))
            .flat_map(|(_, attributed)| attributed.tally.families.values())
            .filter(|family| self.is_lower_bound(family))
            .map(|family| family.count)
            .sum()
    }

    /// Startup passes (`RimWorld <version>` banner entries) the file shows,
    /// for a `Player.log` only. A console snapshot is part of one session's
    /// console, never a whole session, so it states no passes: a banner
    /// entry near its top is not a startup pass the file can vouch for.
    #[must_use]
    pub fn startup_passes(&self) -> Option<u64> {
        match &self.coverage {
            LogCoverage::PlayerLog(player_log) => Some(player_log.passes),
            LogCoverage::ConsoleSnapshot(_) => None,
        }
    }

    /// The reconciliation of the parse (property P1).
    #[must_use]
    pub fn totals(&self) -> LogTotals {
        totals_of(
            self.classes
                .iter()
                .map(|(class, entry)| (class, &entry.tally)),
            self.read_stats.lines_read,
            self.blank_separator_lines,
        )
    }

    /// Whether the summary shows less than the session produced: a read
    /// bound truncated something, a class pushed families into its
    /// overflow, the engine stopped logging for a stretch, or the game had
    /// already evicted the oldest entries of a console snapshot.
    #[must_use]
    pub fn has_losses(&self) -> bool {
        self.read_stats.has_losses()
            || !self.logging_gaps.is_empty()
            || self.coverage.is_missing_older_entries()
            || self
                .classes
                .values()
                .any(|class| class.tally.overflow != Tally::default())
    }

    /// The mod list of the **last** load event — the order the session
    /// that wrote this log actually ended up playing under, and so the
    /// one to compare against an analyzed order. `None` when the log has
    /// no load event.
    #[must_use]
    pub fn last_load_order(&self) -> Option<&[ModId]> {
        self.load_events.last().map(|event| event.mods.as_slice())
    }

    /// Whether the log's load events disagree with each other: at least
    /// two of them carry different mod lists (a session that loaded
    /// saves under a changed mod list). Derived, never stored.
    #[must_use]
    pub fn load_events_disagree(&self) -> bool {
        let Some((first, rest)) = self.load_events.split_first() else {
            return false;
        };
        rest.iter().any(|event| event.mods != first.mods)
    }
}

/// Whether `class` is one of the engine's two logging-gap messages.
fn is_gap_marker(class: EntryClass) -> bool {
    matches!(
        class,
        EntryClass::EngineInfo(EngineInfoKind::LoggingStopped | EngineInfoKind::LoggingResumed)
    )
}

/// One mod-list block from the log, attributed to [`ModId`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadEvent {
    /// The 1-based line number of the block's header line.
    pub line: usize,
    /// Which header opened the block: a new game or a save load.
    pub kind: LoadEventKind,
    /// The active mods in load order.
    pub mods: Vec<ModId>,
}

/// Everything that can go wrong importing a `Player.log`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportGameLogError {
    /// The log couldn't be read or parsed.
    #[error(transparent)]
    Read(#[from] GameLogError),
}

/// Imports and attributes a game log (a `Player.log` or a console snapshot).
pub struct ImportGameLog<Reader> {
    reader: Reader,
}

impl<Reader: GameLogReader> ImportGameLog<Reader> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(reader: Reader) -> Self {
        Self { reader }
    }

    /// Reads, parses, and attributes `path` against `session`'s own
    /// active-mod list, as the kind `kind` says.
    ///
    /// # Errors
    ///
    /// Returns [`ImportGameLogError::Read`] when the log can't be read;
    /// a log's size, its kind or invalid UTF-8 never causes it.
    pub fn execute(
        &self,
        session: &Session,
        path: &Path,
        kind: KindChoice,
    ) -> Result<GameLogSummary, ImportGameLogError> {
        let parsed = self
            .reader
            .read(path, &session.mod_knowledge().log_formats(), kind)?;
        let mods = &session.report().mods;
        let name_resolver = NameResolver::new(mods, super::verify_order::build_gate_name_map(mods));
        let path_index = PathIndex::new(mods);
        let attributor = Attributor {
            paths: &path_index,
            names: &name_resolver,
            sources: session.sources(),
        };

        let patch_failures = parsed
            .patch_failures
            .into_iter()
            .map(|failure| patch_failure_summary(failure, &attributor))
            .collect();
        let extra_stack_traces = parsed
            .extra_stack_traces
            .into_iter()
            .map(|block| unpaired_stack_trace(block, &attributor))
            .collect();
        let dds_failures = parsed
            .dds_failures
            .into_iter()
            .map(|dds| dds_failure_summary(dds, &attributor))
            .collect();
        let dependency_warnings = parsed
            .dependency_warnings
            .into_iter()
            .map(|warning| dependency_warning_summary(warning, &attributor))
            .collect();
        let timers = parsed
            .timers
            .into_iter()
            .map(|timer| timer_summary(timer, &attributor))
            .collect();
        let load_events = parsed
            .load_events
            .into_iter()
            .map(|event| LoadEvent {
                line: event.line,
                kind: event.kind,
                mods: event.mods.into_iter().map(ModId::new).collect(),
            })
            .collect();

        Ok(GameLogSummary {
            patch_failures,
            extra_stack_traces,
            cross_references: parsed.cross_references,
            dds_failures,
            dependency_warnings,
            timers,
            def_cache_lines: parsed.def_cache_lines,
            load_events,
            read_stats: parsed.read_stats,
            classes: attribute_classes(parsed.classes, &attributor),
            blank_separator_lines: parsed.blank_separator_lines,
            sentinels: parsed.sentinels,
            coverage: parsed.coverage,
            logging_gaps: parsed.logging_gaps,
        })
    }
}

fn patch_failure_summary(
    failure: RawPatchFailure,
    attributor: &Attributor<'_>,
) -> PatchFailureSummary {
    let attribution =
        attributor.by_path_then_name(&failure.mod_tag, failure.source_file.as_deref());
    PatchFailureSummary {
        attribution,
        operation: failure.operation,
        source_file: failure.source_file,
        stack_trace: failure.stack_trace.and_then(stack_trace_detail),
    }
}

fn unpaired_stack_trace(block: StackTraceBlock, attributor: &Attributor<'_>) -> UnpairedStackTrace {
    let attribution = attributor.by_path_then_name(&block.mod_tag, block.source_file.as_deref());
    let source_file = block.source_file.clone();
    let detail = stack_trace_detail(block);
    UnpairedStackTrace {
        attribution,
        source_file,
        detail,
    }
}

fn dds_failure_summary(dds: RawDdsFailure, attributor: &Attributor<'_>) -> DdsFailureSummary {
    DdsFailureSummary {
        attribution: attributor.by_path(&dds.path),
        path: dds.path,
        width: dds.width,
        height: dds.height,
        format: dds.format,
    }
}

fn dependency_warning_summary(
    warning: RawDependencyWarning,
    attributor: &Attributor<'_>,
) -> DependencyWarningSummary {
    DependencyWarningSummary {
        attribution: attributor.by_name(&warning.mod_name),
        dependency_id: warning.dependency_id,
    }
}

fn timer_summary(timer: RawTimer, attributor: &Attributor<'_>) -> TimerSummary {
    let attribution = match leading_bracket_tag(&timer.label) {
        Some(tag) => match attributor.by_name(tag) {
            mod_match @ LogAttribution::Mod(_) => mod_match,
            LogAttribution::Unattributed(_) => LogAttribution::Unattributed(timer.label.clone()),
        },
        None => LogAttribution::Unattributed(timer.label.clone()),
    };
    TimerSummary {
        attribution,
        label: timer.label,
        milliseconds: timer.milliseconds,
        pass: timer.pass,
    }
}

/// Splits a stack-trace block's own ops into its leaf (first) and
/// enclosing chain (the rest, innermost first) — `None` for the
/// (malformed, never seen in a real log) case of a block with no op
/// lines at all.
fn stack_trace_detail(block: StackTraceBlock) -> Option<StackTraceDetail> {
    let mut ops = block.ops.into_iter();
    let leaf = ops.next()?;
    let enclosing_chain = ops.map(enclosing_op).collect();
    Some(StackTraceDetail {
        leaf_class: leaf.class,
        leaf_xpath: leaf.xpath,
        leaf_reason: leaf.reason,
        enclosing_chain,
    })
}

fn enclosing_op(op: StackTraceOp) -> EnclosingOp {
    EnclosingOp {
        class: op.class,
        detail: op.detail,
        reason: op.reason,
        branch: op.branch,
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::analysis::SourceIndex;
    use rim_resolve::domain::DecisionSet;
    use rim_resolve::test_support::ReportBuilder;

    use super::attribution::NameMatch;
    use super::*;
    use crate::ports::{
        AttributionInput, Family, FamilyKey, LogReadStats, ModsConfigFile, RawLoadEvent,
        StoredRules,
    };
    use crate::test_support::{FakeGameLogReader, session_fixture};
    use crate::{ProjectPaths, Session};

    fn parsed_with(patch_failures: Vec<RawPatchFailure>) -> crate::ports::ParsedGameLog {
        crate::ports::ParsedGameLog {
            patch_failures,
            ..Default::default()
        }
    }

    fn stack_op(class: &str, xpath: Option<&str>, reason: &str) -> StackTraceOp {
        StackTraceOp {
            class: class.to_string(),
            detail: xpath.map(|x| format!("xpath={x}")),
            xpath: xpath.map(str::to_string),
            reason: reason.to_string(),
            branch: None,
        }
    }

    /// A spec for one [`session_with_mods`] mod: `(id, display_name,
    /// path, loaded_folders)`.
    type ModSpec<'a> = (&'a str, &'a str, &'a str, &'a [&'a str]);

    /// A session whose mods' `Mod::path`/`loaded_folders`/display names
    /// are all set explicitly — `crate::test_support::session_fixture`'s
    /// plain mods have an empty path and `name == id`, which can't
    /// exercise path attribution at all, let alone a *choice* between
    /// two real candidates.
    fn session_with_mods(specs: &[ModSpec<'_>]) -> Session {
        session_with_mods_and_sources(specs, SourceIndex::default())
    }

    /// [`session_with_mods`] over an explicit source index (its shipped
    /// assemblies decide stack attribution).
    fn session_with_mods_and_sources(specs: &[ModSpec<'_>], sources: SourceIndex) -> Session {
        let mut builder = ReportBuilder::new();
        for &(id, display_name, path, loaded_folders) in specs {
            builder = builder.mod_with(id, |m| {
                m.name = display_name.to_string();
                m.path = path.into();
                m.loaded_folders = loaded_folders.iter().map(Into::into).collect();
            });
        }
        let report = builder.build();
        Session::new(
            ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report,
            Vec::new(),
            sources,
            StoredRules::default(),
            DecisionSet::new(),
            ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: specs.iter().map(|(id, ..)| ModId::new(*id)).collect(),
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        )
    }

    /// [`session_with_mods`] for the common one-mod case.
    fn session_with_mod_paths(
        id: &str,
        display_name: &str,
        path: &str,
        loaded_folders: &[&str],
    ) -> Session {
        session_with_mods(&[(id, display_name, path, loaded_folders)])
    }

    /// A [`NameResolver`] over mods given as `(id, display name)`, built
    /// the way [`ImportGameLog::execute`] builds it.
    fn resolver_over(specs: &[(&str, &str)]) -> NameResolver {
        let mut builder = ReportBuilder::new();
        for &(id, display_name) in specs {
            builder = builder.mod_with(id, |m| m.name = display_name.to_string());
        }
        let mods = builder.build().mods;
        NameResolver::new(
            &mods,
            crate::use_cases::verify_order::build_gate_name_map(&mods),
        )
    }

    #[test]
    fn a_package_id_match_beats_a_compact_name_match_on_a_different_mod() {
        // `pkg.target` is the first mod's packageId; the second mod's
        // display name "Pkg Target" reduces to the same letters and digits.
        // Neither is the exact (lowercased) display name, so the order of the
        // two fallbacks decides: the packageId wins.
        let resolver = resolver_over(&[("pkg.target", "Alpha"), ("other.mod", "Pkg Target")]);

        match resolver.resolve("pkg.target") {
            NameMatch::Sole(id) => assert_eq!(id, &ModId::new("pkg.target")),
            NameMatch::Ambiguous(_) | NameMatch::Unmatched => {
                panic!("a packageId match must resolve to its one mod")
            }
        }
    }

    #[test]
    fn a_tag_naming_a_package_id_two_active_copies_share_is_ambiguous_with_both() {
        // `dup.mod` and its `_steam` copy share one base packageId, and
        // neither's display name is the tag, so the tag resolves to both.
        let resolver = resolver_over(&[("dup.mod", "Dup One"), ("dup.mod_steam", "Dup Two")]);

        match resolver.resolve("dup.mod") {
            NameMatch::Ambiguous(candidates) => assert_eq!(
                candidates.iter().cloned().collect::<Vec<_>>(),
                [ModId::new("dup.mod"), ModId::new("dup.mod_steam")]
            ),
            NameMatch::Sole(_) | NameMatch::Unmatched => {
                panic!("two active copies of one packageId must be ambiguous, not a guess")
            }
        }
    }

    #[test]
    fn attributes_a_patch_failure_by_the_file_path_over_a_mismatched_display_name() {
        // A real counterexample: `[Example Biomes]` is logged
        // against a file that actually belongs to a different mod. The
        // session's own mod is named something else entirely from the
        // terse line's own `[Tag]`, so the display-name fallback alone
        // would resolve to nothing (or the wrong mod) — only the `file:`
        // path, matched against `loaded_folders`, gets this right.
        let session = session_with_mod_paths(
            "fixture.moda",
            "Real Mod Display Name",
            "C:/Mods/ModA",
            &["C:/Mods/ModA/1.6"],
        );

        let parsed = parsed_with(vec![RawPatchFailure {
            mod_tag: "Totally Different Display Name".to_string(),
            operation: "Verse.PatchOperationAdd(...)".to_string(),
            source_file: Some(r"C:\Mods\ModA\1.6\Patches\a.xml".to_string()),
            stack_trace: None,
        }]);
        let use_case = ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)));

        let summary = use_case
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed");

        assert_eq!(
            summary.patch_failures[0].attribution,
            LogAttribution::Mod(ModId::new("fixture.moda")),
            "the file: path is under the mod's loaded_folders entry, so path attribution \
             must win even though the [Tag] display name doesn't match at all"
        );
    }

    #[test]
    fn attribution_prefers_the_longest_matching_path_between_two_real_candidates() {
        // Two *distinct* mods, one genuinely nested inside the other's own
        // path — a real install shape (`...\3000000002\Extra Mods\Example
        // Hygiene\...`: a workshop upload id folder containing an
        // unrelated mod's own subfolder). A single mod whose own `path` and
        // `loaded_folders[0]` both resolve to the same id would let an
        // inverted "longest wins" comparator (or just taking the first
        // match) pass too.
        let outer_path = "Q:/Steam/steamapps/workshop/content/294100/3000000000";
        let inner_path = "Q:/Steam/steamapps/workshop/content/294100/3000000000/Mods/Inner";
        let session = session_with_mods(&[
            ("pkg.outer", "Outer Pack", outer_path, &[]),
            ("pkg.inner", "Inner Mod", inner_path, &[inner_path]),
        ]);

        let parsed = parsed_with(vec![
            RawPatchFailure {
                mod_tag: "Unrelated Tag".to_string(),
                operation: "Verse.PatchOperationAdd(...)".to_string(),
                source_file: Some(format!(r"{}\Patches\a.xml", inner_path.replace('/', "\\"))),
                stack_trace: None,
            },
            RawPatchFailure {
                mod_tag: "Unrelated Tag".to_string(),
                operation: "Verse.PatchOperationAdd(...)".to_string(),
                source_file: Some(format!(
                    r"{}\SomeOtherFile.xml",
                    outer_path.replace('/', "\\")
                )),
                stack_trace: None,
            },
        ]);
        let use_case = ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)));

        let summary = use_case
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed");

        assert_eq!(
            summary.patch_failures[0].attribution,
            LogAttribution::Mod(ModId::new("pkg.inner")),
            "a file under the nested mod's own path must attribute to it, not the outer mod \
             whose path is also technically a (shorter) matching prefix"
        );
        assert_eq!(
            summary.patch_failures[1].attribution,
            LogAttribution::Mod(ModId::new("pkg.outer")),
            "a file outside the nested mod's own subfolder must still attribute to the outer mod"
        );
    }

    #[test]
    fn attribution_by_path_is_case_insensitive() {
        // Windows paths compare case-insensitively; RimWorld's own log
        // casing need not match the filesystem's stored casing. Deleting
        // `.to_lowercase()` from `normalize_path_text` passes the rest of
        // this suite silently (every other fixture writes both sides in
        // identical casing) and falls through to the *wrong* mod via the
        // display-name fallback instead of erroring — the
        // `[Example Biomes]` counterexample is exactly this failure
        // mode, so this must be pinned directly.
        // The mod's own real display name deliberately does *not* match
        // the terse line's own `[Tag]` — only path attribution can
        // resolve this correctly; a display-name-fallback false match
        // would silently paper over a broken case-insensitive compare.
        let session = session_with_mod_paths(
            "fixture.moda",
            "Real Mod Display Name",
            "Q:/Steam/Mods/ModA",
            &["Q:/Steam/Mods/ModA"],
        );
        let parsed = parsed_with(vec![RawPatchFailure {
            mod_tag: "Totally Different Tag".to_string(),
            operation: "Verse.PatchOperationAdd(...)".to_string(),
            source_file: Some(r"q:\steam\mods\moda\Patches\a.xml".to_string()),
            stack_trace: None,
        }]);
        let use_case = ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)));

        let summary = use_case
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed");

        assert_eq!(
            summary.patch_failures[0].attribution,
            LogAttribution::Mod(ModId::new("fixture.moda")),
            "a case-mismatched but otherwise identical path must still match"
        );
    }

    #[test]
    fn falls_back_to_the_display_name_when_no_path_matches() {
        let session = session_fixture(&["fixture.moda"]);
        // `session_fixture`'s plain mods have `name == id` and an empty
        // `path`/`loaded_folders` — no path can ever match.
        let parsed = parsed_with(vec![RawPatchFailure {
            mod_tag: "fixture.moda".to_string(),
            operation: "Verse.PatchOperationAdd(...)".to_string(),
            source_file: Some(r"C:\unrelated\path.xml".to_string()),
            stack_trace: None,
        }]);
        let use_case = ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)));

        let summary = use_case
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed");

        assert_eq!(
            summary.patch_failures[0].attribution,
            LogAttribution::Mod(ModId::new("fixture.moda"))
        );
    }

    #[test]
    fn a_tag_matching_no_active_mod_by_path_or_name_is_unattributed() {
        let session = session_fixture(&["fixture.moda"]);
        let parsed = parsed_with(vec![RawPatchFailure {
            mod_tag: "Some Mod Not In This Install".to_string(),
            operation: "Verse.PatchOperationAdd(...)".to_string(),
            source_file: Some(r"C:\unrelated\path.xml".to_string()),
            stack_trace: None,
        }]);
        let use_case = ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)));

        let summary = use_case
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed");

        assert_eq!(
            summary.patch_failures[0].attribution,
            LogAttribution::Unattributed("Some Mod Not In This Install".to_string()),
            "never dropped and never guessed at"
        );
    }

    #[test]
    fn a_stack_trace_leaf_and_enclosing_chain_survive_attribution_unchanged() {
        let session = session_fixture(&["fixture.moda"]);
        let parsed = parsed_with(vec![RawPatchFailure {
            mod_tag: "fixture.moda".to_string(),
            operation: "Verse.PatchOperationConditional(...)".to_string(),
            source_file: None,
            stack_trace: Some(StackTraceBlock {
                mod_tag: "fixture.moda".to_string(),
                ops: vec![
                    stack_op(
                        "Verse.PatchOperationAdd",
                        Some("Defs/ThingDef[defName=\"X\"]"),
                        "Failed to find a node with the given xpath",
                    ),
                    StackTraceOp {
                        branch: Some("nomatch".to_string()),
                        ..stack_op(
                            "Verse.PatchOperationConditional",
                            Some("Defs/ThingDef[defName=\"X\"]/researchPrerequisites"),
                            "Error in <nomatch>",
                        )
                    },
                ],
                source_file: Some("C:/Mods/ModA/a.xml".to_string()),
            }),
        }]);
        let use_case = ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)));

        let summary = use_case
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed");

        let stack = summary.patch_failures[0]
            .stack_trace
            .as_ref()
            .expect("must have a stack trace");
        assert_eq!(stack.leaf_class, "Verse.PatchOperationAdd");
        assert_eq!(
            stack.leaf_xpath.as_deref(),
            Some(r#"Defs/ThingDef[defName="X"]"#)
        );
        assert_eq!(stack.enclosing_chain.len(), 1);
        assert_eq!(stack.enclosing_chain[0].branch.as_deref(), Some("nomatch"));
    }

    fn raw_event(line: usize, kind: LoadEventKind, mods: &[&str]) -> RawLoadEvent {
        RawLoadEvent {
            line,
            kind,
            mods: mods.iter().map(ToString::to_string).collect(),
        }
    }

    fn summarize(events: Vec<RawLoadEvent>) -> GameLogSummary {
        let session = session_fixture(&["fixture.moda"]);
        let mut parsed = parsed_with(Vec::new());
        parsed.load_events = events;
        ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)))
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed")
    }

    #[test]
    fn the_class_tallies_and_separators_reach_the_summary_and_reconcile() {
        use crate::ports::{ClassTally, EntryClass, Family, FamilyKey};

        let session = session_fixture(&["fixture.moda"]);
        let mut parsed = parsed_with(Vec::new());
        parsed.read_stats.lines_read = 5;
        parsed.blank_separator_lines = 2;
        parsed.classes.insert(
            EntryClass::ModMessage,
            ClassTally {
                families: std::collections::BTreeMap::from([(
                    FamilyKey::new("[Tag] hello".to_string()),
                    Family {
                        count: 2,
                        lines: 3,
                        first_line: 1,
                        last_line: 4,
                        count_by_pass: std::collections::BTreeMap::new(),
                        severity: None,
                        sample: None,
                        attribution_input: None,
                    },
                )]),
                overflow: Default::default(),
            },
        );

        let summary = ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)))
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed");

        assert_eq!(summary.blank_separator_lines, 2);
        assert_eq!(summary.totals().entries, 2);
        assert!(summary.totals().is_conserved());
        assert!(!summary.has_losses());
    }

    #[test]
    fn a_class_overflow_counts_as_a_loss_of_the_summary() {
        use crate::ports::{ClassTally, EntryClass, Tally};

        let session = session_fixture(&["fixture.moda"]);
        let mut parsed = parsed_with(Vec::new());
        parsed.classes.insert(
            EntryClass::Unclassified,
            ClassTally {
                overflow: Tally {
                    entries: 1,
                    lines: 1,
                },
                ..ClassTally::default()
            },
        );

        let summary = ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)))
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed");

        assert!(summary.has_losses());
    }

    #[test]
    fn the_readers_statistics_reach_the_summary_unchanged() {
        let session = session_fixture(&["fixture.moda"]);
        let mut parsed = parsed_with(Vec::new());
        parsed.read_stats = LogReadStats {
            lines_read: 9,
            lines_truncated: 1,
            lines_with_invalid_utf8: 2,
            stack_block_lines_dropped: 3,
            stack_joins_abandoned: 4,
            passes_folded: 5,
            stack_refs_dropped: 6,
            logging_gaps_dropped: 7,
            crash_report_paths_truncated: 8,
        };
        let expected = parsed.read_stats;

        let summary = ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)))
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed");

        assert_eq!(summary.read_stats, expected);
    }

    #[test]
    fn load_events_are_normalized_into_mod_ids_keeping_kind_and_line() {
        let summary = summarize(vec![raw_event(
            7,
            LoadEventKind::SaveLoad {
                save_name: "SaveA".to_string(),
            },
            &["Example.EarlyLoader", "example.patchlib"],
        )]);

        assert_eq!(
            summary.load_events,
            vec![LoadEvent {
                line: 7,
                kind: LoadEventKind::SaveLoad {
                    save_name: "SaveA".to_string()
                },
                mods: vec![
                    ModId::new("example.earlyloader"),
                    ModId::new("example.patchlib")
                ],
            }]
        );
    }

    #[test]
    fn a_log_with_no_load_event_has_no_last_order_and_no_disagreement() {
        // "Can't compare orders" must stay distinct from "ran with zero mods".
        let summary = summarize(Vec::new());

        assert!(summary.load_events.is_empty());
        assert_eq!(summary.last_load_order(), None);
        assert!(!summary.load_events_disagree());
    }

    #[test]
    fn the_last_load_event_is_the_order_to_compare() {
        let summary = summarize(vec![
            raw_event(1, LoadEventKind::NewGame, &["a.mod"]),
            raw_event(
                9,
                LoadEventKind::SaveLoad {
                    save_name: "S".to_string(),
                },
                &["a.mod", "b.mod"],
            ),
        ]);

        assert_eq!(
            summary.last_load_order(),
            Some([ModId::new("a.mod"), ModId::new("b.mod")].as_slice())
        );
    }

    #[test]
    fn events_with_different_mod_lists_disagree() {
        let summary = summarize(vec![
            raw_event(1, LoadEventKind::NewGame, &["a.mod", "b.mod"]),
            raw_event(5, LoadEventKind::NewGame, &["b.mod", "a.mod"]),
            raw_event(9, LoadEventKind::NewGame, &["a.mod", "b.mod"]),
        ]);

        assert!(summary.load_events_disagree());
    }

    #[test]
    fn events_with_the_same_mod_list_do_not_disagree() {
        let summary = summarize(vec![
            raw_event(1, LoadEventKind::NewGame, &["a.mod", "b.mod"]),
            raw_event(5, LoadEventKind::NewGame, &["A.Mod", "b.mod"]),
        ]);

        assert!(!summary.load_events_disagree());
    }

    #[test]
    fn cross_references_pass_through_unattributed_verbatim() {
        let session = session_fixture(&["fixture.moda"]);
        let mut parsed = parsed_with(Vec::new());
        parsed.cross_references = vec![RawCrossReference::Wanter {
            missing_type: "Verse.ThingDef".to_string(),
            missing_name: "X".to_string(),
            wanter_field: "requiredBuildings".to_string(),
        }];
        let use_case = ImportGameLog::new(FakeGameLogReader::new(Ok(parsed.clone())));

        let summary = use_case
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed");

        assert_eq!(summary.cross_references, parsed.cross_references);
    }

    #[test]
    fn a_read_failure_surfaces_unchanged_through_the_use_case() {
        let session = session_fixture(&["fixture.moda"]);
        let use_case = ImportGameLog::new(FakeGameLogReader::new(Err(GameLogError(
            "cannot open the log".to_string(),
        ))));

        let error = use_case
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect_err("a read failure must not be swallowed");

        assert!(matches!(error, ImportGameLogError::Read(_)));
        assert!(error.to_string().contains("cannot open the log"));
    }

    /// Records the [`KindChoice`] each read was asked for.
    struct KindRecordingReader {
        asked: std::cell::RefCell<Vec<KindChoice>>,
    }

    impl GameLogReader for KindRecordingReader {
        fn read(
            &self,
            _path: &Path,
            _formats: &crate::ports::LogFormats<'_>,
            kind: KindChoice,
        ) -> Result<crate::ports::ParsedGameLog, GameLogError> {
            self.asked.borrow_mut().push(kind);
            Ok(crate::ports::ParsedGameLog::default())
        }
    }

    #[test]
    fn the_callers_kind_choice_reaches_the_reader() {
        let session = session_fixture(&["fixture.moda"]);
        let reader = KindRecordingReader {
            asked: std::cell::RefCell::default(),
        };
        let use_case = ImportGameLog::new(reader);

        for choice in [
            KindChoice::Detect,
            KindChoice::Force(LogKind::ConsoleSnapshot),
        ] {
            use_case
                .execute(&session, Path::new("x.txt"), choice)
                .expect("must succeed");
        }

        assert_eq!(
            *use_case.reader.asked.borrow(),
            vec![
                KindChoice::Detect,
                KindChoice::Force(LogKind::ConsoleSnapshot)
            ]
        );
    }

    fn family_spanning(first_line: u64, last_line: u64) -> crate::ports::Family {
        crate::ports::Family {
            count: 2,
            lines: 2,
            first_line,
            last_line,
            count_by_pass: BTreeMap::new(),
            severity: None,
            sample: None,
            attribution_input: None,
        }
    }

    fn summary_with_gap_and_families(gap: LoggingGap) -> GameLogSummary {
        use crate::ports::{ClassTally, FamilyKey};

        let session = session_fixture(&["fixture.moda"]);
        let mut parsed = parsed_with(Vec::new());
        parsed.logging_gaps = vec![gap];
        let families = [
            ("inside", family_spanning(150, 160)),
            ("before", family_spanning(10, 20)),
        ];
        parsed.classes.insert(
            EntryClass::ModMessage,
            ClassTally {
                families: families
                    .into_iter()
                    .map(|(key, family)| (FamilyKey::new(key.to_string()), family))
                    .collect(),
                ..ClassTally::default()
            },
        );
        parsed.classes.insert(
            EntryClass::EngineInfo(EngineInfoKind::LoggingStopped),
            ClassTally {
                families: BTreeMap::from([(
                    FamilyKey::new("stop".to_string()),
                    family_spanning(100, 300),
                )]),
                ..ClassTally::default()
            },
        );
        ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)))
            .execute(&session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed")
    }

    #[test]
    fn a_family_overlapping_a_gap_is_a_lower_bound_and_the_gap_markers_are_exact() {
        let summary = summary_with_gap_and_families(LoggingGap {
            stop_line: 100,
            resume: crate::ports::GapEnd::Resumed { line: 200 },
        });

        let keys: Vec<_> = summary
            .lower_bound_families()
            .into_iter()
            .map(|(class, key)| (class, key.as_str().to_string()))
            .collect();

        assert_eq!(keys, vec![(EntryClass::ModMessage, "inside".to_string())]);
    }

    #[test]
    fn the_lower_bound_entries_are_those_of_the_families_spanning_a_gap() {
        let summary = summary_with_gap_and_families(LoggingGap {
            stop_line: 100,
            resume: crate::ports::GapEnd::Resumed { line: 200 },
        });

        assert_eq!(summary.lower_bound_entries(), 2, "only `inside` counts");
        assert_eq!(summarize(Vec::new()).lower_bound_entries(), 0);
    }

    #[test]
    fn only_a_player_log_states_startup_passes() {
        use crate::ports::{PlayerLogCoverage, SnapshotCoverage};

        let session = session_fixture(&["fixture.moda"]);
        let summarize_with = |coverage| {
            let mut parsed = parsed_with(Vec::new());
            parsed.coverage = coverage;
            ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)))
                .execute(&session, Path::new("x.txt"), KindChoice::Detect)
                .expect("must succeed")
        };

        let player_log = summarize_with(LogCoverage::PlayerLog(PlayerLogCoverage {
            passes: 2,
            ..PlayerLogCoverage::default()
        }));
        let snapshot = summarize_with(LogCoverage::ConsoleSnapshot(SnapshotCoverage {
            entries: 4,
            head_truncated: false,
        }));

        assert_eq!(player_log.startup_passes(), Some(2));
        assert_eq!(snapshot.startup_passes(), None);
    }

    #[test]
    fn a_summary_with_a_logging_gap_has_losses_and_one_without_has_none() {
        let gapped = summary_with_gap_and_families(LoggingGap {
            stop_line: 100,
            resume: crate::ports::GapEnd::NeverResumed,
        });
        let clean = summarize(Vec::new());

        assert!(gapped.has_losses());
        assert!(!clean.has_losses());
        assert!(clean.lower_bound_families().is_empty());
    }

    #[test]
    fn a_snapshot_at_the_console_cap_has_losses_and_one_below_it_does_not() {
        use crate::ports::SnapshotCoverage;

        let session = session_fixture(&["fixture.moda"]);
        let summarize_snapshot = |entries| {
            let mut parsed = parsed_with(Vec::new());
            parsed.coverage = LogCoverage::ConsoleSnapshot(SnapshotCoverage {
                entries,
                head_truncated: false,
            });
            ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)))
                .execute(&session, Path::new("x.txt"), KindChoice::Detect)
                .expect("must succeed")
        };

        assert!(summarize_snapshot(1_000).has_losses());
        assert!(!summarize_snapshot(999).has_losses());
        assert!(
            !summarize_snapshot(1_001).has_losses(),
            "over the cap is reported as its own state, not as a known loss"
        );
        assert_eq!(summarize_snapshot(999).kind(), LogKind::ConsoleSnapshot);
    }

    fn timer(label: &str) -> RawTimer {
        RawTimer {
            label: label.to_string(),
            milliseconds: 5,
            pass: 1,
        }
    }

    #[test]
    fn a_timer_is_attributed_by_the_balanced_leading_tag_not_the_last_bracket() {
        let session = session_with_mod_paths("fixture.moda", "Mod A", "C:/A", &[]);
        let mut parsed = parsed_with(Vec::new());
        parsed.timers = vec![
            timer("[Mod A] patched 3 [defs] in"),
            timer("[Mod A] patched ]"),
            timer("no tag at all"),
        ];

        let summary = summarize_parsed(&session, parsed);

        assert_eq!(summary.timers[0].attribution, owner_of("fixture.moda"));
        assert_eq!(
            summary.timers[1].attribution,
            owner_of("fixture.moda"),
            "a stray closing bracket later in the label must not stretch the tag"
        );
        assert_eq!(
            summary.timers[2].attribution,
            LogAttribution::Unattributed("no tag at all".to_string())
        );
    }

    #[test]
    fn a_mod_folder_listed_with_a_trailing_separator_still_owns_its_files() {
        let session = session_with_mods(&[
            ("pkg.outer", "Outer Pack", "C:/Mods/Pack", &[]),
            (
                "pkg.inner",
                "Inner Mod",
                "C:/Mods/Pack/Inner/",
                &["C:/Mods/Pack/Inner/"],
            ),
        ]);
        let parsed = parsed_with(vec![RawPatchFailure {
            mod_tag: "Nobody".to_string(),
            operation: "op".to_string(),
            source_file: Some(r"C:\Mods\Pack\Inner\Patches\a.xml".to_string()),
            stack_trace: None,
        }]);

        let summary = summarize_parsed(&session, parsed);

        assert_eq!(
            summary.patch_failures[0].attribution,
            owner_of("pkg.inner"),
            "the trailing '/' must not hide the inner folder behind the outer mod's"
        );
    }

    // -- family attribution ---------------------------------------------------

    fn input_family(input: Option<AttributionInput>) -> Family {
        Family {
            count: 5,
            lines: 5,
            first_line: 1,
            last_line: 9,
            count_by_pass: BTreeMap::new(),
            severity: None,
            sample: None,
            attribution_input: input,
        }
    }

    fn key(text: &str) -> FamilyKey {
        FamilyKey::new(text.to_string())
    }

    /// One class holding one family per `(key, input)`.
    fn parsed_with_class(
        class: EntryClass,
        families: Vec<(&str, Option<AttributionInput>)>,
    ) -> crate::ports::ParsedGameLog {
        let mut parsed = parsed_with(Vec::new());
        parsed.classes.insert(
            class,
            ClassTally {
                families: families
                    .into_iter()
                    .map(|(text, input)| (key(text), input_family(input)))
                    .collect(),
                overflow: Tally::default(),
            },
        );
        parsed
    }

    fn summarize_parsed(session: &Session, parsed: crate::ports::ParsedGameLog) -> GameLogSummary {
        ImportGameLog::new(FakeGameLogReader::new(Ok(parsed)))
            .execute(session, Path::new("Player.log"), KindChoice::Detect)
            .expect("must succeed")
    }

    fn attribution_of<'a>(
        summary: &'a GameLogSummary,
        class: EntryClass,
        family: &str,
    ) -> Option<&'a FamilyAttribution> {
        summary.classes.get(&class)?.attribution.get(&key(family))
    }

    fn sources_shipping(assemblies: &[(&str, &[&str])]) -> SourceIndex {
        SourceIndex {
            assembly_owners: assemblies
                .iter()
                .map(|(name, owners)| {
                    ((*name).to_string(), owners.iter().map(ModId::new).collect())
                })
                .collect(),
            ..SourceIndex::default()
        }
    }

    fn owner(id: &str) -> FamilyAttribution {
        FamilyAttribution::Mod(ModId::new(id))
    }

    fn owner_of(id: &str) -> LogAttribution {
        LogAttribution::Mod(ModId::new(id))
    }

    fn unattributed(raw: &str) -> FamilyAttribution {
        FamilyAttribution::Unattributed {
            raw: raw.to_string(),
        }
    }

    fn ambiguous(raw: &str, candidates: &[&str]) -> FamilyAttribution {
        FamilyAttribution::Ambiguous {
            raw: raw.to_string(),
            candidates: candidates.iter().map(|id| ModId::new(*id)).collect(),
        }
    }

    fn type_family(name: &str) -> Option<AttributionInput> {
        AttributionInput::type_name(name)
    }

    #[test]
    fn a_display_name_family_is_attributed_to_the_mod_of_that_name() {
        let session = session_with_mod_paths("fixture.moda", "Real Mod Name", "C:/Mods/A", &[]);
        let parsed = parsed_with_class(
            EntryClass::ModMessage,
            vec![
                (
                    "known",
                    Some(AttributionInput::display_name("real mod name")),
                ),
                (
                    "unknown",
                    Some(AttributionInput::display_name("Not Installed")),
                ),
            ],
        );

        let summary = summarize_parsed(&session, parsed);

        assert_eq!(
            attribution_of(&summary, EntryClass::ModMessage, "known"),
            Some(&owner("fixture.moda"))
        );
        assert_eq!(
            attribution_of(&summary, EntryClass::ModMessage, "unknown"),
            Some(&unattributed("Not Installed")),
            "an unknown tag stays unattributed, with its raw text"
        );
    }

    fn tag_family(tag: &str) -> Option<AttributionInput> {
        Some(AttributionInput::display_name(tag))
    }

    fn mod_message_attribution(session: &Session, tags: &[&str]) -> Vec<Option<FamilyAttribution>> {
        let families = tags.iter().map(|tag| (*tag, tag_family(tag))).collect();
        let summary =
            summarize_parsed(session, parsed_with_class(EntryClass::ModMessage, families));
        tags.iter()
            .map(|tag| attribution_of(&summary, EntryClass::ModMessage, tag).cloned())
            .collect()
    }

    #[test]
    fn an_exact_display_name_still_wins_over_a_package_id_or_compact_match() {
        let session = session_with_mods(&[
            ("fixture.exact", "Example Mod", "C:/Mods/A", &[]),
            ("example.mod", "Other Name", "C:/Mods/B", &[]),
            ("fixture.compact", "Examplemod", "C:/Mods/C", &[]),
        ]);

        let result = mod_message_attribution(&session, &["Example Mod"]);

        assert_eq!(result, vec![Some(owner("fixture.exact"))]);
    }

    #[test]
    fn a_tag_without_spaces_or_punctuation_matches_the_display_name() {
        let session = session_with_mod_paths("fixture.moda", "Example Mod", "C:/Mods/A", &[]);

        let result = mod_message_attribution(&session, &["ExampleMod", "example-mod"]);

        assert_eq!(
            result,
            vec![Some(owner("fixture.moda")), Some(owner("fixture.moda"))]
        );
    }

    #[test]
    fn a_tag_naming_a_package_id_matches_that_mod_through_the_steam_suffix() {
        let session = session_with_mods(&[
            ("example.shipper_steam", "Unrelated Title", "C:/Mods/A", &[]),
            ("fixture.modb", "Mod B", "C:/Mods/B", &[]),
        ]);

        let result = mod_message_attribution(&session, &["Example.Shipper"]);

        assert_eq!(result, vec![Some(owner("example.shipper_steam"))]);
    }

    #[test]
    fn a_compact_name_two_mods_share_attributes_nothing_and_lists_both_candidates() {
        let session = session_with_mods(&[
            ("fixture.moda", "Example-Mod", "C:/Mods/A", &[]),
            ("fixture.modb", "Example Mod", "C:/Mods/B", &[]),
        ]);

        let result = mod_message_attribution(&session, &["ExampleMod"]);

        assert_eq!(
            result,
            vec![Some(ambiguous(
                "ExampleMod",
                &["fixture.moda", "fixture.modb"]
            ))]
        );
    }

    #[test]
    fn a_name_with_a_non_latin_prefix_matches_its_latin_tag() {
        let session = session_with_mod_paths(
            "fixture.moda",
            "\u{5E7C}\u{866B} Example Mod",
            "C:/Mods/A",
            &[],
        );

        let result = mod_message_attribution(&session, &["ExampleMod"]);

        assert_eq!(result, vec![Some(owner("fixture.moda"))]);
    }

    #[test]
    fn a_tag_that_only_punctuation_remains_of_matches_nothing() {
        let session = session_with_mod_paths("fixture.moda", "!!!", "C:/Mods/A", &[]);

        let result = mod_message_attribution(&session, &["???"]);

        assert_eq!(result, vec![Some(unattributed("???"))]);
    }

    #[test]
    fn a_timer_with_an_ambiguous_compact_tag_is_unattributed() {
        let session = session_with_mods(&[
            ("fixture.moda", "Example-Mod", "C:/Mods/A", &[]),
            ("fixture.modb", "Example Mod", "C:/Mods/B", &[]),
        ]);
        let mut parsed = parsed_with(Vec::new());
        parsed.timers = vec![timer("[ExampleMod] tick")];

        let summary = summarize_parsed(&session, parsed);

        assert_eq!(
            summary.timers[0].attribution,
            LogAttribution::Unattributed("[ExampleMod] tick".to_string())
        );
    }

    #[test]
    fn the_fallback_attribution_is_identical_across_runs() {
        let session = session_with_mods(&[
            ("fixture.moda", "Example-Mod", "C:/Mods/A", &[]),
            ("fixture.modb", "Example Mod", "C:/Mods/B", &[]),
            ("fixture.modc", "Third", "C:/Mods/C", &[]),
        ]);
        let tags = ["ExampleMod", "Third", "fixture.modc", "Nobody"];

        let first = mod_message_attribution(&session, &tags);
        let second = mod_message_attribution(&session, &tags);

        assert_eq!(first, second);
    }

    #[test]
    fn a_name_and_path_family_prefers_the_path_and_falls_back_to_the_name() {
        let session = session_with_mods(&[
            ("fixture.moda", "Mod A", "C:/Mods/A", &["C:/Mods/A/1.6"]),
            ("fixture.modb", "Mod B", "C:/Mods/B", &[]),
        ]);
        let by_path =
            AttributionInput::display_name_and_path("Mod B", r"C:\Mods\A\1.6\Patches\x.xml");
        let by_name = AttributionInput::display_name_and_path("Mod B", r"C:\Other\x.xml");
        let neither = AttributionInput::display_name_and_path("Nobody", r"C:\Other\x.xml");
        let parsed = parsed_with_class(
            EntryClass::PatchFailure,
            vec![
                ("by_path", Some(by_path)),
                ("by_name", Some(by_name)),
                ("neither", Some(neither)),
            ],
        );

        let summary = summarize_parsed(&session, parsed);

        let attributions = |family| attribution_of(&summary, EntryClass::PatchFailure, family);
        assert_eq!(attributions("by_path"), Some(&owner("fixture.moda")));
        assert_eq!(attributions("by_name"), Some(&owner("fixture.modb")));
        assert_eq!(attributions("neither"), Some(&unattributed("Nobody")));
    }

    #[test]
    fn a_path_family_is_attributed_by_folder_and_a_path_outside_every_mod_is_not() {
        let session = session_with_mod_paths("fixture.moda", "Mod A", "C:/Mods/A", &[]);
        let parsed = parsed_with_class(
            EntryClass::TextureFallback,
            vec![
                (
                    "inside",
                    Some(AttributionInput::path(r"c:\mods\a\Textures\t.dds")),
                ),
                (
                    "outside",
                    Some(AttributionInput::path(r"C:\Mods\AB\Textures\t.dds")),
                ),
            ],
        );

        let summary = summarize_parsed(&session, parsed);

        assert_eq!(
            attribution_of(&summary, EntryClass::TextureFallback, "inside"),
            Some(&owner("fixture.moda"))
        );
        assert_eq!(
            attribution_of(&summary, EntryClass::TextureFallback, "outside"),
            Some(&unattributed(r"C:\Mods\AB\Textures\t.dds")),
            "a sibling folder sharing a name prefix is not the mod's folder"
        );
    }

    fn two_mods_with_sources(sources: SourceIndex) -> Session {
        session_with_mods_and_sources(
            &[
                ("fixture.moda", "Mod A", "C:/A", &[]),
                ("fixture.modb", "Mod B", "C:/B", &[]),
            ],
            sources,
        )
    }

    #[test]
    fn a_type_family_is_attributed_to_the_sole_owner_of_its_longest_matching_assembly() {
        let sources = sources_shipping(&[
            ("example.alib", &["fixture.moda"]),
            ("example.blib", &["fixture.modb"]),
        ]);
        let session = two_mods_with_sources(sources);
        let parsed = parsed_with_class(
            EntryClass::RuntimeException,
            vec![
                ("a", type_family("Example.ALib.Deep.Type")),
                ("b", type_family("Example.BLib.Type")),
            ],
        );

        let summary = summarize_parsed(&session, parsed);

        let attributions = |family| attribution_of(&summary, EntryClass::RuntimeException, family);
        assert_eq!(attributions("a"), Some(&owner("fixture.moda")));
        assert_eq!(attributions("b"), Some(&owner("fixture.modb")));
    }

    #[test]
    fn a_type_no_active_assembly_owns_is_unattributed_naming_the_type() {
        let sources = sources_shipping(&[("example.alib", &["fixture.moda"])]);
        let session = two_mods_with_sources(sources);
        let parsed = parsed_with_class(
            EntryClass::RuntimeException,
            vec![
                ("unowned", type_family("Unknown.Ns.Type")),
                ("no_namespace", type_family("Bare")),
                ("no_frames", None),
            ],
        );

        let summary = summarize_parsed(&session, parsed);

        let attributions = |family| attribution_of(&summary, EntryClass::RuntimeException, family);
        assert_eq!(
            attributions("unowned"),
            Some(&unattributed("Unknown.Ns.Type"))
        );
        assert_eq!(attributions("no_namespace"), Some(&unattributed("Bare")));
        assert_eq!(attributions("no_frames"), None, "no evidence, no row");
    }

    #[test]
    fn a_type_in_a_dll_several_mods_ship_is_ambiguous_with_every_candidate() {
        let sources = sources_shipping(&[
            ("example.shared", &["fixture.moda", "fixture.modb"]),
            // A shorter, unique prefix must not take over from the ambiguous
            // longest one.
            ("example", &["fixture.moda"]),
        ]);
        let session = two_mods_with_sources(sources);
        let parsed = parsed_with_class(
            EntryClass::RuntimeException,
            vec![("shared", type_family("Example.Shared.Type"))],
        );

        let summary = summarize_parsed(&session, parsed);

        assert_eq!(
            attribution_of(&summary, EntryClass::RuntimeException, "shared"),
            Some(&ambiguous(
                "Example.Shared.Type",
                &["fixture.moda", "fixture.modb"]
            ))
        );
    }

    #[test]
    fn an_engine_type_is_never_owned_even_by_a_mod_shipping_a_like_named_assembly() {
        let session = two_mods_with_sources(sources_shipping(&[("verse", &["fixture.moda"])]));
        let parsed = parsed_with_class(
            EntryClass::RuntimeException,
            vec![("engine", type_family("Verse.Pawn"))],
        );

        let summary = summarize_parsed(&session, parsed);

        assert_eq!(
            attribution_of(&summary, EntryClass::RuntimeException, "engine"),
            Some(&unattributed("Verse.Pawn"))
        );
    }

    #[test]
    fn an_assembly_family_reads_the_exact_assembly_name() {
        let sources = sources_shipping(&[
            ("example.alib", &["fixture.moda"]),
            ("example.shared", &["fixture.moda", "fixture.modb"]),
        ]);
        let session = two_mods_with_sources(sources);
        let parsed = parsed_with_class(
            EntryClass::TypeLoadError,
            vec![
                ("sole", AttributionInput::assembly("Example.ALib")),
                ("shared", AttributionInput::assembly("Example.Shared")),
                ("none", AttributionInput::assembly("Nowhere")),
            ],
        );

        let summary = summarize_parsed(&session, parsed);

        let attributions = |family| attribution_of(&summary, EntryClass::TypeLoadError, family);
        assert_eq!(attributions("sole"), Some(&owner("fixture.moda")));
        assert_eq!(
            attributions("shared"),
            Some(&ambiguous(
                "Example.Shared",
                &["fixture.moda", "fixture.modb"]
            ))
        );
        assert_eq!(attributions("none"), Some(&unattributed("Nowhere")));
    }

    #[test]
    fn a_steam_suffixed_mod_keeps_its_report_id_when_a_stack_names_its_assembly() {
        let sources = sources_shipping(&[("example.alib", &["fixture.moda_steam"])]);
        let session =
            session_with_mods_and_sources(&[("fixture.moda_steam", "Mod A", "C:/A", &[])], sources);
        let parsed = parsed_with_class(
            EntryClass::RuntimeException,
            vec![("f", type_family("Example.ALib.Type"))],
        );

        let summary = summarize_parsed(&session, parsed);

        let Some(FamilyAttribution::Mod(id)) =
            attribution_of(&summary, EntryClass::RuntimeException, "f")
        else {
            panic!("the stack must attribute to the mod");
        };
        assert_eq!(
            id,
            &ModId::new("fixture.moda_steam"),
            "the raw report id, so a lookup by it finds the mod"
        );
        assert_eq!(id.base(), ModId::new("fixture.moda"));
    }

    #[test]
    fn a_family_without_evidence_has_no_row_and_attribution_is_per_family_not_per_entry() {
        let session = session_with_mod_paths("fixture.moda", "Mod A", "C:/A", &[]);
        let mut parsed = parsed_with_class(EntryClass::CrossReference, vec![("no evidence", None)]);
        parsed.classes.insert(
            EntryClass::ModMessage,
            ClassTally {
                families: BTreeMap::from([(
                    key("[Mod A] storm"),
                    Family {
                        count: 250_000,
                        ..input_family(Some(AttributionInput::display_name("Mod A")))
                    },
                )]),
                overflow: Tally::default(),
            },
        );

        let summary = summarize_parsed(&session, parsed);

        assert!(
            summary.classes[&EntryClass::CrossReference]
                .attribution
                .is_empty()
        );
        let storm = &summary.classes[&EntryClass::ModMessage];
        assert_eq!(
            storm.attribution,
            BTreeMap::from([(key("[Mod A] storm"), owner("fixture.moda"))]),
            "one resolved row for the whole 250,000-entry family"
        );
        assert_eq!(storm.tally.entries(), 250_000);
    }

    #[test]
    fn family_attribution_is_deterministic_and_leaves_the_tally_unchanged() {
        let session = session_with_mod_paths("fixture.moda", "Mod A", "C:/A", &[]);
        let parsed = parsed_with_class(
            EntryClass::ModMessage,
            vec![
                ("b", Some(AttributionInput::display_name("Mod A"))),
                ("a", Some(AttributionInput::path("C:/A/x"))),
            ],
        );
        let expected_tally = parsed.classes[&EntryClass::ModMessage].clone();

        let first = summarize_parsed(&session, parsed.clone());
        let second = summarize_parsed(&session, parsed);

        assert_eq!(first, second);
        assert_eq!(first.classes[&EntryClass::ModMessage].tally, expected_tally);
    }

    #[test]
    fn is_under_requires_a_real_path_boundary() {
        assert!(attribution::is_under("mods/foo/patches/a.xml", "mods/foo"));
        assert!(!attribution::is_under(
            "mods/foobar/patches/a.xml",
            "mods/foo"
        ));
        assert!(!attribution::is_under("mods/foo", ""));
    }
}
