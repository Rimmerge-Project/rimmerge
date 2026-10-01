import { createTestingPinia } from "@pinia/testing";
import { setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useSessionStore } from "@/stores/session";
import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import type { LoadEventDto } from "@/types/generated/LoadEventDto";
import { snapshotCoverage } from "@/utils/gameLogSummary.test-support";

function newGameEvent(mods: string[], line = 1): LoadEventDto {
  return { line, kind: { type: "newGame" }, mods };
}

function saveLoadEvent(saveName: string, mods: string[], line: number): LoadEventDto {
  return { line, kind: { type: "saveLoad", saveName }, mods };
}

function gameLogSummaryFixture(loadEvents: LoadEventDto[]): GameLogSummaryDto {
  return {
    patchFailures: [],
    extraStackTraces: [],
    crossReferences: [],
    ddsFailures: [],
    dependencyWarnings: [],
    timers: [],
    defCacheLines: [],
    loadEvents,
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
}

describe("useSessionStore", () => {
  beforeEach(() => {
    setActivePinia(createTestingPinia({ stubActions: false, createSpy: vi.fn }));
  });

  it("starts unloaded with no paths and the current order selected", () => {
    const session = useSessionStore();

    expect(session.loaded).toBe(false);
    expect(session.paths).toBeNull();
    expect(session.selected).toBe("current");
  });

  it("setLoaded records the paths and marks the session loaded", () => {
    const session = useSessionStore();
    const paths = {
      gameDir: "C:/Game",
      workshopDir: "C:/Workshop",
      modsConfig: "C:/Game/ModsConfig.xml",
      profileDir: "C:/Profile",
    };

    session.setLoaded(paths, "current");

    expect(session.loaded).toBe(true);
    expect(session.paths).toEqual(paths);
  });

  it("setLoaded takes the selected order from the backend, replacing a previous project's", () => {
    const session = useSessionStore();
    const paths = {
      gameDir: "C:/Game",
      workshopDir: "C:/Workshop",
      modsConfig: "C:/Game/ModsConfig.xml",
      profileDir: "C:/Profile",
    };
    session.setLoaded(paths, "current");

    session.setLoaded(paths, "suggested");

    expect(session.selected).toBe("suggested");
  });

  it("reset clears the loaded project and returns to the current order", () => {
    const session = useSessionStore();
    session.setLoaded(
      {
        gameDir: "C:/Game",
        workshopDir: "C:/Workshop",
        modsConfig: "C:/Game/ModsConfig.xml",
        profileDir: "C:/Profile",
      },
      "current",
    );
    session.setSelected("suggested");

    session.reset();

    expect(session.loaded).toBe(false);
    expect(session.paths).toBeNull();
    expect(session.selected).toBe("current");
  });

  it("markLostForReload keeps the paths but marks the session unloaded", () => {
    const session = useSessionStore();
    const paths = {
      gameDir: "C:/Game",
      workshopDir: "C:/Workshop",
      modsConfig: "C:/Game/ModsConfig.xml",
      profileDir: "C:/Profile",
    };
    session.setLoaded(paths, "current");

    const returned = session.markLostForReload();

    expect(returned).toEqual(paths);
    expect(session.paths).toEqual(paths);
    expect(session.loaded).toBe(false);
  });

  it("markLostForReload returns null when no project was ever loaded", () => {
    const session = useSessionStore();

    expect(session.markLostForReload()).toBeNull();
    expect(session.loaded).toBe(false);
  });

  it("starts with no scan notes", () => {
    const session = useSessionStore();

    expect(session.scanNotes).toEqual([]);
  });

  it("setScanNotes records the current load's notes", () => {
    const session = useSessionStore();
    const notes = [{ modId: "some.mod", message: "some/path: skipped: bad xml" }];

    session.setScanNotes(notes);

    expect(session.scanNotes).toEqual(notes);
  });

  it("reset also clears the scan notes", () => {
    const session = useSessionStore();
    session.setScanNotes([{ modId: "some.mod", message: "note" }]);

    session.reset();

    expect(session.scanNotes).toEqual([]);
  });

  it("starts with no imported game log", () => {
    const session = useSessionStore();

    expect(session.gameLogSummary).toBeNull();
  });

  it("setGameLogSummary records the last import_game_log result", () => {
    const session = useSessionStore();
    const summary = gameLogSummaryFixture([newGameEvent(["a.mod", "b.mod"])]);

    session.setGameLogSummary(summary);

    expect(session.gameLogSummary).toEqual(summary);
  });

  it("reset also clears the imported game log", () => {
    const session = useSessionStore();
    session.setGameLogSummary(gameLogSummaryFixture([newGameEvent(["a.mod"])]));

    session.reset();

    expect(session.gameLogSummary).toBeNull();
  });

  describe("loggedOrderDiffersFrom", () => {
    it("returns null when no log has been imported", () => {
      const session = useSessionStore();

      expect(session.loggedOrderDiffersFrom(["a.mod", "b.mod"])).toBeNull();
    });

    it("returns null when the imported log carried no order block at all", () => {
      const session = useSessionStore();
      session.setGameLogSummary(gameLogSummaryFixture([]));

      expect(session.loggedOrderDiffersFrom(["a.mod", "b.mod"])).toBeNull();
    });

    it("treats an event with no mods (a log that ran with zero mods) as a real, comparable order, distinct from no events", () => {
      // No load events ("can't compare orders") and an event with an
      // empty mod list ("ran with zero mods") are genuinely different
      // facts — only the former should ever read as "unknown".
      const session = useSessionStore();
      session.setGameLogSummary(gameLogSummaryFixture([newGameEvent([])]));

      expect(session.loggedOrderDiffersFrom(["a.mod", "b.mod"])).toBe(true);
      expect(session.loggedOrderDiffersFrom([])).toBe(false);
    });

    it("returns false when the logged order matches the active order exactly", () => {
      const session = useSessionStore();
      session.setGameLogSummary(gameLogSummaryFixture([newGameEvent(["a.mod", "b.mod"])]));

      expect(session.loggedOrderDiffersFrom(["a.mod", "b.mod"])).toBe(false);
    });

    it("compares against the last load event, not the first", () => {
      const session = useSessionStore();
      session.setGameLogSummary(
        gameLogSummaryFixture([
          newGameEvent(["a.mod", "b.mod"], 1),
          saveLoadEvent("SaveA", ["b.mod", "a.mod"], 40),
        ]),
      );

      expect(session.loggedOrderDiffersFrom(["b.mod", "a.mod"])).toBe(false);
      expect(session.loggedOrderDiffersFrom(["a.mod", "b.mod"])).toBe(true);
    });

    it("uses a console snapshot's last load event as evidence of the order, like a Player.log's", () => {
      const session = useSessionStore();
      session.setGameLogSummary({
        ...gameLogSummaryFixture([newGameEvent(["a.mod", "b.mod"])]),
        coverage: snapshotCoverage(1000),
      });

      expect(session.loggedOrderDiffersFrom(["a.mod", "b.mod"])).toBe(false);
      expect(session.loggedOrderDiffersFrom(["b.mod", "a.mod"])).toBe(true);
    });

    it("ignores the _steam suffix on order ids, since logged ids never carry it", () => {
      const session = useSessionStore();
      session.setGameLogSummary(gameLogSummaryFixture([newGameEvent(["a.mod", "b.mod"])]));

      expect(session.loggedOrderDiffersFrom(["a.mod_steam", "b.mod"])).toBe(false);
      expect(session.loggedOrderDiffersFrom(["b.mod_steam", "a.mod"])).toBe(true);
    });

    it("returns true when the logged order has a different length", () => {
      const session = useSessionStore();
      session.setGameLogSummary(gameLogSummaryFixture([newGameEvent(["a.mod", "b.mod"])]));

      expect(session.loggedOrderDiffersFrom(["a.mod"])).toBe(true);
    });

    it("returns true when the logged order has the same mods in a different sequence", () => {
      const session = useSessionStore();
      session.setGameLogSummary(gameLogSummaryFixture([newGameEvent(["a.mod", "b.mod"])]));

      expect(session.loggedOrderDiffersFrom(["b.mod", "a.mod"])).toBe(true);
    });
  });
});
