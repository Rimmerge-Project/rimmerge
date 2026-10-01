import { describe, expect, it } from "vitest";

import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import {
  gameLogSummary,
  playerLogCoverage,
  snapshotCoverage,
} from "@/utils/gameLogSummary.test-support";
import { badDimensionDdsLookup, observedTimersLookup } from "@/utils/startupMetrics";

const emptySummary = gameLogSummary;

describe("badDimensionDdsLookup", () => {
  it("reports not-imported for every mod when no log has been imported", () => {
    const lookup = badDimensionDdsLookup(null);

    expect(lookup("a.mod")).toEqual({ imported: false, reason: "noLog" });
  });

  it("counts DDS failures per mod once a log is imported, zero for a mod with none", () => {
    const summary = emptySummary();
    summary.ddsFailures = [
      {
        attribution: { kind: "mod", modId: "a.mod" },
        path: "x.dds",
        width: 3,
        height: 3,
        format: "BC7",
      },
      {
        attribution: { kind: "mod", modId: "a.mod" },
        path: "y.dds",
        width: 5,
        height: 5,
        format: "BC7",
      },
      {
        attribution: { kind: "unattributed", raw: "z.dds" },
        path: "z.dds",
        width: 3,
        height: 3,
        format: "BC7",
      },
    ];

    const lookup = badDimensionDdsLookup(summary);

    expect(lookup("a.mod")).toEqual({ imported: true, count: 2, isLowerBound: false });
    expect(lookup("b.mod")).toEqual({ imported: true, count: 0, isLowerBound: false });
  });
});

describe("observedTimersLookup", () => {
  it("reports not-imported for every mod when no log has been imported", () => {
    const lookup = observedTimersLookup(null);

    expect(lookup("a.mod")).toEqual({ imported: false, reason: "noLog" });
  });

  it("sums attributed timers' durations per mod, zero for a mod with none", () => {
    const summary = emptySummary();
    summary.timers = [
      { attribution: { kind: "mod", modId: "a.mod" }, label: "load", milliseconds: 100, pass: 2 },
      { attribution: { kind: "mod", modId: "a.mod" }, label: "init", milliseconds: 50, pass: 2 },
      {
        attribution: { kind: "unattributed", raw: "DEFCACHE" },
        label: "DEFCACHE",
        milliseconds: 8000,
        pass: 2,
      },
    ];

    const lookup = observedTimersLookup(summary);

    expect(lookup("a.mod")).toEqual({
      imported: true,
      count: 2,
      totalMilliseconds: 150,
      isLowerBound: false,
    });
    expect(lookup("b.mod")).toEqual({
      imported: true,
      count: 0,
      totalMilliseconds: 0,
      isLowerBound: false,
    });
  });
});

describe("startup metrics for a console snapshot", () => {
  const snapshot = () => gameLogSummary({ coverage: snapshotCoverage(1000) });

  it("never shows observed timers: load timers are a startup fact a snapshot cannot vouch for", () => {
    const summary = snapshot();
    summary.timers = [
      { attribution: { kind: "mod", modId: "a.mod" }, label: "load", milliseconds: 100, pass: 2 },
    ];

    const lookup = observedTimersLookup(summary);

    expect(lookup("a.mod")).toEqual({ imported: false, reason: "consoleSnapshot" });
    expect(lookup("b.mod")).toEqual({ imported: false, reason: "consoleSnapshot" });
  });

  it("marks bad-dimension DDS counts as lower bounds, including a zero", () => {
    const summary = snapshot();
    summary.ddsFailures = [
      {
        attribution: { kind: "mod", modId: "a.mod" },
        path: "x.dds",
        width: 3,
        height: 3,
        format: "BC7",
      },
    ];

    const lookup = badDimensionDdsLookup(summary);

    expect(lookup("a.mod")).toEqual({ imported: true, count: 1, isLowerBound: true });
    expect(lookup("b.mod")).toEqual({ imported: true, count: 0, isLowerBound: true });
  });
});

describe("startup metrics for a Player.log with a logging gap", () => {
  const gapped = (loggingGaps: GameLogSummaryDto["loggingGaps"]) => {
    const summary = gameLogSummary({ loggingGaps });
    summary.timers = [
      { attribution: { kind: "mod", modId: "a.mod" }, label: "load", milliseconds: 100, pass: 2 },
    ];
    summary.ddsFailures = [
      {
        attribution: { kind: "mod", modId: "a.mod" },
        path: "x.dds",
        width: 3,
        height: 3,
        format: "BC7",
      },
    ];
    return summary;
  };

  it("reports observed timers and bad-dimension DDS counts as lower bounds", () => {
    const summary = gapped([{ stopLine: 10, end: { kind: "neverResumed" } }]);

    expect(observedTimersLookup(summary)("a.mod")).toEqual({
      imported: true,
      count: 1,
      totalMilliseconds: 100,
      isLowerBound: true,
    });
    expect(badDimensionDdsLookup(summary)("a.mod")).toEqual({
      imported: true,
      count: 1,
      isLowerBound: true,
    });
  });

  it("keeps a gap-free Player.log's figures exact", () => {
    const summary = gapped([]);

    expect(observedTimersLookup(summary)("a.mod")).toMatchObject({ isLowerBound: false });
    expect(badDimensionDdsLookup(summary)("a.mod")).toMatchObject({ isLowerBound: false });
  });

  it("reports a Player.log cut off before any patch result as lower bounds, with no gap", () => {
    const summary = gapped([]);
    summary.coverage = playerLogCoverage({ endState: { kind: "truncated" } });

    expect(observedTimersLookup(summary)("a.mod")).toMatchObject({ isLowerBound: true });
    expect(badDimensionDdsLookup(summary)("a.mod")).toMatchObject({ isLowerBound: true });
  });

  it("counts a gap that was only counted, not listed, as a gap", () => {
    const summary = gapped([]);
    summary.readStats.loggingGapsDropped = 3;

    expect(badDimensionDdsLookup(summary)("a.mod")).toMatchObject({ isLowerBound: true });
  });
});
