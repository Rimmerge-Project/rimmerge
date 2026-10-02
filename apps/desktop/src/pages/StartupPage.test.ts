import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import ToastService from "primevue/toastservice";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import StartupPage from "@/pages/StartupPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import type { ModCostRowDto } from "@/types/generated/ModCostRowDto";
import type { OrderRowDto } from "@/types/generated/OrderRowDto";
import { playerLogCoverage, snapshotCoverage } from "@/utils/gameLogSummary.test-support";

// `@tanstack/vue-virtual` measures the scroll container via
// `offsetWidth`/`offsetHeight`, which happy-dom always reports as `0` with
// no layout engine behind it — without a stubbed viewport height the
// virtualizer computes an empty visible range and every row-content
// assertion below would silently see zero rows. Same fix as
// `components/order/OrderTable.test.ts` uses for the identical reason.
beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    value: 400,
  });
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
    configurable: true,
    value: 600,
  });
});

afterEach(() => {
  clearMocks();
  Reflect.deleteProperty(HTMLElement.prototype, "offsetHeight");
  Reflect.deleteProperty(HTMLElement.prototype, "offsetWidth");
});

function costRow(overrides: Partial<ModCostRowDto> = {}): ModCostRowDto {
  return {
    modId: "a.mod",
    patchOps: 0,
    slowXpathOps: 0,
    textureFiles: 0,
    textureBytes: 0,
    ddsFiles: 0,
    assemblyCount: 0,
    assemblyBytes: 0,
    defCount: 0,
    contentOnly: false,
    overriddenTextureBytes: 0,
    ...overrides,
  };
}

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

function emptySummary(overrides: Partial<GameLogSummaryDto> = {}): GameLogSummaryDto {
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
    coverage: {
      kind: "playerLog",
      passes: 2,
      patchPhase: "noneLogged",
      endState: { kind: "cleanExit" },
    },
    loggingGaps: [],
    lowerBound: null,
    ...overrides,
  };
}

function mountPage(costs: ModCostRowDto[], orderRows: OrderRowDto[] = []) {
  installMockIpc({
    list_mod_names: {},
    get_startup_costs: costs,
    list_order: orderRows,
  });
  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(StartupPage, {
    global: { plugins: [pinia, PiniaColada, ToastService] },
  });
  return { wrapper, session: useSessionStore() };
}

describe("StartupPage", () => {
  it("puts every value in the column its header names", async () => {
    // Table layout places a cell by its index, so two swapped body cells would
    // still line up with the declared widths; only the value gives it away.
    const { wrapper, session } = mountPage([
      costRow({
        modId: "a.mod",
        patchOps: 11,
        slowXpathOps: 22,
        textureFiles: 33,
        textureBytes: 4096,
        ddsFiles: 55,
        assemblyCount: 77,
        assemblyBytes: 2048,
        defCount: 88,
        overriddenTextureBytes: 3072,
        contentOnly: true,
      }),
    ]);
    await flushPromises();
    const attribution = { kind: "mod", modId: "a.mod" } as const;
    const ddsFailure = { attribution, path: "x.dds", width: 3, height: 3, format: "BC7" };
    session.setGameLogSummary(
      emptySummary({
        ddsFailures: [ddsFailure, ddsFailure, ddsFailure],
        timers: [{ attribution, label: "load", milliseconds: 40, pass: 2 }],
      }),
    );
    await flushPromises();

    const expectedBySortKey: Record<string, string> = {
      patchOps: "11",
      slowXpathOps: "22",
      textureFiles: "33",
      textureBytes: "4.00 KiB",
      ddsFiles: "55",
      badDimensionDds: "3",
      assemblyCount: "77",
      assemblyBytes: "2.00 KiB",
      defCount: "88",
      overriddenTextureBytes: "3.00 KiB",
      observedTimers: "1 (40 ms)",
    };
    const cells = wrapper
      .get('[data-testid="startup-row-a.mod"]')
      .findAll("th, td")
      .map((cell) => cell.text().replace(/\s+/g, " "));
    const headers = wrapper.findAll("thead th");
    expect(cells).toHaveLength(headers.length);

    headers.forEach((header, index) => {
      const sortButton = header.find('[data-testid^="startup-sort-"]');
      if (!sortButton.exists()) {
        // The two columns without a sort button: the mod's name and the flag.
        expect(cells[index], header.text()).toBe(
          header.text() === "Mod" ? "a.mod" : "content-only",
        );
        return;
      }
      const sortKey = sortButton.attributes("data-testid")?.replace("startup-sort-", "") ?? "";
      expect(cells[index], sortKey).toBe(expectedBySortKey[sortKey]);
    });
  });

  it("shows a totals row summing every mod's own columns", async () => {
    const { wrapper } = mountPage([
      costRow({ modId: "a.mod", textureBytes: 100, patchOps: 2, defCount: 5 }),
      costRow({ modId: "b.mod", textureBytes: 200, patchOps: 3, defCount: 7 }),
    ]);
    await flushPromises();

    const totals = wrapper.get('[data-testid="startup-totals-row"]');
    expect(totals.text()).toContain("Total (2 mods)");
    expect(totals.text()).toContain("300 B");
    expect(totals.text()).toContain("5");
    expect(totals.text()).toContain("12");
  });

  it("defaults to sorting by texture bytes, descending", async () => {
    const { wrapper } = mountPage([
      costRow({ modId: "small.mod", textureBytes: 100 }),
      costRow({ modId: "big.mod", textureBytes: 900 }),
    ]);
    await flushPromises();

    const rows = wrapper
      .findAll('[data-testid^="startup-row-"]')
      .map((row) => row.attributes("data-testid"));
    expect(rows).toEqual(["startup-row-big.mod", "startup-row-small.mod"]);
  });

  it("sorts by a different column when its header is clicked, and reverses on a second click", async () => {
    const { wrapper } = mountPage([
      costRow({ modId: "few-defs.mod", defCount: 1, textureBytes: 900 }),
      costRow({ modId: "many-defs.mod", defCount: 9, textureBytes: 100 }),
    ]);
    await flushPromises();

    await wrapper.get('[data-testid="startup-sort-defCount"]').trigger("click");
    await flushPromises();
    expect(
      wrapper.findAll('[data-testid^="startup-row-"]').map((row) => row.attributes("data-testid")),
    ).toEqual(["startup-row-many-defs.mod", "startup-row-few-defs.mod"]);

    await wrapper.get('[data-testid="startup-sort-defCount"]').trigger("click");
    await flushPromises();
    expect(
      wrapper.findAll('[data-testid^="startup-row-"]').map((row) => row.attributes("data-testid")),
    ).toEqual(["startup-row-few-defs.mod", "startup-row-many-defs.mod"]);
  });

  it("renders the bad-dimension-DDS and observed-timer columns as 'not imported' before any log import", async () => {
    const { wrapper } = mountPage([costRow({ modId: "a.mod" })]);
    await flushPromises();

    expect(wrapper.get('[data-testid="startup-bad-dds-a.mod"]').text()).toBe("not imported");
    expect(wrapper.get('[data-testid="startup-observed-timers-a.mod"]').text()).toBe(
      "not imported",
    );
  });

  it("renders real counts, not 'not imported', once a log has been imported", async () => {
    const { wrapper, session } = mountPage([costRow({ modId: "a.mod" })]);
    await flushPromises();
    session.setGameLogSummary(
      emptySummary({
        ddsFailures: [
          {
            attribution: { kind: "mod", modId: "a.mod" },
            path: "x.dds",
            width: 3,
            height: 3,
            format: "BC7",
          },
        ],
        timers: [
          {
            attribution: { kind: "mod", modId: "a.mod" },
            label: "load",
            milliseconds: 40,
            pass: 2,
          },
        ],
      }),
    );
    await flushPromises();

    expect(wrapper.get('[data-testid="startup-bad-dds-a.mod"]').text()).toBe("1");
    expect(wrapper.get('[data-testid="startup-observed-timers-a.mod"]').text()).toContain(
      "1 (40 ms)",
    );
  });

  it("shows the different-order warning only when the imported log's order differs from the selected one", async () => {
    const { wrapper, session } = mountPage(
      [costRow({ modId: "a.mod" })],
      [orderRow("a.mod", 0), orderRow("b.mod", 1)],
    );
    await flushPromises();
    expect(wrapper.find('[data-testid="startup-order-mismatch-warning"]').exists()).toBe(false);

    session.setGameLogSummary(
      emptySummary({
        loadEvents: [{ line: 1, kind: { type: "newGame" }, mods: ["a.mod", "b.mod"] }],
      }),
    );
    await flushPromises();
    expect(wrapper.find('[data-testid="startup-order-mismatch-warning"]').exists()).toBe(false);

    session.setGameLogSummary(
      emptySummary({
        loadEvents: [{ line: 1, kind: { type: "newGame" }, mods: ["b.mod", "a.mod"] }],
      }),
    );
    await flushPromises();
    expect(wrapper.find('[data-testid="startup-order-mismatch-warning"]').exists()).toBe(true);
  });

  it("shows the 'can't be checked' warning, not the mismatch one, when the log carries no order block at all", async () => {
    // A real log shape: a real `Player.log` can have no
    // load block at all (neither `Initializing new game with mods:` nor
    // `Loading game from file ... with mods:`) — distinct from
    // an empty one, and the page must say so rather than silently
    // reading as "the orders match".
    const { wrapper, session } = mountPage([costRow({ modId: "a.mod" })], [orderRow("a.mod", 0)]);
    await flushPromises();

    session.setGameLogSummary(emptySummary({ loadEvents: [] }));
    await flushPromises();

    expect(wrapper.find('[data-testid="startup-order-mismatch-warning"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="startup-order-unknown-warning"]').text()).toContain(
      "carries no active-mod list",
    );
  });

  it("shows neither order warning while the selected order's own list_order query is still pending", async () => {
    // `list_order` resolving late
    // must never read as an empty active order and false-positive a
    // "differs" warning against a real (non-empty) logged order.
    installMockIpc({
      list_mod_names: {},
      get_startup_costs: [costRow({ modId: "a.mod" })],
      // `list_order` never resolves in this test — `orderRows.value`
      // stays `undefined` throughout — the pending state.
      list_order: () => new Promise(() => {}),
    });
    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mount(StartupPage, { global: { plugins: [pinia, PiniaColada, ToastService] } });
    const session = useSessionStore();
    await flushPromises();

    session.setGameLogSummary(
      emptySummary({
        loadEvents: [{ line: 1, kind: { type: "newGame" }, mods: ["a.mod", "b.mod"] }],
      }),
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="startup-order-mismatch-warning"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="startup-order-unknown-warning"]').exists()).toBe(false);
  });

  describe("with a console snapshot imported", () => {
    const badDds = {
      attribution: { kind: "mod", modId: "a.mod" } as const,
      path: "x.dds",
      width: 3,
      height: 3,
      format: "BC7",
    };
    const timer = {
      attribution: { kind: "mod", modId: "a.mod" } as const,
      label: "load",
      milliseconds: 40,
      pass: 2,
    };

    it("labels the import as a snapshot and explains what the columns can and cannot say", async () => {
      const { wrapper, session } = mountPage([costRow({ modId: "a.mod" })]);
      await flushPromises();

      session.setGameLogSummary(
        emptySummary({ coverage: snapshotCoverage(1000), ddsFailures: [badDds], timers: [timer] }),
      );
      await flushPromises();

      expect(wrapper.get('[data-testid="imported-log-kind"]').text()).toBe("Console snapshot");
      expect(wrapper.get('[data-testid="startup-snapshot-note"]').text()).toContain(
        "not the log of a whole game run",
      );
    });

    it("never shows observed timers for a snapshot, even one that holds a timer line", async () => {
      const { wrapper, session } = mountPage([costRow({ modId: "a.mod" })]);
      await flushPromises();

      session.setGameLogSummary(
        emptySummary({ coverage: snapshotCoverage(1000), timers: [timer] }),
      );
      await flushPromises();

      const cell = wrapper.get('[data-testid="startup-observed-timers-a.mod"]');
      // The dash is decorative; the reason is the cell's accessible text.
      expect(cell.get('[aria-hidden="true"]').text()).toBe("—");
      expect(cell.get(".sr-only").text()).toBe("not available from a console snapshot");
      expect(cell.find("[aria-label]").exists()).toBe(false);
    });

    it("shows bad-dimension DDS counts as lower bounds, including for a mod with none", async () => {
      const { wrapper, session } = mountPage([
        costRow({ modId: "a.mod", textureBytes: 2 }),
        costRow({ modId: "b.mod", textureBytes: 1 }),
      ]);
      await flushPromises();

      session.setGameLogSummary(
        emptySummary({ coverage: snapshotCoverage(1000), ddsFailures: [badDds] }),
      );
      await flushPromises();

      expect(wrapper.get('[data-testid="startup-bad-dds-a.mod"]').text()).toBe("at least 1");
      expect(wrapper.get('[data-testid="startup-bad-dds-b.mod"]').text()).toBe("at least 0");
    });

    it("keeps checking the snapshot's last load event against the selected order", async () => {
      const { wrapper, session } = mountPage(
        [costRow({ modId: "a.mod" })],
        [orderRow("a.mod", 0), orderRow("b.mod", 1)],
      );
      await flushPromises();

      session.setGameLogSummary(
        emptySummary({
          coverage: snapshotCoverage(400),
          loadEvents: [{ line: 1, kind: { type: "newGame" }, mods: ["b.mod", "a.mod"] }],
        }),
      );
      await flushPromises();

      expect(wrapper.find('[data-testid="startup-order-mismatch-warning"]').exists()).toBe(true);
    });

    it("does not show the snapshot note for a Player.log", async () => {
      const { wrapper, session } = mountPage([costRow({ modId: "a.mod" })]);
      await flushPromises();

      session.setGameLogSummary(emptySummary());
      await flushPromises();

      expect(wrapper.find('[data-testid="startup-snapshot-note"]').exists()).toBe(false);
      expect(wrapper.get('[data-testid="imported-log-kind"]').text()).toBe("Player.log");
    });
  });

  describe("with a Player.log that has a logging gap", () => {
    const badDds = {
      attribution: { kind: "mod", modId: "a.mod" },
      path: "x.dds",
      width: 3,
      height: 3,
      format: "BC7",
    } as const;
    const timer = {
      attribution: { kind: "mod", modId: "a.mod" },
      label: "load",
      milliseconds: 120,
      pass: 2,
    } as const;
    const gap = { stopLine: 9, end: { kind: "neverResumed" } } as const;

    it("shows timers and bad-dimension DDS counts as lower bounds, with the gaps note", async () => {
      const { wrapper, session } = mountPage([costRow({ modId: "a.mod" })]);
      await flushPromises();

      session.setGameLogSummary(
        emptySummary({ loggingGaps: [gap], timers: [timer], ddsFailures: [badDds] }),
      );
      await flushPromises();

      expect(wrapper.get('[data-testid="startup-observed-timers-a.mod"]').text()).toBe(
        "at least 1 (120 ms)",
      );
      expect(wrapper.get('[data-testid="startup-bad-dds-a.mod"]').text()).toBe("at least 1");
      expect(wrapper.get('[data-testid="startup-gaps-note"]').text()).toContain("may be short");
      expect(wrapper.find('[data-testid="startup-snapshot-note"]').exists()).toBe(false);
    });

    it("shows a cut-off Player.log's figures as lower bounds with the cut-off note, before the gaps note", async () => {
      const { wrapper, session } = mountPage([costRow({ modId: "a.mod" })]);
      await flushPromises();

      session.setGameLogSummary(
        emptySummary({
          coverage: playerLogCoverage({ endState: { kind: "truncated" } }),
          loggingGaps: [gap],
          timers: [timer],
          ddsFailures: [badDds],
        }),
      );
      await flushPromises();

      expect(wrapper.get('[data-testid="startup-observed-timers-a.mod"]').text()).toBe(
        "at least 1 (120 ms)",
      );
      expect(wrapper.get('[data-testid="startup-cut-off-note"]').text()).toContain(
        "ends before any patch result was logged",
      );
      const notes = wrapper
        .findAll('[data-testid="startup-cut-off-note"], [data-testid="startup-gaps-note"]')
        .map((note) => note.attributes("data-testid"));
      expect(notes).toEqual(["startup-cut-off-note", "startup-gaps-note"]);
    });

    it("does not call a non-clean end lower bounds once a patch result was logged", async () => {
      const { wrapper, session } = mountPage([costRow({ modId: "a.mod" })]);
      await flushPromises();

      session.setGameLogSummary(
        emptySummary({
          coverage: playerLogCoverage({
            endState: { kind: "truncated" },
            patchPhase: "patchFailureLogged",
          }),
          timers: [timer],
        }),
      );
      await flushPromises();

      expect(wrapper.get('[data-testid="startup-observed-timers-a.mod"]').text()).toBe(
        "1 (120 ms)",
      );
      expect(wrapper.find('[data-testid="startup-cut-off-note"]').exists()).toBe(false);
    });

    it("keeps a gap-free Player.log's figures exact and shows no gaps note", async () => {
      const { wrapper, session } = mountPage([costRow({ modId: "a.mod" })]);
      await flushPromises();

      session.setGameLogSummary(emptySummary({ timers: [timer], ddsFailures: [badDds] }));
      await flushPromises();

      expect(wrapper.get('[data-testid="startup-observed-timers-a.mod"]').text()).toBe(
        "1 (120 ms)",
      );
      expect(wrapper.get('[data-testid="startup-bad-dds-a.mod"]').text()).toBe("1");
      expect(wrapper.find('[data-testid="startup-gaps-note"]').exists()).toBe(false);
    });
  });
});
