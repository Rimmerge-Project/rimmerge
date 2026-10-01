//! DTOs for `import_game_log`: mirrors
//! [`rim_session::use_cases::GameLogSummary`] one field/variant at a
//! time — camelCase here per this crate's own DTO convention, snake_case
//! in `apps/cli`'s own `log import --json`.
//!
//! **This DTO carries no predicted-vs-observed join of its own.** The predicted-vs-observed
//! join (`(ModId, normalize_log_text(operation))` against the *grouped*
//! `VerifyOperationDto`, never a raw `Finding`) is computed entirely
//! client-side, in `ApplyDialog.vue`/`StartupPage.vue`, from this DTO's
//! own `patchFailures`/`extraStackTraces` plus a
//! [`super::verify::VerifyReportDto`] already in hand — there is no
//! session-side cache of either to join server-side, and building one
//! here would duplicate `rim_resolve::domain::normalize_log_text`'s own
//! whitespace-collapse rule for no benefit.

use rim_session::ports::{LoadEventKind, LogReadStats, RawCrossReference};
use rim_session::use_cases::{
    DdsFailureSummary, DependencyWarningSummary, EnclosingOp, GameLogSummary, LoadEvent,
    LogAttribution, PatchFailureSummary, StackTraceDetail, TimerSummary, UnpairedStackTrace,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::game_log_coverage::{LogCoverageDto, LoggingGapDto, LowerBoundDto};

/// Mirrors [`LogAttribution`] — which active mod (if any) a raw log
/// reference resolved to. `Unattributed` keeps the raw text so the user
/// can still see what the log said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum LogAttributionDto {
    /// See [`LogAttribution::Mod`].
    #[serde(rename_all = "camelCase")]
    Mod {
        /// The active mod this raw reference resolved to.
        mod_id: String,
    },
    /// See [`LogAttribution::Unattributed`].
    #[serde(rename_all = "camelCase")]
    Unattributed {
        /// The raw text the log carried, verbatim.
        raw: String,
    },
}

impl From<&LogAttribution> for LogAttributionDto {
    fn from(value: &LogAttribution) -> Self {
        match value {
            LogAttribution::Mod(mod_id) => Self::Mod {
                mod_id: mod_id.as_str().to_string(),
            },
            LogAttribution::Unattributed(raw) => Self::Unattributed { raw: raw.clone() },
        }
    }
}

/// Mirrors [`EnclosingOp`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct EnclosingOpDto {
    /// The operation's own class, e.g. `Verse.PatchOperationAdd`.
    pub class: String,
    /// The raw parenthesized text, verbatim, when present.
    pub detail: Option<String>,
    /// Why the fold stopped here.
    pub reason: String,
    /// The branch marker, when present.
    pub branch: Option<String>,
}

impl From<&EnclosingOp> for EnclosingOpDto {
    fn from(value: &EnclosingOp) -> Self {
        Self {
            class: value.class.clone(),
            detail: value.detail.clone(),
            reason: value.reason.clone(),
            branch: value.branch.clone(),
        }
    }
}

/// Mirrors [`StackTraceDetail`] — the leaf op that actually failed, plus
/// every enclosing operation, innermost first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct StackTraceDetailDto {
    /// The leaf's own class.
    pub leaf_class: String,
    /// The leaf's own xpath, when its detail was one.
    pub leaf_xpath: Option<String>,
    /// The leaf's own failure reason.
    pub leaf_reason: String,
    /// Every enclosing operation, innermost first, outermost last.
    pub enclosing_chain: Vec<EnclosingOpDto>,
}

impl From<&StackTraceDetail> for StackTraceDetailDto {
    fn from(value: &StackTraceDetail) -> Self {
        Self {
            leaf_class: value.leaf_class.clone(),
            leaf_xpath: value.leaf_xpath.clone(),
            leaf_reason: value.leaf_reason.clone(),
            enclosing_chain: value.enclosing_chain.iter().map(Into::into).collect(),
        }
    }
}

/// Mirrors [`PatchFailureSummary`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct PatchFailureSummaryDto {
    /// The mod this failure is attributed to.
    pub attribution: LogAttributionDto,
    /// The top-level operation's own RimWorld-log identity text — the
    /// predicted-vs-observed join key against a [`super::verify::VerifyOperationDto`],
    /// after both sides go through `normalizeLogText`
    /// (`utils/gameLog.ts`, mirroring
    /// `rim_resolve::domain::normalize_log_text`).
    pub operation: String,
    /// The `file:` line's path.
    pub source_file: Option<String>,
    /// The richer stack-trace record, when one was paired with this
    /// failure.
    pub stack_trace: Option<StackTraceDetailDto>,
}

impl From<&PatchFailureSummary> for PatchFailureSummaryDto {
    fn from(value: &PatchFailureSummary) -> Self {
        Self {
            attribution: (&value.attribution).into(),
            operation: value.operation.clone(),
            source_file: value.source_file.clone(),
            stack_trace: value.stack_trace.as_ref().map(Into::into),
        }
    }
}

/// Mirrors [`UnpairedStackTrace`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct UnpairedStackTraceDto {
    /// The mod this block is attributed to.
    pub attribution: LogAttributionDto,
    /// The block's own trailer source-file path.
    pub source_file: Option<String>,
    /// The block's own leaf/chain detail — `None` only when none of its
    /// own lines could be parsed as an operation.
    pub detail: Option<StackTraceDetailDto>,
}

impl From<&UnpairedStackTrace> for UnpairedStackTraceDto {
    fn from(value: &UnpairedStackTrace) -> Self {
        Self {
            attribution: (&value.attribution).into(),
            source_file: value.source_file.clone(),
            detail: value.detail.as_ref().map(Into::into),
        }
    }
}

/// Mirrors [`RawCrossReference`] — two genuinely different renderings, both
/// parsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum RawCrossReferenceDto {
    /// See [`RawCrossReference::Wanter`].
    #[serde(rename_all = "camelCase")]
    Wanter {
        /// The type that couldn't be resolved.
        missing_type: String,
        /// The name that couldn't be resolved.
        missing_name: String,
        /// The field on the wanting def/comp that named it.
        wanter_field: String,
    },
    /// See [`RawCrossReference::WantingDef`].
    #[serde(rename_all = "camelCase")]
    WantingDef {
        /// The type that couldn't be resolved.
        missing_type: String,
        /// The name that couldn't be resolved.
        missing_name: String,
        /// The def that wanted it, e.g. `Verse.Tool fist`.
        wanting_def: String,
        /// The trailing parenthetical, when present.
        note: Option<String>,
    },
}

impl From<&RawCrossReference> for RawCrossReferenceDto {
    fn from(value: &RawCrossReference) -> Self {
        match value {
            RawCrossReference::Wanter {
                missing_type,
                missing_name,
                wanter_field,
            } => Self::Wanter {
                missing_type: missing_type.clone(),
                missing_name: missing_name.clone(),
                wanter_field: wanter_field.clone(),
            },
            RawCrossReference::WantingDef {
                missing_type,
                missing_name,
                wanting_def,
                note,
            } => Self::WantingDef {
                missing_type: missing_type.clone(),
                missing_name: missing_name.clone(),
                wanting_def: wanting_def.clone(),
                note: note.clone(),
            },
        }
    }
}

/// Mirrors [`DdsFailureSummary`] — the sole source for the startup
/// page's bad-dimension-DDS column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DdsFailureSummaryDto {
    /// The mod this failure is attributed to.
    pub attribution: LogAttributionDto,
    /// The texture file's own path, as logged.
    pub path: String,
    /// The texture's width in pixels.
    pub width: u32,
    /// The texture's height in pixels.
    pub height: u32,
    /// The compressed format, e.g. `DXT1`/`BC7`.
    pub format: String,
}

impl From<&DdsFailureSummary> for DdsFailureSummaryDto {
    fn from(value: &DdsFailureSummary) -> Self {
        Self {
            attribution: (&value.attribution).into(),
            path: value.path.clone(),
            width: value.width,
            height: value.height,
            format: value.format.clone(),
        }
    }
}

/// Mirrors [`DependencyWarningSummary`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DependencyWarningSummaryDto {
    /// The dependent mod this warning is attributed to.
    pub attribution: LogAttributionDto,
    /// The missing dependency's own package id.
    pub dependency_id: String,
}

impl From<&DependencyWarningSummary> for DependencyWarningSummaryDto {
    fn from(value: &DependencyWarningSummary) -> Self {
        Self {
            attribution: (&value.attribution).into(),
            dependency_id: value.dependency_id.clone(),
        }
    }
}

/// Mirrors [`TimerSummary`]. `milliseconds` carries `#[ts(type =
/// "number")]` — a load-time duration never remotely approaches
/// `Number.MAX_SAFE_INTEGER`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct TimerSummaryDto {
    /// The mod this timer is attributed to.
    pub attribution: LogAttributionDto,
    /// The timer's own context label.
    pub label: String,
    /// The timed duration, in whole milliseconds.
    #[ts(type = "number")]
    pub milliseconds: u64,
    /// The startup pass the timer was logged in (`0` before the first
    /// `RimWorld <version>` banner).
    pub pass: u32,
}

impl From<&TimerSummary> for TimerSummaryDto {
    fn from(value: &TimerSummary) -> Self {
        Self {
            attribution: (&value.attribution).into(),
            label: value.label.clone(),
            milliseconds: value.milliseconds,
            pass: value.pass,
        }
    }
}

/// `import_game_log`'s request shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ImportGameLogRequestDto {
    /// The log path the user picked: a `Player.log` or a console snapshot
    /// (the kind is detected from content). Read-only: never modified,
    /// moved, or copied.
    pub path: String,
}

/// Mirrors [`GameLogSummary`] — everything `rim_io::game_log::parse`
/// (via `rim_session::use_cases::ImportGameLog`) recovers from a
/// `Player.log`, attributed against the session's own active-mod list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GameLogSummaryDto {
    /// Every terse patch failure, attributed, with its `file:` line and
    /// (when found) its own richer stack-trace detail.
    pub patch_failures: Vec<PatchFailureSummaryDto>,
    /// Stack-trace blocks left over with no terse line paired to them.
    pub extra_stack_traces: Vec<UnpairedStackTraceDto>,
    /// Every cross-reference error, verbatim (no mod attribution is
    /// possible).
    pub cross_references: Vec<RawCrossReferenceDto>,
    /// Every DDS failure, attributed by its own texture path.
    pub dds_failures: Vec<DdsFailureSummaryDto>,
    /// Every dependency-without-URL warning, attributed.
    pub dependency_warnings: Vec<DependencyWarningSummaryDto>,
    /// Every recognized timer, attributed.
    pub timers: Vec<TimerSummaryDto>,
    /// Every raw def-cache-plugin log line, verbatim (color tags left
    /// as-is — `utils/defCache.ts` strips them for the apply-dialog
    /// note).
    pub def_cache_lines: Vec<String>,
    /// Every mod-list block the game logged on starting or loading a
    /// game, in file order. Empty when the log has no such block at all
    /// (distinct from an event with no mods — "can't compare orders"
    /// versus "ran with zero mods"); the different-order warning
    /// compares against the last one.
    pub load_events: Vec<LoadEventDto>,
    /// What the reader had to truncate, replace or drop to read the log
    /// at any size.
    pub read_stats: LogReadStatsDto,
    /// What the file covers. The variant is the log's kind (`Player.log` or
    /// console snapshot), so a kind and its facts cannot disagree.
    pub coverage: LogCoverageDto,
    /// Every stretch where the engine stopped writing messages, in file
    /// order.
    pub logging_gaps: Vec<LoggingGapDto>,
    /// How much of the log those gaps put in doubt; `None` when no family
    /// spans a gap.
    pub lower_bound: Option<LowerBoundDto>,
}

/// Mirrors [`LogReadStats`]: the reader's totals and the losses it
/// counted instead of refusing the log. Every counter except
/// `linesRead` is a loss; all zero for an ordinary log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct LogReadStatsDto {
    /// Every line the reader split off.
    #[ts(type = "number")]
    pub lines_read: u64,
    /// Lines longer than the per-line bound: only the head was
    /// classified.
    #[ts(type = "number")]
    pub lines_truncated: u64,
    /// Lines decoded with replacement characters for invalid UTF-8.
    #[ts(type = "number")]
    pub lines_with_invalid_utf8: u64,
    /// Lines past the per-block bound inside one stack-trace block.
    #[ts(type = "number")]
    pub stack_block_lines_dropped: u64,
    /// Multi-line joins abandoned for growing past their bound.
    #[ts(type = "number")]
    pub stack_joins_abandoned: u64,
    /// Entries folded into the last tracked startup pass.
    #[ts(type = "number")]
    pub passes_folded: u64,
    /// Stack back-reference originals that were not indexed.
    #[ts(type = "number")]
    pub stack_refs_dropped: u64,
    /// Logging gaps past the list bound: counted, not listed.
    #[ts(type = "number")]
    pub logging_gaps_dropped: u64,
    /// Crash-report locations cut to their length bound.
    #[ts(type = "number")]
    pub crash_report_paths_truncated: u64,
}

impl From<&LogReadStats> for LogReadStatsDto {
    fn from(value: &LogReadStats) -> Self {
        // Exhaustive destructuring: a new counter fails to compile here until
        // it is mapped (or bound to `_` on purpose).
        let LogReadStats {
            lines_read,
            lines_truncated,
            lines_with_invalid_utf8,
            stack_block_lines_dropped,
            stack_joins_abandoned,
            passes_folded,
            stack_refs_dropped,
            logging_gaps_dropped,
            crash_report_paths_truncated,
        } = *value;
        Self {
            lines_read,
            lines_truncated,
            lines_with_invalid_utf8,
            stack_block_lines_dropped,
            stack_joins_abandoned,
            passes_folded,
            stack_refs_dropped,
            logging_gaps_dropped,
            crash_report_paths_truncated,
        }
    }
}

/// Mirrors [`LoadEvent`]: one mod-list block from the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct LoadEventDto {
    /// The 1-based line number of the block's header line.
    #[ts(type = "number")]
    pub line: u64,
    /// Which header opened the block.
    pub kind: LoadEventKindDto,
    /// The active mods in load order, as normalized (lowercased) mod ids.
    pub mods: Vec<String>,
}

/// Mirrors [`LoadEventKind`]: a new game, or a save load carrying the
/// save's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum LoadEventKindDto {
    /// `Initializing new game with mods:`.
    NewGame,
    /// `Loading game from file <save name> with mods:`.
    #[serde(rename_all = "camelCase")]
    SaveLoad {
        /// The save's name exactly as the log printed it.
        save_name: String,
    },
}

impl From<&LoadEventKind> for LoadEventKindDto {
    fn from(value: &LoadEventKind) -> Self {
        match value {
            LoadEventKind::NewGame => Self::NewGame,
            LoadEventKind::SaveLoad { save_name } => Self::SaveLoad {
                save_name: save_name.clone(),
            },
        }
    }
}

impl From<&LoadEvent> for LoadEventDto {
    fn from(value: &LoadEvent) -> Self {
        Self {
            line: value.line as u64,
            kind: (&value.kind).into(),
            mods: value
                .mods
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
        }
    }
}

impl From<&GameLogSummary> for GameLogSummaryDto {
    fn from(value: &GameLogSummary) -> Self {
        Self {
            patch_failures: value.patch_failures.iter().map(Into::into).collect(),
            extra_stack_traces: value.extra_stack_traces.iter().map(Into::into).collect(),
            cross_references: value.cross_references.iter().map(Into::into).collect(),
            dds_failures: value.dds_failures.iter().map(Into::into).collect(),
            dependency_warnings: value.dependency_warnings.iter().map(Into::into).collect(),
            timers: value.timers.iter().map(Into::into).collect(),
            def_cache_lines: value.def_cache_lines.clone(),
            load_events: value.load_events.iter().map(Into::into).collect(),
            read_stats: (&value.read_stats).into(),
            coverage: (&value.coverage).into(),
            logging_gaps: value.logging_gaps.iter().map(Into::into).collect(),
            lower_bound: LowerBoundDto::of(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;

    use super::*;

    #[test]
    fn attribution_maps_a_resolved_mod() {
        let dto: LogAttributionDto = (&LogAttribution::Mod(ModId::new("a.mod"))).into();
        assert_eq!(
            dto,
            LogAttributionDto::Mod {
                mod_id: "a.mod".to_string()
            }
        );
    }

    #[test]
    fn attribution_keeps_the_raw_text_when_unattributed() {
        let dto: LogAttributionDto =
            (&LogAttribution::Unattributed("[Some Mod]".to_string())).into();
        assert_eq!(
            dto,
            LogAttributionDto::Unattributed {
                raw: "[Some Mod]".to_string()
            }
        );
    }

    fn summary_with(load_events: Vec<LoadEvent>) -> GameLogSummary {
        GameLogSummary {
            patch_failures: Vec::new(),
            extra_stack_traces: Vec::new(),
            cross_references: Vec::new(),
            dds_failures: Vec::new(),
            dependency_warnings: Vec::new(),
            timers: Vec::new(),
            def_cache_lines: Vec::new(),
            load_events,
            read_stats: LogReadStats::default(),
            classes: std::collections::BTreeMap::new(),
            blank_separator_lines: 0,
            sentinels: rim_session::ports::SentinelReport::default(),
            coverage: rim_session::ports::LogCoverage::default(),
            logging_gaps: Vec::new(),
        }
    }

    #[test]
    fn game_log_summary_maps_every_read_statistic() {
        let mut summary = summary_with(Vec::new());
        summary.read_stats = LogReadStats {
            lines_read: 10,
            lines_truncated: 2,
            lines_with_invalid_utf8: 3,
            stack_block_lines_dropped: 4,
            stack_joins_abandoned: 5,
            passes_folded: 6,
            stack_refs_dropped: 7,
            logging_gaps_dropped: 8,
            crash_report_paths_truncated: 9,
        };

        let dto: GameLogSummaryDto = (&summary).into();

        assert_eq!(
            dto.read_stats,
            LogReadStatsDto {
                lines_read: 10,
                lines_truncated: 2,
                lines_with_invalid_utf8: 3,
                stack_block_lines_dropped: 4,
                stack_joins_abandoned: 5,
                passes_folded: 6,
                stack_refs_dropped: 7,
                logging_gaps_dropped: 8,
                crash_report_paths_truncated: 9,
            }
        );
    }

    #[test]
    fn game_log_summary_maps_coverage_and_logging_gaps() {
        use rim_session::ports::{GapEnd, LogCoverage, LoggingGap, SnapshotCoverage};

        let mut summary = summary_with(Vec::new());
        summary.coverage = LogCoverage::ConsoleSnapshot(SnapshotCoverage {
            entries: 1_000,
            head_truncated: false,
        });
        summary.logging_gaps = vec![
            LoggingGap {
                stop_line: 10,
                resume: GapEnd::Resumed { line: 20 },
            },
            LoggingGap {
                stop_line: 90,
                resume: GapEnd::NeverResumed,
            },
        ];

        let dto: GameLogSummaryDto = (&summary).into();

        assert!(matches!(
            dto.coverage,
            LogCoverageDto::ConsoleSnapshot {
                entries: 1_000,
                head_truncated: false,
                ..
            }
        ));
        assert_eq!(dto.logging_gaps.len(), 2);
        assert_eq!(dto.logging_gaps[1].stop_line, 90);
        assert_eq!(dto.lower_bound, None, "no family spans a gap here");
    }

    #[test]
    fn game_log_summary_with_no_load_event_maps_to_an_empty_list() {
        let dto: GameLogSummaryDto = (&summary_with(Vec::new())).into();
        assert!(dto.load_events.is_empty());
    }

    #[test]
    fn game_log_summary_maps_load_events_in_order_with_kind_and_line() {
        let summary = summary_with(vec![
            LoadEvent {
                line: 3,
                kind: LoadEventKind::NewGame,
                mods: vec![ModId::new("a.mod"), ModId::new("b.mod")],
            },
            LoadEvent {
                line: 40,
                kind: LoadEventKind::SaveLoad {
                    save_name: "SaveA".to_string(),
                },
                mods: vec![ModId::new("b.mod")],
            },
        ]);

        let dto: GameLogSummaryDto = (&summary).into();

        assert_eq!(
            dto.load_events,
            vec![
                LoadEventDto {
                    line: 3,
                    kind: LoadEventKindDto::NewGame,
                    mods: vec!["a.mod".to_string(), "b.mod".to_string()],
                },
                LoadEventDto {
                    line: 40,
                    kind: LoadEventKindDto::SaveLoad {
                        save_name: "SaveA".to_string()
                    },
                    mods: vec!["b.mod".to_string()],
                },
            ]
        );
    }

    #[test]
    fn load_event_kind_serializes_as_a_camel_case_type_tagged_union() {
        let json = serde_json::to_value(LoadEventKindDto::SaveLoad {
            save_name: "SaveA".to_string(),
        })
        .expect("serializes");
        assert_eq!(
            json,
            serde_json::json!({"type": "saveLoad", "saveName": "SaveA"})
        );
    }
}
