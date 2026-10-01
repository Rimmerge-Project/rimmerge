import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import { areCountsLowerBounds, isConsoleSnapshot } from "@/utils/gameLogCoverage";

/**
 * The `/startup` page's
 * two log-derived columns are deliberately absent from `ModCostRowDto`
 * (see `dto/startup.rs`'s own doc comment) — these pure helpers join a
 * `GameLogSummaryDto` already in hand against a row's `modId`,
 * client-side, per mod.
 */

/**
 * Why a per-mod value is absent: no log was imported at all, or the log is a
 * console snapshot — a copy of at most the console's last entries, which
 * never shows a whole startup, so a fact only a `Player.log` holds is not
 * available from it (and is never shown as `0`).
 */
export type NotImportedReason = "noLog" | "consoleSnapshot";

/** A count sourced from an imported log — `imported: false` renders as "not imported", never `0`. */
export type ImportedCount =
  | { imported: false; reason: NotImportedReason }
  | { imported: true; count: number; isLowerBound: boolean };

/** Observed-timer totals for one mod, sourced from an imported log. */
export type ImportedTimers =
  | { imported: false; reason: NotImportedReason }
  | { imported: true; count: number; totalMilliseconds: number; isLowerBound: boolean };

/**
 * Builds a `modId -> bad-dimension-DDS count` lookup (the startup
 * page's sole source for this column — no static header reader exists).
 * `summary: null` (no log imported yet) makes every lookup report
 * `{ imported: false, reason: "noLog" }`. A console snapshot, or a
 * `Player.log` where the game stopped logging for a stretch or that is cut
 * off before any patch result, still counts what it shows, but only as a
 * lower bound: the missing entries (the loading phase, or what the game never
 * wrote) make an exact count claim too much.
 */
export function badDimensionDdsLookup(
  summary: GameLogSummaryDto | null,
): (modId: string) => ImportedCount {
  if (!summary) {
    return () => ({ imported: false, reason: "noLog" });
  }
  const isLowerBound = areCountsLowerBounds(summary);
  const counts = new Map<string, number>();
  for (const dds of summary.ddsFailures) {
    if (dds.attribution.kind !== "mod") {
      continue;
    }
    counts.set(dds.attribution.modId, (counts.get(dds.attribution.modId) ?? 0) + 1);
  }
  return (modId) => ({ imported: true, count: counts.get(modId) ?? 0, isLowerBound });
}

/**
 * Builds a `modId -> observed timer totals` lookup, from the import's
 * timers. `summary: null` makes every lookup report
 * `{ imported: false, reason: "noLog" }`; a console snapshot reports
 * `{ imported: false, reason: "consoleSnapshot" }` for every mod, since
 * load timers are a `Player.log` startup fact a snapshot cannot vouch for.
 * A `Player.log` with a logging gap, or cut off before any patch result,
 * reports its timers as lower bounds: they were observed, but the log may
 * lack some of them.
 */
export function observedTimersLookup(
  summary: GameLogSummaryDto | null,
): (modId: string) => ImportedTimers {
  if (!summary) {
    return () => ({ imported: false, reason: "noLog" });
  }
  if (isConsoleSnapshot(summary)) {
    return () => ({ imported: false, reason: "consoleSnapshot" });
  }
  const isLowerBound = areCountsLowerBounds(summary);
  const byMod = new Map<string, { count: number; totalMilliseconds: number }>();
  for (const timer of summary.timers) {
    if (timer.attribution.kind !== "mod") {
      continue;
    }
    const existing = byMod.get(timer.attribution.modId) ?? { count: 0, totalMilliseconds: 0 };
    existing.count += 1;
    existing.totalMilliseconds += timer.milliseconds;
    byMod.set(timer.attribution.modId, existing);
  }
  return (modId) => {
    const entry = byMod.get(modId) ?? { count: 0, totalMilliseconds: 0 };
    return { imported: true, ...entry, isLowerBound };
  };
}
