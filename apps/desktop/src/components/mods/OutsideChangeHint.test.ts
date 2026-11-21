// OutsideChangeHint: the note shown off the Mods page while the file changed outside Rimmerge.

import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import { flush } from "@/components/apply/ApplyDialog.test-support";
import OutsideChangeHint from "@/components/mods/OutsideChangeHint.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ExternalFileChangeDto } from "@/types/generated/ExternalFileChangeDto";

const STUB = { template: "<div />" };

async function mountHint(routeName: string, externalChange: ExternalFileChangeDto) {
  installMockIpc({
    get_mods_config_baseline: { state: { kind: "fresh" }, externalChange },
  });
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", name: "dashboard", component: STUB },
      { path: "/mods", name: "mods", component: STUB },
      { path: "/mods/:modId", name: "mod-detail", component: STUB },
    ],
  });
  await router.push({ name: routeName, params: routeName === "mod-detail" ? { modId: "a" } : {} });
  const wrapper = mount(OutsideChangeHint, {
    global: { plugins: [createPinia(), PiniaColada, router, PrimeVue] },
  });
  await flush();
  return wrapper;
}

describe("OutsideChangeHint", () => {
  afterEach(() => {
    clearMocks();
  });

  it("shows a link to the Mods page on another page while the file changed outside", async () => {
    const wrapper = await mountHint("dashboard", { kind: "reordered" });

    expect(wrapper.find('[data-testid="outside-change-hint"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="outside-change-hint-link"]').attributes("href")).toBe(
      "/mods",
    );
  });

  it("is hidden when nothing changed outside", async () => {
    const wrapper = await mountHint("dashboard", { kind: "none" });

    expect(wrapper.find('[data-testid="outside-change-hint"]').exists()).toBe(false);
  });

  it.each(["mods", "mod-detail"])(
    "is hidden on the Mods page (%s), which has its own note",
    async (name) => {
      const wrapper = await mountHint(name, { kind: "membership", added: 1, removed: 0 });

      expect(wrapper.find('[data-testid="outside-change-hint"]').exists()).toBe(false);
    },
  );
});
