import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, describe, expect, it } from "vitest";

import { dashboardFixture } from "@/components/apply/ApplyDialog.test-support";
import OrderNotInFileNote from "@/components/order/OrderNotInFileNote.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { PendingActiveChangesDto } from "@/types/generated/PendingActiveChangesDto";

const SETTLED_REORDER: PendingActiveChangesDto = {
  unscanned: { added: [], removed: [] },
  unapplied: { added: [], removed: [] },
  orderDiffersFromFile: true,
};

function mountNote(
  pending: PendingActiveChangesDto,
  options: { allowRevert: boolean; revert?: () => unknown } = { allowRevert: false },
) {
  installMockIpc({
    get_dashboard: { ...dashboardFixture(0), fileMatchesCurrent: false },
    get_pending_active_changes: pending,
    revert_order_to_file:
      options.revert ?? (() => ({ ...SETTLED_REORDER, orderDiffersFromFile: false })),
  });
  return mount(OrderNotInFileNote, {
    props: { allowRevert: options.allowRevert },
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      stubs: { teleport: true },
    },
  });
}

describe("OrderNotInFileNote revert", () => {
  afterEach(() => {
    clearMocks();
  });

  it("offers going back to the file's order for a settled reorder, where the page allows it", async () => {
    const wrapper = mountNote(SETTLED_REORDER, { allowRevert: true });
    await flushPromises();

    expect(wrapper.get('[data-testid="order-not-in-file-revert"]').text()).toBe(
      "Revert to your saved active order",
    );
    // The file is named only in the tooltip, where the user can see what the button acts on.
    expect(wrapper.get('[data-testid="order-not-in-file-revert"]').attributes("title")).toBe(
      "Puts your order back to the sequence saved in ModsConfig.xml.",
    );
  });

  it("does not offer it on a page that does not ask for it", async () => {
    const wrapper = mountNote(SETTLED_REORDER);
    await flushPromises();

    wrapper.get('[data-testid="order-not-in-file-apply"]');
    expect(wrapper.find('[data-testid="order-not-in-file-revert"]').exists()).toBe(false);
  });

  it("does not offer it when the file lists other mods, which would be refused", async () => {
    const wrapper = mountNote(
      { ...SETTLED_REORDER, unapplied: { added: ["new.mod"], removed: [] } },
      { allowRevert: true },
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="order-not-in-file-revert"]').exists()).toBe(false);
  });

  it("does not offer it while edits wait for a rescan", async () => {
    const wrapper = mountNote(
      { ...SETTLED_REORDER, unscanned: { added: ["new.mod"], removed: [] } },
      { allowRevert: true },
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="order-not-in-file-revert"]').exists()).toBe(false);
  });

  it("does not offer it when only the order differs in the dashboard's reading, not the pending one", async () => {
    const wrapper = mountNote(
      { ...SETTLED_REORDER, orderDiffersFromFile: false },
      { allowRevert: true },
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="order-not-in-file-revert"]').exists()).toBe(false);
  });

  it("calls revert_order_to_file when clicked", async () => {
    let calls = 0;
    const wrapper = mountNote(SETTLED_REORDER, {
      allowRevert: true,
      revert: () => {
        calls += 1;
        return { ...SETTLED_REORDER, orderDiffersFromFile: false };
      },
    });
    await flushPromises();

    await wrapper.get('[data-testid="order-not-in-file-revert"]').trigger("click");
    await flushPromises();

    expect(calls).toBe(1);
  });

  it("shows a refused revert through the error mapping", async () => {
    const wrapper = mountNote(SETTLED_REORDER, {
      allowRevert: true,
      revert: () => {
        throw { code: "revert_order_membership_differs", message: "different mods" };
      },
    });
    await flushPromises();

    await wrapper.get('[data-testid="order-not-in-file-revert"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="order-not-in-file-revert-error"]').text()).toContain(
      "The saved list has different mods",
    );
  });
});
