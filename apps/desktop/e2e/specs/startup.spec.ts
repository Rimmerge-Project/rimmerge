import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * `mod.000` sorts last under the table's default (descending
 * texture-bytes) sort in this fixture, and the table is now virtualized
 * — only the rows
 * scrolled into view exist in the DOM, so a handful of specs that assert
 * on `mod.000`'s own cells need to scroll its row into range first, the
 * same thing a real user would do to see it.
 */
async function scrollStartupTableToBottom(page: Page): Promise<void> {
  await page
    .getByTestId("startup-table-scroll")
    .evaluate((el) => el.scrollTo({ top: el.scrollHeight }));
}

test.describe("startup cost", () => {
  test("the table sorts by texture bytes by default, and re-sorts on a header click", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-startup").click();
    await expect(page).toHaveURL(/\/startup$/);

    const table = page.getByTestId("startup-table");
    await expect(table).toBeVisible();

    // The fixture scales `textureBytes` with mod index, so the default
    // (descending) sort puts the highest-index mod first.
    const firstRowBefore = page.locator('[data-testid^="startup-row-"]').first();
    await expect(firstRowBefore).toHaveAttribute("data-testid", "startup-row-mod.059");

    await page.getByTestId("startup-sort-defCount").click();
    const firstRowAfterDefCount = page.locator('[data-testid^="startup-row-"]').first();
    await expect(firstRowAfterDefCount).toHaveAttribute("data-testid", "startup-row-mod.059");

    // `defCount` also scales with index, so click again to confirm the
    // header toggles direction rather than just re-selecting the column.
    await page.getByTestId("startup-sort-defCount").click();
    const firstRowAscending = page.locator('[data-testid^="startup-row-"]').first();
    await expect(firstRowAscending).toHaveAttribute("data-testid", "startup-row-mod.000");
  });

  test("shows a totals row above the per-mod rows", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-startup").click();

    const totals = page.getByTestId("startup-totals-row");
    await expect(totals).toBeVisible();
    await expect(totals).toContainText("Total (60 mods)");

    // The totals row must render before any per-mod row in the table.
    const table = page.getByTestId("startup-table");
    const rowTestIds = await table
      .locator("tbody tr")
      .evaluateAll((rows) => rows.map((row) => row.getAttribute("data-testid")));
    expect(rowTestIds[0]).toBe("startup-totals-row");
  });

  test("the bad-dimension-DDS and observed-timer columns read 'not imported' until a log is imported, then show real counts", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-startup").click();
    await scrollStartupTableToBottom(page);

    await expect(page.getByTestId("startup-bad-dds-mod.000")).toHaveText("not imported");
    await expect(page.getByTestId("startup-observed-timers-mod.000")).toHaveText("not imported");

    await page.evaluate(() => {
      window.__PICK_FILE_RESULT__ = "C:/RimWorld/Player.log";
    });
    await page.getByTestId("import-game-log-button").click();

    await expect(page.getByTestId("startup-bad-dds-mod.000")).toHaveText("1");
    await expect(page.getByTestId("startup-observed-timers-mod.000")).toHaveText("1 (250 ms)");
  });

  test("the different-order warning appears only once the imported log's order differs from the selected one", async ({
    page,
  }) => {
    await loadScenario(page);
    // A fresh load opens on the suggested order; the fixture log below
    // matches the current one.
    await page.getByTestId("order-source-current").click();
    await page.getByTestId("nav-startup").click();
    await scrollStartupTableToBottom(page);

    await page.evaluate(() => {
      window.__PICK_FILE_RESULT__ = "C:/RimWorld/Player.log";
      // The fixture default's `loadEvents` (last event) matches `currentOrder` —
      // this import must not warn.
    });
    await page.getByTestId("import-game-log-button").click();
    await expect(page.getByTestId("startup-bad-dds-mod.000")).toHaveText("1");
    await expect(page.getByTestId("startup-order-mismatch-warning")).toHaveCount(0);

    await page.evaluate(() => {
      window.__IMPORT_GAME_LOG_RESULT__ = {
        patchFailures: [],
        extraStackTraces: [],
        crossReferences: [],
        ddsFailures: [],
        dependencyWarnings: [],
        timers: [],
        defCacheLines: [],
        loadEvents: [{ line: 1, kind: { type: "newGame" }, mods: ["mod.059", "mod.000"] }],
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
        coverage: {
          kind: "playerLog",
          passes: 2,
          patchPhase: "noneLogged",
          endState: { kind: "cleanExit" },
        },
        loggingGaps: [],
        lowerBound: null,
      };
    });
    await page.getByTestId("import-game-log-button").click();

    await expect(page.getByTestId("startup-order-mismatch-warning")).toBeVisible();
  });

  test("shows the 'can't be checked' warning, not the mismatch one, when the log carries no active-mod list at all", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-startup").click();

    await page.evaluate(() => {
      // A real log shape (a real `Player.log` can have no load block at
      // all: neither `Initializing new game with mods:` nor `Loading game
      // from file ... with mods:`) — distinct from an empty order, and from no log imported yet.
      window.__IMPORT_GAME_LOG_RESULT__ = {
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
        coverage: {
          kind: "playerLog",
          passes: 2,
          patchPhase: "noneLogged",
          endState: { kind: "cleanExit" },
        },
        loggingGaps: [],
        lowerBound: null,
      };
      window.__PICK_FILE_RESULT__ = "C:/RimWorld/Player.log";
    });
    await page.getByTestId("import-game-log-button").click();

    await expect(page.getByTestId("startup-order-mismatch-warning")).toHaveCount(0);
    await expect(page.getByTestId("startup-order-unknown-warning")).toContainText(
      "carries no active-mod list",
    );
  });

  test("the dashboard's own import button round-trips the same summary and confirms with a toast", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.evaluate(() => {
      window.__PICK_FILE_RESULT__ = "C:/RimWorld/Player.log";
    });
    await page.getByTestId("import-game-log-button").click();

    // A successful import gives visible feedback on the dashboard.
    await expect(page.getByText("Game log imported")).toBeVisible();

    await page.getByTestId("nav-startup").click();
    await scrollStartupTableToBottom(page);
    await expect(page.getByTestId("startup-bad-dds-mod.000")).toHaveText("1");
  });
});

test.describe("imported log coverage", () => {
  async function importOnStartup(page: Page): Promise<void> {
    await loadScenario(page);
    await page.getByTestId("nav-startup").click();
    await page.evaluate(() => {
      window.__PICK_FILE_RESULT__ = "C:/RimWorld/Player.log";
    });
    await page.getByTestId("import-game-log-button").click();
  }

  test("a Player.log is labelled with its kind, startup passes and end state, with no warnings", async ({
    page,
  }) => {
    await importOnStartup(page);

    await expect(page.getByTestId("imported-log-kind")).toHaveText("Player.log");
    await expect(page.getByTestId("imported-log-coverage")).toHaveText(
      "2 startup passes; the log ends with a clean exit",
    );
    await expect(page.getByTestId("imported-log-gaps-warning")).toHaveCount(0);
    await expect(page.getByTestId("imported-log-losses")).toHaveCount(0);
    await expect(page.getByTestId("startup-snapshot-note")).toHaveCount(0);
    await expect(page.getByTestId("startup-gaps-note")).toHaveCount(0);
  });

  test("a Player.log with logging gaps and read losses warns about both", async ({ page }) => {
    await page.addInitScript(() => {
      window.__IMPORT_GAME_LOG_OVERRIDES__ = {
        coverage: {
          kind: "playerLog",
          passes: 2,
          patchPhase: "patchFailureLogged",
          endState: { kind: "truncated" },
        },
        loggingGaps: [
          { stopLine: 120, end: { kind: "resumed", line: 131 } },
          { stopLine: 4800, end: { kind: "resumed", line: 4811 } },
          { stopLine: 9000, end: { kind: "neverResumed" } },
        ],
        lowerBound: { families: 14, entries: 2200, entriesPercent: 91 },
        readStats: {
          linesRead: 41114,
          linesTruncated: 12,
          linesWithInvalidUtf8: 0,
          stackBlockLinesDropped: 0,
          stackJoinsAbandoned: 0,
          passesFolded: 0,
          stackRefsDropped: 0,
          loggingGapsDropped: 0,
          crashReportPathsTruncated: 0,
        },
      };
    });
    await importOnStartup(page);

    await expect(page.getByTestId("imported-log-coverage")).toContainText(
      "ends mid-run with no exit footer",
    );
    const warning = page.getByTestId("imported-log-gaps-warning");
    await expect(warning).toContainText("stopped writing messages 3 times");
    await expect(warning).toContainText("1 logging stop has no resume message after it");
    await expect(warning).toContainText(
      "At least 14 families of entries (91% of the log's entries)",
    );
    await expect(page.getByTestId("imported-log-gaps-list")).toContainText(
      "Line 9000 to the end of the log",
    );
    await expect(page.getByTestId("imported-log-losses")).toContainText(
      "12 lines were longer than the reader's limit",
    );
  });

  test("a Player.log with a logging gap shows its startup counts as lower bounds, with a note", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      window.__IMPORT_GAME_LOG_OVERRIDES__ = {
        loggingGaps: [{ stopLine: 120, end: { kind: "neverResumed" } }],
        lowerBound: { families: 2, entries: 10, entriesPercent: 5 },
      };
    });
    await importOnStartup(page);
    await scrollStartupTableToBottom(page);

    await expect(page.getByTestId("startup-gaps-note")).toContainText(
      "the counts below may be short",
    );
    await expect(page.getByTestId("startup-snapshot-note")).toHaveCount(0);
    // This log logged a patch result, so it is not cut off before the patch phase.
    await expect(page.getByTestId("startup-cut-off-note")).toHaveCount(0);
    await expect(page.getByTestId("startup-bad-dds-mod.000")).toHaveText("at least 1");
    await expect(page.getByTestId("startup-observed-timers-mod.000")).toContainText(
      "at least 1 (250 ms)",
    );
  });

  test("a Player.log cut off before any patch result, with no gaps, shows lower bounds and the cut-off note", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      window.__IMPORT_GAME_LOG_OVERRIDES__ = {
        coverage: {
          kind: "playerLog",
          passes: 1,
          patchPhase: "noneLogged",
          endState: { kind: "truncated" },
        },
      };
    });
    await importOnStartup(page);
    await scrollStartupTableToBottom(page);

    await expect(page.getByTestId("imported-log-gaps-warning")).toHaveCount(0);
    await expect(page.getByTestId("startup-gaps-note")).toHaveCount(0);
    await expect(page.getByTestId("startup-cut-off-note")).toContainText(
      "ends before any patch result was logged, and without a clean exit",
    );
    await expect(page.getByTestId("startup-bad-dds-mod.000")).toHaveText("at least 1");
    await expect(page.getByTestId("startup-observed-timers-mod.000")).toHaveText(
      "at least 1 (250 ms)",
    );
  });

  test("a console snapshot at the cap is labelled as only what the console held, and the startup columns say what they cannot", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      window.__IMPORT_GAME_LOG_OVERRIDES__ = {
        coverage: { kind: "consoleSnapshot", entries: 1000, fill: "atCap", headTruncated: false },
      };
    });
    await importOnStartup(page);
    await scrollStartupTableToBottom(page);

    await expect(page.getByTestId("imported-log-kind")).toHaveText("Console snapshot");
    await expect(page.getByTestId("imported-log-coverage")).toContainText(
      "Only the 1000 entries the console held, at the console's cap: older entries were dropped by the game",
    );
    await expect(page.getByTestId("startup-snapshot-note")).toContainText(
      "not the log of a whole game run",
    );
    const timersCell = page.getByTestId("startup-observed-timers-mod.000");
    // The dash is decorative; the reason is the cell's accessible text.
    await expect(timersCell.locator('[aria-hidden="true"]')).toHaveText("—");
    await expect(timersCell).toContainText("not available from a console snapshot");
    await expect(timersCell.locator("[aria-label]")).toHaveCount(0);
    await expect(page.getByTestId("startup-bad-dds-mod.000")).toHaveText("at least 1");
  });

  test("the dashboard shows the same summary under its import button", async ({ page }) => {
    await page.addInitScript(() => {
      window.__IMPORT_GAME_LOG_OVERRIDES__ = {
        coverage: { kind: "consoleSnapshot", entries: 392, fill: "belowCap", headTruncated: true },
      };
    });
    await loadScenario(page);
    await page.evaluate(() => {
      window.__PICK_FILE_RESULT__ = "C:/logs/midgame1.txt";
    });
    await page.getByTestId("import-game-log-button").click();

    await expect(page.getByTestId("imported-log-kind")).toHaveText("Console snapshot");
    await expect(page.getByTestId("imported-log-coverage")).toContainText(
      "The console held only 392 entries",
    );
    await expect(page.getByTestId("imported-log-coverage-note")).toContainText("starts mid-entry");
  });
});

test.describe("def-cache note", () => {
  test("does not appear when no active mod is a def-cache carrier", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("shell-apply-button").click();

    await expect(page.getByTestId("apply-dialog")).toBeVisible();
    await expect(page.getByTestId("apply-dialog-def-cache-note")).toHaveCount(0);
  });

  test("appears with the observed cache-build duration once a carrier is active and a log is imported", async ({
    page,
  }) => {
    // `get_def_cache_carrier` is fetched once, on the apply dialog's own
    // mount (part of the shell, mounted by `loadScenario` itself) — the
    // flag must be set before that first navigation, not after, since a
    // stale-time-cached query result won't otherwise pick it up.
    await page.addInitScript(() => {
      window.__DEF_CACHE_CARRIER_MOD_ID__ = "mod.010";
    });
    await loadScenario(page);

    await page.getByTestId("shell-apply-button").click();
    await expect(page.getByTestId("apply-dialog-def-cache-note")).toContainText(
      "keys its cache on the load order",
    );
    await expect(page.getByTestId("apply-dialog-def-cache-note")).not.toContainText("seconds");
    await page.getByTestId("apply-dialog-cancel").click();

    await page.evaluate(() => {
      window.__PICK_FILE_RESULT__ = "C:/RimWorld/Player.log";
    });
    await page.getByTestId("import-game-log-button").click();

    await page.getByTestId("shell-apply-button").click();
    await expect(page.getByTestId("apply-dialog-def-cache-note")).toContainText(
      "about 8 seconds on your last log",
    );
  });
});
