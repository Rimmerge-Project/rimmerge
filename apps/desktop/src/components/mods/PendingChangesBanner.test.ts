import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, describe, expect, it, vi } from "vitest";

import PendingChangesBanner from "@/components/mods/PendingChangesBanner.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { PendingActiveChangesDto } from "@/types/generated/PendingActiveChangesDto";

// `useToast()` mocked so the rescan-warnings toast test below can assert the exact toast
// payload `useRescanMutation` builds — see `SetupPage.test.ts`'s
// identical comment for why (no `<Toast />` is mounted here to render
// against).
const { toastAddSpy } = vi.hoisted(() => ({ toastAddSpy: vi.fn() }));
vi.mock("primevue/usetoast", () => ({
  useToast: () => ({ add: toastAddSpy }),
}));

const EMPTY: PendingActiveChangesDto = {
  unscanned: { added: [], removed: [] },
  unapplied: { added: [], removed: [] },
  orderDiffersFromFile: false,
};

function mountBanner(
  pending: PendingActiveChangesDto,
  rescan: (() => unknown) | Record<string, never> = {},
  more: Record<string, unknown> = {},
) {
  installMockIpc({
    get_pending_active_changes: pending,
    rescan_project: rescan,
    ...more,
  });
  return mount(PendingChangesBanner, {
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
    },
  });
}

describe("PendingChangesBanner", () => {
  afterEach(() => {
    clearMocks();
    toastAddSpy.mockClear();
  });

  it("stays hidden while the working set has no unscanned changes", async () => {
    const wrapper = mountBanner(EMPTY);
    await flushPromises();

    expect(wrapper.find('[data-testid="pending-changes-banner"]').exists()).toBe(false);
  });

  it("shows the unscanned count and, when present, the quieter unapplied line", async () => {
    const wrapper = mountBanner({
      unscanned: { added: ["a.mod", "b.mod"], removed: [] },
      unapplied: { added: ["c.mod"], removed: [] },
      orderDiffersFromFile: false,
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="pending-changes-unscanned-text"]').text()).toContain("2");
    expect(wrapper.get('[data-testid="pending-changes-unapplied-text"]').text()).toContain("1");
  });

  it("omits the unapplied line when there is nothing unapplied", async () => {
    const wrapper = mountBanner({
      unscanned: { added: ["a.mod"], removed: [] },
      unapplied: { added: [], removed: [] },
      orderDiffersFromFile: false,
    });
    await flushPromises();

    expect(wrapper.find('[data-testid="pending-changes-unapplied-text"]').exists()).toBe(false);
  });

  it("clicking Rescan calls rescan_project", async () => {
    let calls = 0;
    const wrapper = mountBanner(
      {
        unscanned: { added: ["a.mod"], removed: [] },
        unapplied: EMPTY.unapplied,
        orderDiffersFromFile: false,
      },
      () => {
        calls += 1;
        return {
          modCount: 1,
          gameVersion: "1.6",
          elapsedMs: 1,
          warnings: [],
          ruleWarnings: [],
          selected: "current",
        };
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="pending-changes-rescan-button"]').trigger("click");
    await flushPromises();

    expect(calls).toBe(1);
  });

  it("adopts the order the backend kept selected across the rescan", async () => {
    installMockIpc({
      get_pending_active_changes: {
        unscanned: { added: ["a.mod"], removed: [] },
        unapplied: EMPTY.unapplied,
      },
      rescan_project: {
        modCount: 1,
        gameVersion: "1.6",
        elapsedMs: 1,
        warnings: [],
        ruleWarnings: [],
        selected: "suggested",
      },
    });
    const pinia = createPinia();
    const session = useSessionStore(pinia);
    session.setSelected("current");
    const wrapper = mount(PendingChangesBanner, {
      global: {
        plugins: [pinia, PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      },
    });
    await flushPromises();

    await wrapper.get('[data-testid="pending-changes-rescan-button"]').trigger("click");
    await flushPromises();

    expect(session.selected).toBe("suggested");
  });

  it("toasts the rescan's own scan notes and rule warnings, like SetupPage does", async () => {
    const wrapper = mountBanner(
      {
        unscanned: { added: ["a.mod"], removed: [] },
        unapplied: EMPTY.unapplied,
        orderDiffersFromFile: false,
      },
      () => ({
        modCount: 1,
        gameVersion: "1.6",
        elapsedMs: 1,
        warnings: ["note one", "note two"],
        ruleWarnings: [{ kind: "modKnowledgeCacheUnreadable", reason: "bad json" }],
        selected: "current",
      }),
    );
    await flushPromises();

    await wrapper.get('[data-testid="pending-changes-rescan-button"]').trigger("click");
    await flushPromises();

    expect(toastAddSpy).toHaveBeenCalledWith(
      expect.objectContaining({ severity: "info", summary: "Rescan finished with 2 notes" }),
    );
    expect(toastAddSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        severity: "warn",
        summary: "Rescanned with warnings",
        detail:
          "Couldn't read the cached Rimmerge rules; using the bundled snapshot\nTechnical details: bad json",
      }),
    );
  });

  it("shows an error message when the rescan fails", async () => {
    const wrapper = mountBanner(
      {
        unscanned: { added: ["a.mod"], removed: [] },
        unapplied: EMPTY.unapplied,
        orderDiffersFromFile: false,
      },
      () => {
        throw { code: "internal", message: "the scan task panicked" };
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="pending-changes-rescan-button"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="pending-changes-rescan-error"]').text()).toContain(
      "the scan task panicked",
    );
  });

  it("offers Discard next to Rescan and calls discard_pending_changes", async () => {
    let calls = 0;
    const wrapper = mountBanner(
      {
        unscanned: { added: ["a.mod"], removed: [] },
        unapplied: EMPTY.unapplied,
        orderDiffersFromFile: false,
      },
      {},
      {
        discard_pending_changes: () => {
          calls += 1;
          return EMPTY;
        },
      },
    );
    await flushPromises();

    const button = wrapper.get('[data-testid="pending-changes-discard-button"]');
    expect(button.text()).toBe("Discard pending changes");
    await button.trigger("click");
    await flushPromises();

    expect(calls).toBe(1);
  });

  it("shows no Discard while nothing is pending", async () => {
    const wrapper = mountBanner(EMPTY);
    await flushPromises();

    expect(wrapper.find('[data-testid="pending-changes-discard-button"]').exists()).toBe(false);
  });

  it("shows a refused discard through the error mapping", async () => {
    const wrapper = mountBanner(
      {
        unscanned: { added: ["a.mod"], removed: [] },
        unapplied: EMPTY.unapplied,
        orderDiffersFromFile: false,
      },
      {},
      {
        discard_pending_changes: () => {
          throw { code: "no_project_loaded", message: "no project is loaded" };
        },
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="pending-changes-discard-button"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="pending-changes-rescan-error"]').text()).toContain(
      "No project loaded",
    );
  });
});
