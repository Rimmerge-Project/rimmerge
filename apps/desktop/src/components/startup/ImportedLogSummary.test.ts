import Aura from "@primevue/themes/aura";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import { describe, expect, it } from "vitest";

import ImportedLogSummary from "@/components/startup/ImportedLogSummary.vue";
import { useSessionStore } from "@/stores/session";
import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import {
  gameLogSummary,
  playerLogCoverage,
  snapshotCoverage,
} from "@/utils/gameLogSummary.test-support";

function mountWith(summary: GameLogSummaryDto | null) {
  const pinia = createPinia();
  setActivePinia(pinia);
  const session = useSessionStore();
  if (summary) {
    session.setGameLogSummary(summary);
  }
  return mount(ImportedLogSummary, {
    global: { plugins: [pinia, [PrimeVue, { theme: { preset: Aura } }]] },
  });
}

type Wrapper = ReturnType<typeof mountWith>;

const text = (wrapper: Wrapper, testId: string) => wrapper.get(`[data-testid="${testId}"]`).text();
const has = (wrapper: Wrapper, testId: string) =>
  wrapper.find(`[data-testid="${testId}"]`).exists();

describe("ImportedLogSummary", () => {
  it("renders nothing until a log is imported", () => {
    expect(has(mountWith(null), "imported-log-summary")).toBe(false);
  });

  it("labels a clean Player.log with its kind, startup passes and clean exit, and warns of nothing", () => {
    const wrapper = mountWith(gameLogSummary());

    expect(text(wrapper, "imported-log-kind")).toBe("Player.log");
    expect(text(wrapper, "imported-log-coverage")).toBe(
      "2 startup passes; the log ends with a clean exit",
    );
    expect(has(wrapper, "imported-log-gaps-warning")).toBe(false);
    expect(has(wrapper, "imported-log-losses")).toBe(false);
  });

  it("says a crashed Player.log crashed and where the crash report is", () => {
    const wrapper = mountWith(
      gameLogSummary({
        coverage: playerLogCoverage({
          endState: { kind: "crashed", reportPath: "C:/scratch/crash.dmp" },
        }),
      }),
    );

    expect(text(wrapper, "imported-log-coverage")).toContain("the log ends with a game crash");
    expect(text(wrapper, "imported-log-coverage")).toContain("C:/scratch/crash.dmp");
  });

  it("says a Player.log with no exit footer is cut off, not that it ended", () => {
    const wrapper = mountWith(
      gameLogSummary({
        coverage: playerLogCoverage({ passes: 1, endState: { kind: "truncated" } }),
      }),
    );

    const coverage = text(wrapper, "imported-log-coverage");
    expect(coverage).toContain("1 startup pass;");
    expect(coverage).toContain("ends mid-run with no exit footer");
    expect(coverage).not.toContain("stopped writing");
    expect(coverage).not.toContain("clean exit");
  });

  it("labels a snapshot below the cap as only what the console held", () => {
    const wrapper = mountWith(gameLogSummary({ coverage: snapshotCoverage(392) }));

    expect(text(wrapper, "imported-log-kind")).toBe("Console snapshot");
    expect(text(wrapper, "imported-log-coverage")).toContain("The console held only 392 entries");
    expect(text(wrapper, "imported-log-coverage")).not.toContain("startup pass");
  });

  it("says a snapshot at the cap had older entries dropped by the game", () => {
    const wrapper = mountWith(gameLogSummary({ coverage: snapshotCoverage(1000) }));

    expect(text(wrapper, "imported-log-coverage")).toContain(
      "older entries were dropped by the game",
    );
  });

  it("says a snapshot over the cap is more than one console holds", () => {
    const wrapper = mountWith(gameLogSummary({ coverage: snapshotCoverage(1500) }));

    const coverage = text(wrapper, "imported-log-coverage");
    expect(coverage).toContain("more than one console holds");
    expect(coverage).toContain("not a single console copy");
    expect(coverage).not.toContain("only part of a session");
  });

  it("says a hand-cut snapshot starts mid-entry", () => {
    const wrapper = mountWith(
      gameLogSummary({ coverage: snapshotCoverage(20, { headTruncated: true }) }),
    );

    expect(text(wrapper, "imported-log-coverage-note")).toContain("starts mid-entry");
  });

  it("does not claim the snapshot starts mid-entry when it does not", () => {
    const wrapper = mountWith(gameLogSummary({ coverage: snapshotCoverage(20) }));

    expect(has(wrapper, "imported-log-coverage-note")).toBe(false);
  });

  it("warns of logging gaps with the count, those that never resumed, the lower-bound share and each gap's lines", () => {
    const wrapper = mountWith(
      gameLogSummary({
        loggingGaps: [
          { stopLine: 3, end: { kind: "resumed", line: 40 } },
          { stopLine: 90, end: { kind: "neverResumed" } },
        ],
        lowerBound: { families: 4, entries: 120, entriesPercent: 37 },
      }),
    );

    const warning = text(wrapper, "imported-log-gaps-warning");
    expect(warning).toContain("stopped writing messages 2 times");
    expect(warning).toContain("1 logging stop has no resume message after it");
    expect(warning).toContain("At least 4 families of entries (37% of the log's entries)");
    const gaps = text(wrapper, "imported-log-gaps-list");
    expect(gaps).toContain("Line 3 to line 40");
    expect(gaps).toContain("Line 90 to the end of the log");
  });

  it("lists at most five gaps and counts the rest", () => {
    const loggingGaps = Array.from({ length: 8 }, (_, index) => ({
      stopLine: 10 * (index + 1),
      end: { kind: "resumed", line: 10 * (index + 1) + 1 } as const,
    }));

    const wrapper = mountWith(gameLogSummary({ loggingGaps }));

    expect(wrapper.findAll('[data-testid="imported-log-gaps-list"] li')).toHaveLength(6);
    expect(text(wrapper, "imported-log-gaps-list")).toContain("And 3 more");
  });

  it("names a single gap in the singular and adds no 'never resumed' line when it resumed", () => {
    const wrapper = mountWith(
      gameLogSummary({ loggingGaps: [{ stopLine: 3, end: { kind: "resumed", line: 4 } }] }),
    );

    const warning = text(wrapper, "imported-log-gaps-warning");
    expect(warning).toContain("stopped writing messages 1 time.");
    expect(warning).not.toContain("never resumed");
  });

  it("counts the gaps the reader dropped from its list in the total", () => {
    const wrapper = mountWith(
      gameLogSummary({
        loggingGaps: [{ stopLine: 3, end: { kind: "resumed", line: 4 } }],
        readStats: { ...gameLogSummary().readStats, loggingGapsDropped: 4 },
      }),
    );

    expect(text(wrapper, "imported-log-gaps-warning")).toContain(
      "stopped writing messages 5 times",
    );
    expect(text(wrapper, "imported-log-gaps-list")).toContain("And 4 more");
  });

  it("lists every nonzero read loss", () => {
    const wrapper = mountWith(
      gameLogSummary({
        readStats: {
          ...gameLogSummary().readStats,
          linesTruncated: 12,
          linesWithInvalidUtf8: 1,
          loggingGapsDropped: 3,
          crashReportPathsTruncated: 1,
        },
      }),
    );

    const losses = text(wrapper, "imported-log-losses");
    expect(losses).toContain("12 lines were longer than the reader's limit");
    expect(losses).toContain("1 line held invalid UTF-8");
    expect(losses).toContain("3 logging gaps past the list limit");
    expect(losses).toContain("1 crash-report location was cut");
  });
});
