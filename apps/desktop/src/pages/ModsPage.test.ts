import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import Select from "primevue/select";
import ToastService from "primevue/toastservice";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import { modList, modRow } from "@/components/mods/modListRow.test-support";
import ModsPage from "@/pages/ModsPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ActivateRequestDto } from "@/types/generated/ActivateRequestDto";
import type { DeactivateRequestDto } from "@/types/generated/DeactivateRequestDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";

const toastAddSpy = vi.hoisted(() => vi.fn());
vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: toastAddSpy }) }));

const activateCalls: ActivateRequestDto[] = [];
const deactivateCalls: DeactivateRequestDto[] = [];
const planActivateCalls: ActivateRequestDto[] = [];
const listWorkingModsCalls: ModListQueryDto[] = [];
const listInactiveModsCalls: ModListQueryDto[] = [];

function mountModsPage(fixtureOverrides: Record<string, unknown> = {}) {
  activateCalls.length = 0;
  deactivateCalls.length = 0;
  planActivateCalls.length = 0;
  listWorkingModsCalls.length = 0;
  listInactiveModsCalls.length = 0;
  installMockIpc({
    list_working_mods: (payload: unknown) => {
      listWorkingModsCalls.push((payload as { query: ModListQueryDto }).query);
      return modList([modRow({ modId: "active.mod", name: "Active Mod" })]);
    },
    list_inactive_mods: (payload: unknown) => {
      listInactiveModsCalls.push((payload as { query: ModListQueryDto }).query);
      return modList([modRow({ modId: "inactive.mod", name: "Inactive Mod" })]);
    },
    list_mod_names: {},
    list_findings: { total: 0, items: [] },
    // The panel `mod-detail` mounts over the list; kept loading, since
    // these tests are about the list, not the panel.
    get_mod_info: () => new Promise(() => {}),
    plan_activate_mods: (payload: unknown) => {
      planActivateCalls.push((payload as { request: ActivateRequestDto }).request);
      return {
        toAdd: ["inactive.mod"],
        unresolvableDependencies: {},
        alreadyActive: [],
      };
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
      { path: "/elsewhere", name: "elsewhere", component: { template: "<div />" } },
      { path: "/mods", name: "mods", component: ModsPage },
      // As in `router.ts`: the mod info panel route renders the same page.
      { path: "/mods/:modId", name: "mod-detail", component: ModsPage },
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
    toastAddSpy.mockClear();
    vi.useRealTimers();
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

  it("queries the Active list unfiltered, with no paging", async () => {
    await mountAndNavigate();

    expect(listWorkingModsCalls).toEqual([{ search: null, tag: null, source: null }]);
    expect(listInactiveModsCalls).toEqual([]);
  });

  it("debounces the search term by 300ms before it reaches the Active query", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const wrapper = await mountAndNavigate();

    await wrapper.get('[data-testid="mods-search"]').setValue("Some");
    vi.advanceTimersByTime(299);
    await flushPromises();
    expect(listWorkingModsCalls.map((filter) => filter.search)).toEqual([null]);

    vi.advanceTimersByTime(1);
    await flushPromises();
    expect(listWorkingModsCalls.at(-1)).toEqual({
      search: "Some",
      tag: null,
      source: null,
    });
  });

  it("sends the tag and source filters to the Active query", async () => {
    const wrapper = await mountAndNavigate();

    await wrapper.get('[data-testid="mods-tag-filter"]').setValue("framework");
    await wrapper.findComponent(Select).vm.$emit("update:modelValue", "workshop");
    await flushPromises();

    expect(listWorkingModsCalls.at(-1)).toEqual({
      search: null,
      tag: "framework",
      source: "workshop",
    });
  });

  it("offers Any plus every source, worded, in the source filter", async () => {
    const wrapper = await mountAndNavigate();

    expect(wrapper.findComponent(Select).props("options")).toEqual([
      { label: "Any", value: null },
      { label: "Core", value: "core" },
      { label: "DLC", value: "dlc" },
      { label: "Local", value: "local" },
      { label: "Workshop", value: "workshop" },
    ]);
  });

  it("hides the tag filter on the Inactive tab and never sends a tag there", async () => {
    const wrapper = await mountAndNavigate();
    await wrapper.get('[data-testid="mods-tag-filter"]').setValue("framework");
    await wrapper.findComponent(Select).vm.$emit("update:modelValue", "local");

    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="mods-tag-filter"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="deactivate-selected-button"]').exists()).toBe(false);
    expect(listInactiveModsCalls.at(-1)).toEqual({
      search: null,
      tag: null,
      source: "local",
    });
  });

  it("shows the total and no page-cap hint when every row fits", async () => {
    const wrapper = await mountAndNavigate();

    expect(wrapper.get('[data-testid="mods-total"]').text()).toBe("1 mod");
    expect(wrapper.find('[data-testid="mods-page-cap-hint"]').exists()).toBe(false);
  });

  it("shows the page-cap hint when the backend holds more rows than it returned", async () => {
    const wrapper = await mountAndNavigate({
      list_working_mods: () => ({ total: 250, items: [modRow()] }),
    });

    expect(wrapper.get('[data-testid="mods-total"]').text()).toContain("250 mods");
    expect(wrapper.get('[data-testid="mods-page-cap-hint"]').text()).toBe(
      "(showing first 1 — narrow with search)",
    );
  });

  it("shows the Inactive tab's own total and page-cap hint", async () => {
    const wrapper = await mountAndNavigate({
      list_inactive_mods: () => ({ total: 3, items: [modRow({ modId: "inactive.mod" })] }),
    });

    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="mods-total"]').text()).toContain("3 mods");
    expect(wrapper.find('[data-testid="mods-page-cap-hint"]').exists()).toBe(true);
  });

  it("shows the query error message when the Active query fails", async () => {
    const wrapper = await mountAndNavigate({
      list_working_mods: () => {
        throw { code: "internal", message: "boom" };
      },
    });

    expect(wrapper.get('[data-testid="mods-error"]').text()).toBe("boom");
    expect(wrapper.find('[data-testid="mod-table"]').exists()).toBe(false);
  });

  it("keeps each tab's selection across a tab switch", async () => {
    const wrapper = await mountAndNavigate();
    await wrapper
      .get('[data-testid="mod-row-select-active.mod"] input[type="checkbox"]')
      .setValue(true);

    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();
    expect(
      wrapper.get('[data-testid="activate-selected-button"]').attributes("disabled"),
    ).toBeDefined();
    await wrapper.get('[data-testid="mods-tab-active"]').trigger("click");
    await flushPromises();

    expect(
      wrapper.get('[data-testid="deactivate-selected-button"]').attributes("disabled"),
    ).toBeUndefined();
  });

  it("plans the activation with the selected ids and the dependencies choice", async () => {
    const wrapper = await mountAndNavigate();
    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();
    await wrapper
      .get('[data-testid="mod-row-select-inactive.mod"] input[type="checkbox"]')
      .setValue(true);

    await wrapper.get('[data-testid="activate-selected-button"]').trigger("click");
    await flushPromises();

    expect(planActivateCalls).toEqual([{ ids: ["inactive.mod"], withDependencies: false }]);
  });

  it("a committed activation closes the dialog, clears the selection and toasts", async () => {
    const wrapper = await mountAndNavigate();
    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();
    await wrapper
      .get('[data-testid="mod-row-select-inactive.mod"] input[type="checkbox"]')
      .setValue(true);
    await wrapper.get('[data-testid="activate-selected-button"]').trigger("click");
    await flushPromises();

    await wrapper.get('[data-testid="activate-confirm-submit"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="activate-confirm-dialog"]').exists()).toBe(false);
    expect(
      wrapper.get('[data-testid="activate-selected-button"]').attributes("disabled"),
    ).toBeDefined();
    expect(toastAddSpy).toHaveBeenCalledWith({
      severity: "success",
      summary: "Activated",
      life: 4000,
    });
  });

  it("a committed deactivation closes the dialog, clears the selection and toasts", async () => {
    const wrapper = await mountAndNavigate();
    await wrapper
      .get('[data-testid="mod-row-select-active.mod"] input[type="checkbox"]')
      .setValue(true);
    await wrapper.get('[data-testid="deactivate-selected-button"]').trigger("click");
    await flushPromises();

    await wrapper.get('[data-testid="deactivate-confirm-submit"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="deactivate-confirm-dialog"]').exists()).toBe(false);
    expect(
      wrapper.get('[data-testid="deactivate-selected-button"]').attributes("disabled"),
    ).toBeDefined();
    expect(toastAddSpy).toHaveBeenCalledWith({
      severity: "success",
      summary: "Deactivated",
      life: 4000,
    });
  });

  it("a failed commit keeps the dialog open and the selection intact, with no toast", async () => {
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

    expect(wrapper.find('[data-testid="deactivate-confirm-dialog"]').exists()).toBe(true);
    expect(
      wrapper.get('[data-testid="deactivate-selected-button"]').attributes("disabled"),
    ).toBeUndefined();
    expect(toastAddSpy).not.toHaveBeenCalled();
  });

  it("clicking a row opens that mod's detail route", async () => {
    const wrapper = await mountAndNavigate();

    await wrapper.get('[data-testid="mod-row-active.mod"]').trigger("click");
    await flushPromises();

    expect(wrapper.vm.$router.currentRoute.value.fullPath).toBe("/mods/active.mod");
  });

  it("hides the active-only columns on the Inactive tab", async () => {
    const wrapper = await mountAndNavigate({
      list_inactive_mods: () =>
        modList([modRow({ modId: "inactive.mod", tags: ["framework"], hardDependents: 7 })]),
    });

    await wrapper.get('[data-testid="mods-tab-inactive"]').trigger("click");
    await flushPromises();

    const rowText = wrapper.get('[data-testid="mod-row-inactive.mod"]').text();
    expect(rowText).not.toContain("framework");
    expect(rowText).not.toContain("7");
  });

  it("a failed activation plan opens the dialog with the error and no way to confirm", async () => {
    const wrapper = await mountAndNavigate({
      plan_activate_mods: () => {
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

    expect(wrapper.find('[data-testid="activate-confirm-dialog"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="activate-confirm-error"]').text()).toContain(
      "ghost.mod is not a known mod",
    );
    expect(
      wrapper.get('[data-testid="activate-confirm-submit"]').attributes("disabled"),
    ).toBeDefined();
    expect(activateCalls).toEqual([]);
  });

  it("a failed deactivation plan opens the dialog with the error and no way to confirm", async () => {
    const wrapper = await mountAndNavigate({
      plan_deactivate_mods: () => {
        throw { code: "invalid_input", message: "plan refused" };
      },
    });
    await wrapper
      .get('[data-testid="mod-row-select-active.mod"] input[type="checkbox"]')
      .setValue(true);

    await wrapper.get('[data-testid="deactivate-selected-button"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="deactivate-confirm-dialog"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="deactivate-confirm-error"]').text()).toContain(
      "plan refused",
    );
    expect(
      wrapper.get('[data-testid="deactivate-confirm-submit"]').attributes("disabled"),
    ).toBeDefined();
    expect(deactivateCalls).toEqual([]);
  });

  describe("selecting rows", () => {
    const threeActive = {
      list_working_mods: () =>
        modList([
          modRow({ modId: "a.mod", name: "A" }),
          modRow({ modId: "b.mod", name: "B" }),
          modRow({ modId: "c.mod", name: "C" }),
        ]),
    };

    it("Select all picks every shown row and says how many", async () => {
      const wrapper = await mountAndNavigate(threeActive);

      expect(wrapper.get('[data-testid="mods-select-all-label"]').text()).toBe(
        "Select all 3 shown",
      );
      await wrapper.get('[data-testid="mods-select-all"] input').setValue(true);
      await flushPromises();

      expect(wrapper.get('[data-testid="mods-selected-count"]').text()).toBe("3 selected");
      await wrapper.get('[data-testid="deactivate-selected-button"]').trigger("click");
      await flushPromises();
      await wrapper.get('[data-testid="deactivate-confirm-submit"]').trigger("click");
      await flushPromises();
      expect(deactivateCalls).toEqual([{ ids: ["a.mod", "b.mod", "c.mod"] }]);
    });

    it("keeps the selected count in an always-mounted polite live region", async () => {
      const wrapper = await mountAndNavigate(threeActive);
      const region = wrapper.get('[data-testid="mods-selected-count"]');

      expect(region.attributes("role")).toBe("status");
      expect(region.attributes("aria-live")).toBe("polite");
      expect(region.text()).toBe("");

      await wrapper.get('[data-testid="mods-select-all"] input').setValue(true);
      await flushPromises();

      // Same element, now carrying text: a screen reader announces the change.
      expect(wrapper.get('[data-testid="mods-selected-count"]').element).toBe(region.element);
      expect(region.text()).toBe("3 selected");
    });

    it("names the Select all checkbox by its label and shows a mixed state for a partial selection", async () => {
      const wrapper = await mountAndNavigate(threeActive);
      const checkbox = wrapper.get('[data-testid="mods-select-all"] input')
        .element as HTMLInputElement;

      expect(checkbox.labels?.[0]?.textContent).toContain("Select all 3 shown");
      expect(checkbox.indeterminate).toBe(false);

      await wrapper
        .get('[data-testid="mod-row-select-a.mod"] input[type="checkbox"]')
        .setValue(true);
      await flushPromises();

      expect(checkbox.indeterminate).toBe(true);
      expect(checkbox.checked).toBe(false);
    });

    it("Clear selection removes everything and is disabled when nothing is selected", async () => {
      const wrapper = await mountAndNavigate(threeActive);
      const clear = wrapper.get('[data-testid="mods-clear-selection"]');
      expect(clear.attributes("disabled")).toBeDefined();

      await wrapper.get('[data-testid="mods-select-all"] input').setValue(true);
      await flushPromises();
      expect(clear.attributes("disabled")).toBeUndefined();
      await clear.trigger("click");
      await flushPromises();

      expect(wrapper.get('[data-testid="mods-selected-count"]').text()).toBe("");
      expect(
        wrapper.get('[data-testid="deactivate-selected-button"]').attributes("disabled"),
      ).toBeDefined();
    });

    it("disables the controls on an empty list", async () => {
      const wrapper = await mountAndNavigate({ list_working_mods: () => modList([]) });

      expect(wrapper.get('[data-testid="mods-select-all-label"]').text()).toBe("Select all");
      expect(
        wrapper.get('[data-testid="mods-select-all"] input').attributes("disabled"),
      ).toBeDefined();
    });

    it("keeps a row a later filter hides selected and says it is not shown", async () => {
      vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
      const wrapper = await mountAndNavigate({
        list_working_mods: (payload: unknown) => {
          const { search } = (payload as { query: ModListQueryDto }).query;
          return modList(
            [modRow({ modId: "a.mod", name: "A" }), modRow({ modId: "b.mod", name: "B" })].filter(
              (row) => search === null || row.name === search,
            ),
          );
        },
      });
      await wrapper.get('[data-testid="mods-select-all"] input').setValue(true);
      await flushPromises();

      await wrapper.get('[data-testid="mods-search"]').setValue("A");
      vi.advanceTimersByTime(300);
      await flushPromises();

      expect(wrapper.get('[data-testid="mods-select-all-label"]').text()).toBe(
        "Select all 1 shown",
      );
      expect(wrapper.get('[data-testid="mods-selected-count"]').text()).toContain("2 selected");
      expect(wrapper.get('[data-testid="mods-selected-hidden"]').text()).toBe("(1 not shown)");

      await wrapper.get('[data-testid="mods-clear-selection"]').trigger("click");
      await flushPromises();
      expect(wrapper.get('[data-testid="mods-selected-count"]').text()).toBe("");
    });

    it("drops a selected id that left an unfiltered list", async () => {
      let rows = [modRow({ modId: "a.mod", name: "A" }), modRow({ modId: "b.mod", name: "B" })];
      const wrapper = await mountAndNavigate({
        list_working_mods: () => modList(rows),
        rescan_project: () => ({
          modCount: 1,
          gameVersion: "1.6",
          elapsedMs: 1,
          warnings: [],
          ruleWarnings: [],
          selected: "current",
        }),
      });
      await wrapper.get('[data-testid="mods-select-all"] input').setValue(true);
      await flushPromises();
      expect(wrapper.get('[data-testid="mods-selected-count"]').text()).toBe("2 selected");

      rows = [modRow({ modId: "a.mod", name: "A" })];
      await wrapper.get('[data-testid="mods-rescan-button"]').trigger("click");
      await flushPromises();

      expect(wrapper.get('[data-testid="mods-selected-count"]').text()).toBe("1 selected");
    });
  });

  describe("the Rescan button", () => {
    it("is always visible and rescans the project", async () => {
      let rescans = 0;
      const wrapper = await mountAndNavigate({
        rescan_project: () => {
          rescans += 1;
          return {
            modCount: 1,
            gameVersion: "1.6",
            elapsedMs: 1,
            warnings: [],
            ruleWarnings: [],
            selected: "current",
          };
        },
      });

      await wrapper.get('[data-testid="mods-rescan-button"]').trigger("click");
      await flushPromises();

      expect(rescans).toBe(1);
    });

    it("shows the failure beside the button", async () => {
      const wrapper = await mountAndNavigate({
        rescan_project: () => {
          throw {
            code: "invalid_input",
            message: "the working active-mod set changed during the rescan; rescan again",
          };
        },
      });

      await wrapper.get('[data-testid="mods-rescan-button"]').trigger("click");
      await flushPromises();

      expect(wrapper.get('[data-testid="mods-rescan-error"]').text()).toContain(
        "the working active-mod set changed during the rescan",
      );
    });
  });

  describe("the info panel follows keyboard focus", () => {
    const twoActiveMods = {
      list_working_mods: () =>
        modList([
          modRow({ modId: "active.mod", name: "Active Mod" }),
          modRow({ modId: "second.mod", name: "Second Mod" }),
        ]),
    };

    /** Mounted at `/mods`, with `/elsewhere` one history entry behind it. */
    async function mountAfterElsewhere() {
      const wrapper = mountModsPage(twoActiveMods);
      const router = wrapper.vm.$router;
      await router.push({ name: "elsewhere" });
      await router.isReady();
      await router.push({ name: "mods" });
      await flushPromises();
      return { wrapper, router };
    }

    async function arrowDownFromFirstRow(
      wrapper: Awaited<ReturnType<typeof mountAfterElsewhere>>["wrapper"],
    ) {
      await wrapper
        .get('[data-testid="mod-row-active.mod"]')
        .trigger("keydown", { key: "ArrowDown" });
      await flushPromises();
    }

    it("waits 150ms of settled focus before following it", async () => {
      vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
      const { wrapper, router } = await mountAfterElsewhere();

      await arrowDownFromFirstRow(wrapper);
      vi.advanceTimersByTime(149);
      await flushPromises();

      expect(router.currentRoute.value.fullPath).toBe("/mods");
    });

    it("follows focus by replacing the history entry, not pushing one", async () => {
      vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
      const { wrapper, router } = await mountAfterElsewhere();

      await arrowDownFromFirstRow(wrapper);
      vi.advanceTimersByTime(150);
      await flushPromises();
      expect(router.currentRoute.value.fullPath).toBe("/mods/second.mod");

      router.back();
      await flushPromises();
      expect(router.currentRoute.value.fullPath).toBe("/elsewhere");
    });

    it("does not navigate when the route moved away before the delay ran out", async () => {
      vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
      const { wrapper, router } = await mountAfterElsewhere();

      await arrowDownFromFirstRow(wrapper);
      await router.push({ name: "elsewhere" });
      vi.advanceTimersByTime(150);
      await flushPromises();

      expect(router.currentRoute.value.fullPath).toBe("/elsewhere");
    });
  });
});
