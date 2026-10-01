import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, describe, expect, it, vi } from "vitest";

import ImportGameLogButton from "@/components/startup/ImportGameLogButton.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import { snapshotCoverage } from "@/utils/gameLogSummary.test-support";

const { pickFileMock } = vi.hoisted(() => ({ pickFileMock: vi.fn() }));
vi.mock("@/services/dialogs", () => ({ pickFile: pickFileMock }));

const SUMMARY_FIXTURE: GameLogSummaryDto = {
  patchFailures: [],
  extraStackTraces: [],
  crossReferences: [],
  ddsFailures: [],
  dependencyWarnings: [],
  timers: [],
  defCacheLines: [],
  loadEvents: [{ line: 1, kind: { type: "newGame" }, mods: ["a.mod"] }],
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

function mountButton() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(ImportGameLogButton, {
    global: {
      plugins: [pinia, PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
    },
  });
  return { wrapper, session: useSessionStore() };
}

describe("ImportGameLogButton", () => {
  afterEach(() => {
    clearMocks();
    pickFileMock.mockReset();
  });

  it("does nothing when the user cancels the file picker", async () => {
    pickFileMock.mockResolvedValue(null);
    installMockIpc({});
    const { wrapper, session } = mountButton();

    await wrapper.get('[data-testid="import-game-log-button"]').trigger("click");
    await flushPromises();

    expect(session.gameLogSummary).toBeNull();
    expect(wrapper.find('[data-testid="import-game-log-error"]').exists()).toBe(false);
  });

  it("imports the picked file and stores the summary on the session store", async () => {
    pickFileMock.mockResolvedValue("C:/RimWorld/Player.log");
    const calls: { path: string }[] = [];
    installMockIpc({
      import_game_log: (payload: unknown) => {
        calls.push((payload as { request: { path: string } }).request);
        return SUMMARY_FIXTURE;
      },
    });
    const { wrapper, session } = mountButton();

    await wrapper.get('[data-testid="import-game-log-button"]').trigger("click");
    await flushPromises();

    expect(calls).toEqual([{ path: "C:/RimWorld/Player.log" }]);
    expect(session.gameLogSummary).toEqual(SUMMARY_FIXTURE);
  });

  it("hands the picker its filter labels from the catalogue", async () => {
    pickFileMock.mockResolvedValue(null);
    installMockIpc({});
    const { wrapper } = mountButton();

    await wrapper.get('[data-testid="import-game-log-button"]').trigger("click");
    await flushPromises();

    expect(pickFileMock).toHaveBeenCalledWith({
      logFiles: "RimWorld log",
      allFiles: "All files",
    });
  });

  it("accepts a console snapshot: the result is stored labelled with its coverage, not refused", async () => {
    pickFileMock.mockResolvedValue("C:/logs/midgame1.txt");
    const snapshot: GameLogSummaryDto = { ...SUMMARY_FIXTURE, coverage: snapshotCoverage(1000) };
    installMockIpc({ import_game_log: () => snapshot });
    const { wrapper, session } = mountButton();

    await wrapper.get('[data-testid="import-game-log-button"]').trigger("click");
    await flushPromises();

    expect(session.gameLogSummary?.coverage).toEqual(snapshotCoverage(1000));
    expect(wrapper.find('[data-testid="import-game-log-error"]').exists()).toBe(false);
  });

  it("shows an error message when the import fails, without touching the session store", async () => {
    pickFileMock.mockResolvedValue("C:/RimWorld/Player.log");
    installMockIpc({
      import_game_log: () => {
        throw { code: "invalid_input", message: "not a Player.log", detail: null };
      },
    });
    const { wrapper, session } = mountButton();

    await wrapper.get('[data-testid="import-game-log-button"]').trigger("click");
    await flushPromises();

    // Localized by code (`invalid_input`'s own title/detail), with the
    // backend's raw message kept as a "technical details" line —
    // `utils/errors.ts`'s `describeCommandError`, not the raw
    // `error.message` alone.
    const errorText = wrapper.get('[data-testid="import-game-log-error"]').text();
    expect(errorText).toContain("Invalid input");
    expect(errorText).toContain("not a Player.log");
    expect(session.gameLogSummary).toBeNull();
  });
});
