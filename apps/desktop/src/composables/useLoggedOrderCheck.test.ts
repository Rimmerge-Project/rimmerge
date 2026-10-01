import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { defineComponent } from "vue";

import { useLoggedOrderCheck } from "@/composables/useLoggedOrderCheck";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import type { LoadEventDto } from "@/types/generated/LoadEventDto";
import type { OrderRowDto } from "@/types/generated/OrderRowDto";

function orderRow(modId: string, position: number): OrderRowDto {
  return {
    modId,
    name: modId,
    position,
    previousPosition: null,
    tier: "core",
    tags: [],
    hardDependents: 0,
    needsInputCount: 0,
  };
}

function newGameEvent(mods: string[], line = 1): LoadEventDto {
  return { line, kind: { type: "newGame" }, mods };
}

function saveLoadEvent(saveName: string, mods: string[], line: number): LoadEventDto {
  return { line, kind: { type: "saveLoad", saveName }, mods };
}

function summary(loadEvents: LoadEventDto[]): GameLogSummaryDto {
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

function mountHarness(orderRows: OrderRowDto[]) {
  installMockIpc({ list_order: orderRows });
  const pinia = createPinia();
  setActivePinia(pinia);
  const session = useSessionStore();

  let composable!: ReturnType<typeof useLoggedOrderCheck>;
  const Harness = defineComponent({
    setup() {
      composable = useLoggedOrderCheck();
      return {};
    },
    template: "<div />",
  });

  const wrapper = mount(Harness, { global: { plugins: [pinia, PiniaColada] } });
  return {
    wrapper,
    session,
    get check() {
      return composable;
    },
  };
}

describe("useLoggedOrderCheck", () => {
  afterEach(() => clearMocks());

  it("reports nothing (not even pending) while the order query hasn't resolved yet", () => {
    const { check } = mountHarness([orderRow("a.mod", 0)]);

    expect(check.pending.value).toBe(true);
    expect(check.differs.value).toBe(false);
    expect(check.noOrderBlock.value).toBe(false);
  });

  it("stays quiet once the order resolves if no log has been imported at all", async () => {
    const { check } = mountHarness([orderRow("a.mod", 0)]);
    await flushPromises();

    expect(check.pending.value).toBe(false);
    expect(check.differs.value).toBe(false);
    expect(check.noOrderBlock.value).toBe(false);
  });

  it("flags noOrderBlock when a log is imported but carries no active-mod list at all", async () => {
    const { check, session } = mountHarness([orderRow("a.mod", 0)]);
    await flushPromises();

    session.setGameLogSummary(summary([]));

    expect(check.noOrderBlock.value).toBe(true);
    expect(check.differs.value).toBe(false);
  });

  it("flags differs when the logged order doesn't match the selected one", async () => {
    const { check, session } = mountHarness([orderRow("a.mod", 0), orderRow("b.mod", 1)]);
    await flushPromises();

    session.setGameLogSummary(summary([newGameEvent(["b.mod", "a.mod"])]));

    expect(check.differs.value).toBe(true);
    expect(check.noOrderBlock.value).toBe(false);
  });

  it("flags neither when the logged order matches the selected one, including an empty logged order", async () => {
    const { check, session } = mountHarness([orderRow("a.mod", 0), orderRow("b.mod", 1)]);
    await flushPromises();

    session.setGameLogSummary(summary([newGameEvent(["a.mod", "b.mod"])]));

    expect(check.differs.value).toBe(false);
    expect(check.noOrderBlock.value).toBe(false);

    // An event with no mods (the log ran with zero mods) is a real,
    // distinct state from no events at all (no order block) — it never reads as
    // `noOrderBlock`, only as a genuine (and here, real) mismatch.
    session.setGameLogSummary(summary([newGameEvent([])]));
    expect(check.differs.value).toBe(true);
    expect(check.noOrderBlock.value).toBe(false);
  });

  it("compares the selected order against the log's last load event", async () => {
    const { check, session } = mountHarness([orderRow("a.mod", 0), orderRow("b.mod", 1)]);
    await flushPromises();

    // A session that loaded two saves: the first under a different order.
    session.setGameLogSummary(
      summary([
        newGameEvent(["b.mod", "a.mod"], 1),
        saveLoadEvent("SaveA", ["a.mod", "b.mod"], 40),
      ]),
    );
    expect(check.differs.value).toBe(false);

    session.setGameLogSummary(
      summary([
        newGameEvent(["a.mod", "b.mod"], 1),
        saveLoadEvent("SaveA", ["b.mod", "a.mod"], 40),
      ]),
    );
    expect(check.differs.value).toBe(true);
  });
});
