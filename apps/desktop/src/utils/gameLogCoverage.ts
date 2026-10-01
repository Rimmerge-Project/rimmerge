import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import type { LogCoverageDto } from "@/types/generated/LogCoverageDto";
import type { LoggingGapDto } from "@/types/generated/LoggingGapDto";
import type { LogReadStatsDto } from "@/types/generated/LogReadStatsDto";
import { assertNever } from "@/utils/assertNever";

/**
 * Pure helpers that say, as {@link MessageDescriptor}s, what an imported
 * game log covers — its kind, how a `Player.log` ends or how full a console
 * snapshot is, where the game stopped logging, and what the reader had to
 * bound. The wording follows the CLI's `log import` (`docs/cli.md`); every
 * sentence lives in the `gameLog.*` catalogue, and every number was derived
 * in Rust (`dto/game_log_coverage.rs`) — nothing is recomputed here.
 */

/** The most logging gaps the summary lists; the total is always stated. */
export const MAX_GAPS_LISTED = 5;

/** Whether the summary is a console snapshot (a copy of the in-game console), never a whole session. */
export function isConsoleSnapshot(summary: GameLogSummaryDto): boolean {
  return summary.coverage.kind === "consoleSnapshot";
}

/** Whether the game itself stopped logging for a stretch (listed or counted past the list limit). */
export function hasLoggingGaps(summary: GameLogSummaryDto): boolean {
  return summary.loggingGaps.length > 0 || summary.readStats.loggingGapsDropped > 0;
}

/**
 * Whether the summary's counts may be short: a console snapshot never shows a
 * whole session, a `Player.log` with a logging gap lacks what the game never
 * wrote, and one cut off before any patch result was logged may stop before
 * the loading that holds the DDS and timer facts. Any of these makes a count
 * "at least N", never an exact figure.
 *
 * Deliberately NOT every non-clean end: a `Player.log` copied while the game
 * runs is the common case, and "at least" on every such copy would stop
 * meaning anything. Only the cut-off-before-the-patch-phase shape, where the
 * log itself shows the game had barely started, qualifies.
 */
export function areCountsLowerBounds(summary: GameLogSummaryDto): boolean {
  return (
    isConsoleSnapshot(summary) ||
    hasLoggingGaps(summary) ||
    isCutOffBeforePatchPhase(summary.coverage)
  );
}

/**
 * Whether a `Player.log` stops before any patch result was logged and
 * without a clean exit: the game crashed or stopped writing, so the patch
 * phase may never have run and the absence of a patch failure proves nothing.
 */
export function isCutOffBeforePatchPhase(coverage: LogCoverageDto): boolean {
  switch (coverage.kind) {
    case "playerLog":
      return coverage.patchPhase === "noneLogged" && coverage.endState.kind !== "cleanExit";
    case "consoleSnapshot":
      return false;
    default:
      return assertNever(coverage);
  }
}

/** The kind's label: `Player.log` or `Console snapshot`. */
export function logKindLabel(coverage: LogCoverageDto): MessageDescriptor {
  switch (coverage.kind) {
    case "playerLog":
      return descriptor("gameLog.kind.playerLog");
    case "consoleSnapshot":
      return descriptor("gameLog.kind.consoleSnapshot");
    default:
      return assertNever(coverage);
  }
}

function playerLogLine(
  coverage: Extract<LogCoverageDto, { kind: "playerLog" }>,
): MessageDescriptor {
  const { passes, endState } = coverage;
  switch (endState.kind) {
    case "cleanExit":
      return descriptor("gameLog.coverage.playerLog.cleanExit", { count: passes }, passes);
    case "crashed":
      return endState.reportPath === null
        ? descriptor("gameLog.coverage.playerLog.crashedNoPath", { count: passes }, passes)
        : descriptor(
            "gameLog.coverage.playerLog.crashed",
            { count: passes, path: endState.reportPath },
            passes,
          );
    case "truncated":
      return descriptor("gameLog.coverage.playerLog.truncated", { count: passes }, passes);
    default:
      return assertNever(endState);
  }
}

function snapshotLine(
  coverage: Extract<LogCoverageDto, { kind: "consoleSnapshot" }>,
): MessageDescriptor {
  const { entries, fill } = coverage;
  switch (fill) {
    case "belowCap":
      return descriptor("gameLog.coverage.snapshot.belowCap", { count: entries }, entries);
    case "atCap":
      return descriptor("gameLog.coverage.snapshot.atCap", { count: entries }, entries);
    case "overCap":
      return descriptor("gameLog.coverage.snapshot.overCap", { count: entries }, entries);
    default:
      return assertNever(fill);
  }
}

/**
 * The one-line description of what the file covers (first), plus the
 * snapshot's starts-mid-entry note when it applies (second).
 */
export function coverageLines(coverage: LogCoverageDto): MessageDescriptor[] {
  switch (coverage.kind) {
    case "playerLog":
      return [playerLogLine(coverage)];
    case "consoleSnapshot":
      return coverage.headTruncated
        ? [snapshotLine(coverage), descriptor("gameLog.coverage.snapshot.headTruncated")]
        : [snapshotLine(coverage)];
    default:
      return assertNever(coverage);
  }
}

/** How many logging gaps the log has, and how many of them never resumed. */
export function loggingGapCounts(gaps: readonly LoggingGapDto[]): {
  total: number;
  neverResumed: number;
} {
  return {
    total: gaps.length,
    neverResumed: gaps.filter((gap) => gap.end.kind === "neverResumed").length,
  };
}

/** One gap as a line: `line N to line M`, or `line N to the end` when it never resumed. */
export function describeGap(gap: LoggingGapDto): MessageDescriptor {
  switch (gap.end.kind) {
    case "resumed":
      return descriptor("gameLog.gaps.resumed", { stop: gap.stopLine, resume: gap.end.line });
    case "neverResumed":
      return descriptor("gameLog.gaps.open", { stop: gap.stopLine });
    default:
      return assertNever(gap.end);
  }
}

/** Every read-stat counter that is a loss: all of them but the `linesRead` total. */
type LossCounter = Exclude<keyof LogReadStatsDto, "linesRead">;

/**
 * One whole-sentence key per loss counter. Keyed by `satisfies Record`, so a
 * counter added to `LogReadStatsDto` is a compile error here until it has a
 * sentence, never a silently unreported loss.
 */
const LOSS_KEYS = {
  linesTruncated: "gameLog.losses.linesTruncated",
  linesWithInvalidUtf8: "gameLog.losses.linesWithInvalidUtf8",
  stackBlockLinesDropped: "gameLog.losses.stackBlockLinesDropped",
  stackJoinsAbandoned: "gameLog.losses.stackJoinsAbandoned",
  passesFolded: "gameLog.losses.passesFolded",
  stackRefsDropped: "gameLog.losses.stackRefsDropped",
  loggingGapsDropped: "gameLog.losses.loggingGapsDropped",
  crashReportPathsTruncated: "gameLog.losses.crashReportPathsTruncated",
} as const satisfies Record<LossCounter, string>;

/** One line per nonzero loss counter, in a fixed order; empty for an ordinary log. */
export function readLossLines(stats: LogReadStatsDto): MessageDescriptor[] {
  const counters = Object.keys(LOSS_KEYS) as LossCounter[];
  return counters
    .filter((counter) => stats[counter] !== 0)
    .map((counter) => descriptor(LOSS_KEYS[counter], { count: stats[counter] }, stats[counter]));
}
