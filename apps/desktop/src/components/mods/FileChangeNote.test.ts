import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, describe, expect, it } from "vitest";

import { flush } from "@/components/apply/ApplyDialog.test-support";
import FileChangeNote from "@/components/mods/FileChangeNote.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { FileBaselineDto } from "@/types/generated/FileBaselineDto";
import type { PendingActiveChangesDto } from "@/types/generated/PendingActiveChangesDto";

const QUIET: FileBaselineDto = {
  state: { kind: "fresh" },
  externalChange: { kind: "none" },
  fileMatchesSelected: false,
  fileHoldsCurrentExactly: false,
};

const NO_PENDING: PendingActiveChangesDto = {
  unscanned: { added: [], removed: [] },
  unapplied: { added: [], removed: [] },
  orderUnapplied: false,
};

/** An own edit the user has not applied yet: a settled move. */
const OWN_MOVE: PendingActiveChangesDto = { ...NO_PENDING, orderUnapplied: true };

function mountNote(
  baseline: FileBaselineDto,
  more: Record<string, unknown> = {},
  pending: PendingActiveChangesDto = NO_PENDING,
) {
  installMockIpc({
    get_mods_config_baseline: baseline,
    get_pending_active_changes: pending,
    ...more,
  });
  const pinia = createPinia();
  const wrapper = mount(FileChangeNote, {
    global: {
      plugins: [pinia, PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      stubs: { teleport: true },
    },
  });
  return { wrapper, pinia };
}

describe("FileChangeNote", () => {
  afterEach(() => clearMocks());

  it("says nothing while the file is as Rimmerge left it", async () => {
    const { wrapper } = mountNote(QUIET);
    await flush();

    expect(wrapper.find('[data-testid="file-changed-outside"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="file-baseline-stale"]').exists()).toBe(false);
  });

  it("names the outside change in plain words and says pending changes are kept", async () => {
    const { wrapper } = mountNote(
      { ...QUIET, externalChange: { kind: "membership", added: 2, removed: 1 } },
      {},
      OWN_MOVE,
    );
    await flush();

    const text = wrapper.get('[data-testid="file-changed-outside-text"]').text();
    expect(text).toBe(
      "Your active mod list was changed outside Rimmerge (+2, -1). Your pending changes are kept. Applying would overwrite those outside changes.",
    );
    expect(text).not.toContain("ModsConfig.xml");
  });

  it("does not claim kept pending changes when there are none", async () => {
    const { wrapper } = mountNote({
      ...QUIET,
      externalChange: { kind: "membership", added: 2, removed: 1 },
    });
    await flush();

    expect(wrapper.get('[data-testid="file-changed-outside-text"]').text()).toBe(
      "Your active mod list was changed outside Rimmerge (+2, -1). Applying would overwrite those outside changes.",
    );
  });

  it("words an outside reorder on its own, without a count", async () => {
    const { wrapper } = mountNote(
      { ...QUIET, externalChange: { kind: "reordered" } },
      {},
      OWN_MOVE,
    );
    await flush();

    expect(wrapper.get('[data-testid="file-changed-outside-text"]').text()).toBe(
      "Your active order was changed outside Rimmerge. Your pending changes are kept. Applying would overwrite that order.",
    );
  });

  it("words an outside reorder without pending changes on its own too", async () => {
    const { wrapper } = mountNote({ ...QUIET, externalChange: { kind: "reordered" } });
    await flush();

    expect(wrapper.get('[data-testid="file-changed-outside-text"]').text()).toBe(
      "Your active order was changed outside Rimmerge. Applying would overwrite that order.",
    );
  });

  it.each([
    [
      "a membership change with pending changes",
      { kind: "membership", added: 2, removed: 1 },
      OWN_MOVE,
      "Your active mod list was changed outside Rimmerge (+2, -1). Your pending changes are kept. It already matches the order you have selected.",
    ],
    [
      "a membership change without pending changes",
      { kind: "membership", added: 2, removed: 1 },
      NO_PENDING,
      "Your active mod list was changed outside Rimmerge (+2, -1). It already matches the order you have selected.",
    ],
    [
      "a reorder with pending changes",
      { kind: "reordered" },
      OWN_MOVE,
      "Your active order was changed outside Rimmerge. Your pending changes are kept. It already matches the order you have selected.",
    ],
    [
      "a reorder without pending changes",
      { kind: "reordered" },
      NO_PENDING,
      "Your active order was changed outside Rimmerge. It already matches the order you have selected.",
    ],
  ] as const)(
    "does not claim an overwrite when the file already holds the selected order: %s",
    async (_name, externalChange, pending, expected) => {
      const { wrapper } = mountNote(
        { ...QUIET, externalChange, fileMatchesSelected: true },
        {},
        pending,
      );
      await flush();

      const text = wrapper.get('[data-testid="file-changed-outside-text"]').text();
      expect(text).toBe(expected);
      expect(text).not.toContain("overwrite");
    },
  );

  it("keeps the action to use the file's list even when the file holds the selected order", async () => {
    const { wrapper } = mountNote({
      ...QUIET,
      externalChange: { kind: "reordered" },
      fileMatchesSelected: true,
    });
    await flush();

    expect(wrapper.find('[data-testid="file-changed-outside-use"]').exists()).toBe(true);
  });

  it.each([
    ["the outside change", { kind: "reordered" }, "file-changed-outside"],
    ["the stale baseline", { kind: "none" }, "file-baseline-stale"],
  ] as const)(
    "renders %s as a quiet note, not an assertive alert",
    async (_name, externalChange, testId) => {
      const { wrapper } = mountNote({
        state:
          testId === "file-baseline-stale"
            ? { kind: "stale", reason: "missing" }
            : { kind: "fresh" },
        externalChange,
        fileMatchesSelected: false,
        fileHoldsCurrentExactly: false,
      });
      await flush();

      // The test transition stub carries the test id; the live region is the message inside it.
      const banner = wrapper.get(`[data-testid="${testId}"] .p-message`);
      expect(banner.attributes("role")).toBe("note");
      expect(banner.attributes("aria-live")).toBe("off");
    },
  );

  it.each([
    ["missing", "ModsConfig.xml is missing."],
    ["unreadable", "Rimmerge can't read ModsConfig.xml right now."],
    ["malformed", "ModsConfig.xml can't be understood right now"],
  ] as const)(
    "explains a %s file and that the marks use the last readable list",
    async (reason, start) => {
      const { wrapper } = mountNote({ ...QUIET, state: { kind: "stale", reason } });
      await flush();

      const note = wrapper.get('[data-testid="file-baseline-stale"]').text();
      expect(note).toContain(start);
      expect(note).toContain("last list");
    },
  );

  it("opens the import preview with the file's list, from the session's own file path", async () => {
    const previews: unknown[] = [];
    const { wrapper, pinia } = mountNote(
      { ...QUIET, externalChange: { kind: "membership", added: 1, removed: 0 } },
      {
        preview_order_import_file: (payload: unknown) => {
          previews.push(payload);
          return { kind: "rejected", reason: { kind: "unrecognizedFormat" } };
        },
      },
    );
    useSessionStore(pinia).setLoaded(
      {
        gameDir: "game",
        workshopDir: "workshop",
        modsConfig: "C:/profile/ModsConfig.xml",
        profileDir: "profile",
      },
      "current",
    );
    await flush();

    await wrapper.get('[data-testid="file-changed-outside-use"]').trigger("click");
    await flush();

    expect(previews).toEqual([{ request: { path: "C:/profile/ModsConfig.xml" } }]);
  });
});
