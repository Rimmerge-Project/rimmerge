import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import OrderSwitchedNote from "@/components/mods/OrderSwitchedNote.vue";
import { createAppI18n, registerAppI18n } from "@/i18n/i18n";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";

registerAppI18n(createAppI18n());

const NOTE = '[data-testid="order-switched-note"]';

/** A note with its `visible` model wired the way a parent's `v-model` would. */
function mountNote() {
  installMockIpc({ select_order: {} });
  const pinia = createPinia();
  const wrapper = mount(OrderSwitchedNote, {
    props: {
      visible: true,
      "onUpdate:visible": (isVisible: boolean | undefined) => {
        void wrapper.setProps({ visible: isVisible ?? false });
      },
    },
    global: { plugins: [pinia, PiniaColada, [PrimeVue, { theme: { preset: Aura } }]] },
  });
  return { wrapper, session: useSessionStore(pinia) };
}

describe("OrderSwitchedNote", () => {
  afterEach(() => {
    clearMocks();
  });

  it("shows while the note is requested and the own order is selected", () => {
    const { wrapper } = mountNote();

    expect(wrapper.find(NOTE).exists()).toBe(true);
  });

  it("stays gone after the suggested order is selected by any route, even once Current is back", async () => {
    const { wrapper, session } = mountNote();

    // Not through the note's own button: the shell's order switch lands here too.
    session.setSelected("suggested");
    await flushPromises();
    expect(wrapper.find(NOTE).exists()).toBe(false);

    session.setSelected("current");
    await flushPromises();

    expect(wrapper.find(NOTE).exists()).toBe(false);
    expect(wrapper.props("visible")).toBe(false);
  });

  it("View suggested selects the suggested order through the backend and ends the note", async () => {
    const { wrapper, session } = mountNote();

    await wrapper.get('[data-testid="order-switched-view-suggested"]').trigger("click");
    await flushPromises();

    expect(session.selected).toBe("suggested");
    expect(wrapper.find(NOTE).exists()).toBe(false);
    expect(wrapper.props("visible")).toBe(false);
  });
});
