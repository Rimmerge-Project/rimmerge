import { describe, expect, it } from "vitest";

import type { LogCoverageDto } from "@/types/generated/LogCoverageDto";
import {
  areCountsLowerBounds,
  coverageLines,
  describeGap,
  hasLoggingGaps,
  isConsoleSnapshot,
  isCutOffBeforePatchPhase,
  loggingGapCounts,
  logKindLabel,
  readLossLines,
} from "@/utils/gameLogCoverage";
import {
  CLEAN_PLAYER_LOG,
  gameLogSummary,
  playerLogCoverage,
  snapshotCoverage,
} from "@/utils/gameLogSummary.test-support";

type EndState = Extract<LogCoverageDto, { kind: "playerLog" }>["endState"];

function playerLog(endState: EndState): LogCoverageDto {
  return { kind: "playerLog", passes: 2, patchPhase: "noneLogged", endState };
}

describe("logKindLabel", () => {
  it("names each kind", () => {
    expect(logKindLabel(CLEAN_PLAYER_LOG).key).toBe("gameLog.kind.playerLog");
    expect(logKindLabel(snapshotCoverage(3)).key).toBe("gameLog.kind.consoleSnapshot");
  });

  it("derives a summary's kind from its coverage variant", () => {
    expect(isConsoleSnapshot(gameLogSummary())).toBe(false);
    expect(isConsoleSnapshot(gameLogSummary({ coverage: snapshotCoverage(3) }))).toBe(true);
  });
});

describe("coverageLines for a Player.log", () => {
  it("states a clean exit with the startup passes", () => {
    expect(coverageLines(playerLog({ kind: "cleanExit" }))).toEqual([
      { key: "gameLog.coverage.playerLog.cleanExit", params: { count: 2 }, count: 2 },
    ]);
  });

  it("names the crash report location when the log holds one", () => {
    const [line] = coverageLines(
      playerLog({ kind: "crashed", reportPath: "C:/scratch/crash.dmp" }),
    );

    expect(line).toEqual({
      key: "gameLog.coverage.playerLog.crashed",
      params: { count: 2, path: "C:/scratch/crash.dmp" },
      count: 2,
    });
  });

  it("says the log names no crash report when it has no location", () => {
    const [line] = coverageLines(playerLog({ kind: "crashed", reportPath: null }));

    expect(line?.key).toBe("gameLog.coverage.playerLog.crashedNoPath");
  });

  it("says a log with no exit footer is cut off, never that it ended cleanly", () => {
    const [line] = coverageLines(playerLog({ kind: "truncated" }));

    expect(line?.key).toBe("gameLog.coverage.playerLog.truncated");
  });
});

describe("coverageLines for a console snapshot", () => {
  it.each([
    [10, "gameLog.coverage.snapshot.belowCap"],
    [1000, "gameLog.coverage.snapshot.atCap"],
    [1200, "gameLog.coverage.snapshot.overCap"],
  ])("a snapshot of %i entries reads as %s", (entries, key) => {
    const lines = coverageLines(snapshotCoverage(entries));

    expect(lines).toHaveLength(1);
    expect(lines[0]?.key).toBe(key);
    expect(lines[0]?.params).toEqual({ count: entries });
  });

  it("adds the starts-mid-entry note when the copy is head-truncated", () => {
    const lines = coverageLines(snapshotCoverage(10, { headTruncated: true }));

    expect(lines.map((line) => line.key)).toEqual([
      "gameLog.coverage.snapshot.belowCap",
      "gameLog.coverage.snapshot.headTruncated",
    ]);
  });
});

describe("logging gaps", () => {
  it("counts the gaps and those that never resumed", () => {
    expect(
      loggingGapCounts([
        { stopLine: 3, end: { kind: "resumed", line: 4 } },
        { stopLine: 9, end: { kind: "neverResumed" } },
      ]),
    ).toEqual({ total: 2, neverResumed: 1 });
    expect(loggingGapCounts([])).toEqual({ total: 0, neverResumed: 0 });
  });

  it("describes a resumed gap by both lines and an open one by its stop line", () => {
    expect(describeGap({ stopLine: 3, end: { kind: "resumed", line: 40 } })).toEqual({
      key: "gameLog.gaps.resumed",
      params: { stop: 3, resume: 40 },
    });
    expect(describeGap({ stopLine: 9, end: { kind: "neverResumed" } })).toEqual({
      key: "gameLog.gaps.open",
      params: { stop: 9 },
    });
  });
});

describe("readLossLines", () => {
  it("reports nothing for an ordinary read", () => {
    expect(readLossLines(gameLogSummary().readStats)).toEqual([]);
  });

  it("reports only the nonzero counters, never the lines-read total", () => {
    const stats = {
      ...gameLogSummary().readStats,
      linesRead: 500,
      linesTruncated: 12,
      linesWithInvalidUtf8: 1,
      loggingGapsDropped: 3,
      crashReportPathsTruncated: 2,
    };

    const lines = readLossLines(stats);

    expect(lines.map((line) => line.key)).toEqual([
      "gameLog.losses.linesTruncated",
      "gameLog.losses.linesWithInvalidUtf8",
      "gameLog.losses.loggingGapsDropped",
      "gameLog.losses.crashReportPathsTruncated",
    ]);
    expect(lines[0]).toEqual({
      key: "gameLog.losses.linesTruncated",
      params: { count: 12 },
      count: 12,
    });
  });
});

describe("isCutOffBeforePatchPhase", () => {
  it("is true for a Player.log that ended without a clean exit and logged no patch result", () => {
    expect(isCutOffBeforePatchPhase(playerLog({ kind: "truncated" }))).toBe(true);
    expect(isCutOffBeforePatchPhase(playerLog({ kind: "crashed", reportPath: null }))).toBe(true);
  });

  it("is false for a clean exit, for a log that logged a patch result, and for a snapshot", () => {
    expect(isCutOffBeforePatchPhase(playerLog({ kind: "cleanExit" }))).toBe(false);
    expect(
      isCutOffBeforePatchPhase({
        ...CLEAN_PLAYER_LOG,
        patchPhase: "patchFailureLogged",
        endState: { kind: "truncated" },
      }),
    ).toBe(false);
    expect(isCutOffBeforePatchPhase(snapshotCoverage(3))).toBe(false);
  });
});

describe("areCountsLowerBounds", () => {
  it("holds for a snapshot and for a Player.log with a gap, listed or only counted", () => {
    const gap = { stopLine: 4, end: { kind: "neverResumed" } } as const;

    expect(areCountsLowerBounds(gameLogSummary({ coverage: snapshotCoverage(3) }))).toBe(true);
    expect(areCountsLowerBounds(gameLogSummary({ loggingGaps: [gap] }))).toBe(true);
    const dropped = gameLogSummary();
    dropped.readStats.loggingGapsDropped = 2;
    expect(hasLoggingGaps(dropped)).toBe(true);
    expect(areCountsLowerBounds(dropped)).toBe(true);
  });

  it("holds for a Player.log cut off before any patch result, with no gap", () => {
    const cutOff = gameLogSummary({
      coverage: playerLogCoverage({ endState: { kind: "truncated" } }),
    });

    expect(hasLoggingGaps(cutOff)).toBe(false);
    expect(areCountsLowerBounds(cutOff)).toBe(true);
  });

  it("does not hold for a non-clean end that logged a patch result: a copy taken mid-run is the common case", () => {
    const midRun = gameLogSummary({
      coverage: playerLogCoverage({
        endState: { kind: "truncated" },
        patchPhase: "patchFailureLogged",
      }),
    });

    expect(areCountsLowerBounds(midRun)).toBe(false);
  });

  it("does not hold for a whole Player.log without gaps", () => {
    expect(areCountsLowerBounds(gameLogSummary())).toBe(false);
  });
});
