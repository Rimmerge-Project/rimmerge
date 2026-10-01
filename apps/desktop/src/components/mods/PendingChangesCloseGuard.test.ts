import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import PendingChangesCloseGuard from "@/components/mods/PendingChangesCloseGuard.vue";
import { isStaleOrUnapplied, requestStaleConfirmation } from "@/composables/useStaleActiveSetGuard";
import { installMockIpc } from "@/services/ipc.mock";
import type { PendingActiveChangesDto } from "@/types/generated/PendingActiveChangesDto";

function mountGuard(pending: PendingActiveChangesDto) {
  installMockIpc({ get_pending_active_changes: pending });
  return mount(PendingChangesCloseGuard, {
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true },
    },
  });
}

describe("PendingChangesCloseGuard", () => {
  afterEach(() => clearMocks());

  it("keeps the module-scoped isStaleOrUnapplied() in sync with the query", async () => {
    mountGuard({
      unscanned: { added: ["a.mod"], removed: [] },
      unapplied: { added: [], removed: [] },
    });
    await flushPromises();

    expect(isStaleOrUnapplied()).toBe(true);
  });

  it("reads true when only unapplied (never unscanned) has anything pending", async () => {
    // A scanned-but-not-yet-applied order (an
    // unwritten `apply`) is just as much "work to lose on close" as an
    // unscanned working-set change — this pins that arm, since every other
    // test in this file only ever varies `unscanned`.
    mountGuard({
      unscanned: { added: [], removed: [] },
      unapplied: { added: ["a.mod"], removed: [] },
    });
    await flushPromises();

    expect(isStaleOrUnapplied()).toBe(true);
  });

  it("reads false once neither diff has anything pending", async () => {
    mountGuard({
      unscanned: { added: [], removed: [] },
      unapplied: { added: [], removed: [] },
    });
    await flushPromises();

    expect(isStaleOrUnapplied()).toBe(false);
  });

  it("Stay resolves the confirmation false without closing; Leave anyway resolves true", async () => {
    const wrapper = mountGuard({
      unscanned: { added: ["a.mod"], removed: [] },
      unapplied: { added: [], removed: [] },
    });
    await flushPromises();

    const pending = requestStaleConfirmation();
    await flushPromises();
    expect(wrapper.get('[data-testid="stale-close-confirm-dialog"]').text()).toContain(
      "Unsaved active-mod changes",
    );

    await wrapper.get('[data-testid="stale-close-confirm-stay"]').trigger("click");
    await expect(pending).resolves.toBe(false);
  });

  it("Leave anyway resolves the confirmation true", async () => {
    const wrapper = mountGuard({
      unscanned: { added: ["a.mod"], removed: [] },
      unapplied: { added: [], removed: [] },
    });
    await flushPromises();

    const pending = requestStaleConfirmation();
    await flushPromises();
    await wrapper.get('[data-testid="stale-close-confirm-leave"]').trigger("click");

    await expect(pending).resolves.toBe(true);
  });
});
