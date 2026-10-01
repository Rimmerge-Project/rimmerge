import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import ModsPage from "@/pages/ModsPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ActivateRequestDto } from "@/types/generated/ActivateRequestDto";
import type { DeactivateRequestDto } from "@/types/generated/DeactivateRequestDto";
import type { ModPageDto } from "@/types/generated/ModPageDto";
import type { ModSummaryDto } from "@/types/generated/ModSummaryDto";

function mod(overrides: Partial<ModSummaryDto> = {}): ModSummaryDto {
  return {
    modId: "active.mod",
    name: "Active Mod",
    source: "local",
    tags: [],
    hardDependents: 0,
    missing: false,
    generated: null,
    workshopId: null,
    ...overrides,
  };
}

function page(items: ModSummaryDto[]): ModPageDto {
  return { total: items.length, items };
}

const activateCalls: ActivateRequestDto[] = [];
const deactivateCalls: DeactivateRequestDto[] = [];

function mountModsPage(fixtureOverrides: Record<string, unknown> = {}) {
  activateCalls.length = 0;
  deactivateCalls.length = 0;
  installMockIpc({
    list_mods: () => page([mod({ modId: "active.mod", name: "Active Mod" })]),
    list_inactive_mods: () => page([mod({ modId: "inactive.mod", name: "Inactive Mod" })]),
    list_mod_names: {},
    list_findings: { total: 0, items: [] },
    plan_activate_mods: {
      toAdd: ["inactive.mod"],
      unresolvableDependencies: {},
      alreadyActive: [],
    },
    activate_mods: (payload: unknown) => {
      activateCalls.push((payload as { request: ActivateRequestDto }).request);
      return {
        unscanned: { added: ["inactive.mod"], removed: [] },
        unapplied: { added: [], removed: [] },
      };
    },
    plan_deactivate_mods: {
      toRemove: ["active.mod"],
      dependentsStillActive: { "active.mod": ["dependent.mod"] },
      refused: [],
    },
    deactivate_mods: (payload: unknown) => {
      deactivateCalls.push((payload as { request: DeactivateRequestDto }).request);
      return {
        unscanned: { added: [], removed: ["active.mod"] },
        unapplied: { added: [], removed: [] },
      };
    },
    ...fixtureOverrides,
  });

  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/mods", name: "mods", component: ModsPage },
      { path: "/mods/:modId", name: "mod-detail", component: { template: "<div />" } },
    ],
  });

  return mount(
    { template: "<router-view />" },
    {
      global: {
        plugins: [
          createPinia(),
          PiniaColada,
          router,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
        // PrimeVue's `Dialog` renders through a real `<Teleport>` — see
        // `TheShell.test.ts`'s identical comment.
        stubs: { teleport: true },
      },
    },
  );
}

async function mountAndNavigate(fixtureOverrides: Record<string, unknown> = {}) {
  const wrapper = mountModsPage(fixtureOverrides);
  const router = wrapper.vm.$router;
  await router.push({ name: "mods" });
  await router.isReady();
  await flushPromises();
  return wrapper;
}

describe("ModsPage", () => {
  beforeEach(() => {
    // `@tanstack/vue-virtual` (via `ModTable`) — see `ModTable.test.ts`'s
    // identical comment.
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

  it("shows the Active tab by default with Deactivate disabled until a row is selected", async () => {
    const wrapper = await mountAndNavigate();

    wrapper.get('[data-testid="mod-row-active.mod"]');
    const deactivateButton = wrapper.get('[data-testid="deactivate-selected-button"]');
    expect(deactivateButton.attributes("disabled")).toBeDefined();

    await wrapper
      .get('[data-testid="mod-row-select-active.mod"] input[type="checkbox"]')
      .setValue(true);
    expect(deactivateButton.attributes("disabled")).toBeUndefined();
  });

  it("switches to the Inactive tab, showing the dependencies checkbox and its own rows", async () => {
    const wrapper = await mountAndNavigate();

    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();

    wrapper.get('[data-testid="mod-row-inactive.mod"]');
    expect(wrapper.find('[data-testid="activate-with-dependencies-checkbox"]').exists()).toBe(true);
    const activateButton = wrapper.get('[data-testid="activate-selected-button"]');
    expect(activateButton.attributes("disabled")).toBeDefined();
  });

  it("activating a selected inactive mod shows the plan, then commits with the exact payload", async () => {
    const wrapper = await mountAndNavigate();
    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();

    await wrapper
      .get('[data-testid="mod-row-select-inactive.mod"] input[type="checkbox"]')
      .setValue(true);
    await wrapper
      .get('[data-testid="activate-with-dependencies-checkbox"] input[type="checkbox"]')
      .setValue(true);
    await wrapper.get('[data-testid="activate-selected-button"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="activate-confirm-dialog"]').text()).toContain("inactive.mod");

    await wrapper.get('[data-testid="activate-confirm-submit"]').trigger("click");
    await flushPromises();

    expect(activateCalls).toEqual([{ ids: ["inactive.mod"], withDependencies: true }]);
  });

  it("shows an unresolvable dependency in the activate-plan dialog", async () => {
    const wrapper = await mountAndNavigate({
      plan_activate_mods: {
        toAdd: ["inactive.mod"],
        unresolvableDependencies: { "inactive.mod": ["ghost.mod"] },
        alreadyActive: [],
      },
    });
    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();

    await wrapper
      .get('[data-testid="mod-row-select-inactive.mod"] input[type="checkbox"]')
      .setValue(true);
    await wrapper.get('[data-testid="activate-selected-button"]').trigger("click");
    await flushPromises();

    const text = wrapper.get('[data-testid="activate-plan-unresolvable"]').text();
    expect(text).toContain("ghost.mod");
    expect(text).toContain("which is not on disk");
  });

  it("words several unresolvable dependencies in the plural", async () => {
    const wrapper = await mountAndNavigate({
      plan_activate_mods: {
        toAdd: ["inactive.mod"],
        unresolvableDependencies: { "inactive.mod": ["ghost.mod", "phantom.mod"] },
        alreadyActive: [],
      },
    });
    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();

    await wrapper
      .get('[data-testid="mod-row-select-inactive.mod"] input[type="checkbox"]')
      .setValue(true);
    await wrapper.get('[data-testid="activate-selected-button"]').trigger("click");
    await flushPromises();

    const text = wrapper.get('[data-testid="activate-plan-unresolvable"]').text();
    expect(text).toContain("declares dependencies on");
    expect(text).toContain("which are not on disk");
    expect(text).not.toContain("which is not on disk");
  });

  it("shows a refused (Core) mod in the deactivate-plan dialog", async () => {
    const wrapper = await mountAndNavigate({
      plan_deactivate_mods: {
        toRemove: [],
        dependentsStillActive: {},
        refused: ["ludeon.rimworld"],
      },
    });

    await wrapper
      .get('[data-testid="mod-row-select-active.mod"] input[type="checkbox"]')
      .setValue(true);
    await wrapper.get('[data-testid="deactivate-selected-button"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="deactivate-confirm-dialog"]').text()).toContain(
      "ludeon.rimworld",
    );
  });

  it("deactivating a selected active mod shows dependents, then commits with the exact payload", async () => {
    const wrapper = await mountAndNavigate();

    await wrapper
      .get('[data-testid="mod-row-select-active.mod"] input[type="checkbox"]')
      .setValue(true);
    await wrapper.get('[data-testid="deactivate-selected-button"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="deactivate-plan-to-remove"]').text()).toBe("active.mod");
    const dialogText = wrapper.get('[data-testid="deactivate-confirm-dialog"]').text();
    expect(dialogText).toContain("active.mod");
    expect(dialogText).toContain("dependent.mod");

    await wrapper.get('[data-testid="deactivate-confirm-submit"]').trigger("click");
    await flushPromises();

    expect(deactivateCalls).toEqual([{ ids: ["active.mod"] }]);
  });

  it("Cancel on the activate dialog leaves the working set untouched", async () => {
    const wrapper = await mountAndNavigate();
    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();

    await wrapper
      .get('[data-testid="mod-row-select-inactive.mod"] input[type="checkbox"]')
      .setValue(true);
    await wrapper.get('[data-testid="activate-selected-button"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-testid="activate-confirm-cancel"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="activate-confirm-dialog"]').exists()).toBe(false);
    expect(activateCalls).toEqual([]);
  });

  it("Cancel on the deactivate dialog leaves the working set untouched", async () => {
    const wrapper = await mountAndNavigate();

    await wrapper
      .get('[data-testid="mod-row-select-active.mod"] input[type="checkbox"]')
      .setValue(true);
    await wrapper.get('[data-testid="deactivate-selected-button"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-testid="deactivate-confirm-cancel"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="deactivate-confirm-dialog"]').exists()).toBe(false);
    expect(deactivateCalls).toEqual([]);
  });

  it("shows an error in the activate dialog when the commit fails", async () => {
    const wrapper = await mountAndNavigate({
      activate_mods: () => {
        throw { code: "invalid_input", message: "ghost.mod is not a known mod" };
      },
    });
    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();

    await wrapper
      .get('[data-testid="mod-row-select-inactive.mod"] input[type="checkbox"]')
      .setValue(true);
    await wrapper.get('[data-testid="activate-selected-button"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-testid="activate-confirm-submit"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="activate-confirm-error"]').text()).toContain(
      "ghost.mod is not a known mod",
    );
  });

  it("shows an error in the deactivate dialog when the commit fails", async () => {
    const wrapper = await mountAndNavigate({
      deactivate_mods: () => {
        throw { code: "invalid_input", message: "cannot deactivate Core" };
      },
    });

    await wrapper
      .get('[data-testid="mod-row-select-active.mod"] input[type="checkbox"]')
      .setValue(true);
    await wrapper.get('[data-testid="deactivate-selected-button"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-testid="deactivate-confirm-submit"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="deactivate-confirm-error"]').text()).toContain(
      "cannot deactivate Core",
    );
  });
});
