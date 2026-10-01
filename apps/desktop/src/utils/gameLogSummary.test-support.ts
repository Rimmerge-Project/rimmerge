import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import type { LogCoverageDto } from "@/types/generated/LogCoverageDto";

type PlayerLogCoverage = Extract<LogCoverageDto, { kind: "playerLog" }>;
type SnapshotCoverage = Extract<LogCoverageDto, { kind: "consoleSnapshot" }>;

/** A `Player.log` that exited cleanly after two startup passes — the ordinary, whole-session case. */
export const CLEAN_PLAYER_LOG: PlayerLogCoverage = {
  kind: "playerLog",
  passes: 2,
  patchPhase: "noneLogged",
  endState: { kind: "cleanExit" },
};

/** A `Player.log`'s coverage: the clean two-pass case unless overridden. */
export function playerLogCoverage(overrides: Partial<PlayerLogCoverage> = {}): LogCoverageDto {
  return { ...CLEAN_PLAYER_LOG, ...overrides };
}

/** A console snapshot of `entries` entries, at the console's 1,000-entry cap when `entries` is 1000. */
export function snapshotCoverage(
  entries: number,
  overrides: Partial<SnapshotCoverage> = {},
): LogCoverageDto {
  const fill = entries === 1000 ? "atCap" : entries > 1000 ? "overCap" : "belowCap";
  return { kind: "consoleSnapshot", entries, fill, headTruncated: false, ...overrides };
}

/** A well-formed, empty `GameLogSummaryDto` (a clean `Player.log`, no findings, no losses); override what a test cares about. */
export function gameLogSummary(overrides: Partial<GameLogSummaryDto> = {}): GameLogSummaryDto {
  return {
    patchFailures: [],
    extraStackTraces: [],
    crossReferences: [],
    ddsFailures: [],
    dependencyWarnings: [],
    timers: [],
    defCacheLines: [],
    loadEvents: [],
    readStats: {
      linesRead: 0,
      linesTruncated: 0,
      linesWithInvalidUtf8: 0,
      stackBlockLinesDropped: 0,
      stackJoinsAbandoned: 0,
      passesFolded: 0,
      stackRefsDropped: 0,
      loggingGapsDropped: 0,
      crashReportPathsTruncated: 0,
    },
    coverage: CLEAN_PLAYER_LOG,
    loggingGaps: [],
    lowerBound: null,
    ...overrides,
  };
}
