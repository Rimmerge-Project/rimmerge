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

import { dashboardFixture } from "@/components/apply/ApplyDialog.test-support";
import { modList, modRow } from "@/components/mods/modListRow.test-support";
import ModsPage from "@/pages/ModsPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ActivateRequestDto } from "@/types/generated/ActivateRequestDto";
import type { DeactivateRequestDto } from "@/types/generated/DeactivateRequestDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import type { MoveRequestDto } from "@/types/generated/MoveRequestDto";

const toastAddSpy = vi.hoisted(() => vi.fn());
vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: toastAddSpy }) }));

const LOAD_ORDER_SORT = { key: "loadOrder", direction: "ascending" } as const;
const NAME_SORT = { key: "name", direction: "ascending" } as const;

const activateCalls: ActivateRequestDto[] = [];
const deactivateCalls: DeactivateRequestDto[] = [];
const planActivateCalls: ActivateRequestDto[] = [];
const listWorkingModsCalls: ModListQueryDto[] = [];
const listInactiveModsCalls: ModListQueryDto[] = [];

function mountModsPage(fixtureOverrides: Record<string, unknown> = {}, attachTo?: HTMLElement) {
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
    get_dashboard: dashboardFixture(0),
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
        pending: {
          unscanned: { added: ["inactive.mod"], removed: [] },
          unapplied: { added: [], removed: [] },
          orderDiffersFromFile: false,
        },
        selected: "current",
        didSwitchToCurrent: false,
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
        orderDiffersFromFile: false,
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
      // Only a test that reads `document.activeElement` needs the page in the document.
      ...(attachTo === undefined ? {} : { attachTo }),
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

async function mountAndNavigate(
  fixtureOverrides: Record<string, unknown> = {},
  attachTo?: HTMLElement,
) {
  const wrapper = mountModsPage(fixtureOverrides, attachTo);
  const router = wrapper.vm.$router;
  await router.push({ name: "mods" });
  await router.isReady();
  await flushPromises();
  return wrapper;
}

/** Resizes the test window; `useMediaQuery` reads it. happy-dom's own API is untyped on `window`. */
function setViewport(width: number, height: number): void {
  const { happyDOM } = window as unknown as {
    happyDOM?: { setViewport: (size: { width: number; height: number }) => void };
  };
  happyDOM?.setViewport({ width, height });
}

/** One of the two side-by-side lists, so a test can reach its own controls. */
function pane(wrapper: Awaited<ReturnType<typeof mountAndNavigate>>, kind: "active" | "inactive") {
  return wrapper.get(`[data-testid="mods-pane-${kind}"]`);
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

    await flushPromises();

    wrapper.get('[data-testid="mod-row-inactive.mod"]');
    expect(wrapper.find('[data-testid="activate-with-dependencies-checkbox"]').exists()).toBe(true);
    const activateButton = wrapper.get('[data-testid="activate-selected-button"]');
    expect(activateButton.attributes("disabled")).toBeDefined();
  });

  it("activating a selected inactive mod shows the plan, then commits with the exact payload", async () => {
    const wrapper = await mountAndNavigate();
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

    expect(activateCalls).toEqual([
      { ids: ["inactive.mod"], withDependencies: true, anchor: null },
    ]);
  });

  it("shows an unresolvable dependency in the activate-plan dialog", async () => {
    const wrapper = await mountAndNavigate({
      plan_activate_mods: {
        toAdd: ["inactive.mod"],
        unresolvableDependencies: { "inactive.mod": ["ghost.mod"] },
        alreadyActive: [],
      },
    });
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

  it("queries both lists unfiltered, with no paging, each in its own default sort", async () => {
    await mountAndNavigate();

    expect(listWorkingModsCalls).toEqual([
      { search: null, tag: null, source: null, sort: LOAD_ORDER_SORT },
    ]);
    expect(listInactiveModsCalls).toEqual([
      { search: null, tag: null, source: null, sort: NAME_SORT },
    ]);
  });

  it("debounces the search term by 300ms before it reaches the Active query", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const wrapper = await mountAndNavigate();

    await pane(wrapper, "active").get('[data-testid="mods-search"]').setValue("Some");
    vi.advanceTimersByTime(299);
    await flushPromises();
    expect(listWorkingModsCalls.map((filter) => filter.search)).toEqual([null]);

    vi.advanceTimersByTime(1);
    await flushPromises();
    expect(listWorkingModsCalls.at(-1)).toEqual({
      search: "Some",
      tag: null,
      source: null,
      sort: LOAD_ORDER_SORT,
    });
  });

  it("sends the tag and source filters to the Active query", async () => {
    const wrapper = await mountAndNavigate();

    await pane(wrapper, "active").get('[data-testid="mods-tag-filter"]').setValue("framework");
    await pane(wrapper, "active").findComponent(Select).vm.$emit("update:modelValue", "workshop");
    await flushPromises();

    expect(listWorkingModsCalls.at(-1)).toEqual({
      search: null,
      tag: "framework",
      source: "workshop",
      sort: LOAD_ORDER_SORT,
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

  it("offers no tag filter on the Inactive list, and a tag typed on the Active list never reaches it", async () => {
    const wrapper = await mountAndNavigate();
    await pane(wrapper, "active").get('[data-testid="mods-tag-filter"]').setValue("framework");
    await pane(wrapper, "inactive").findComponent(Select).vm.$emit("update:modelValue", "local");

    await flushPromises();

    expect(pane(wrapper, "inactive").find('[data-testid="mods-tag-filter"]').exists()).toBe(false);
    expect(listInactiveModsCalls.at(-1)).toEqual({
      search: null,
      tag: null,
      source: "local",
      sort: NAME_SORT,
    });
  });

  it("shows a row's load-order position only when the backend says the Active list is not reorderable", async () => {
    const row = modRow({ modId: "active.mod", position: 4 });
    const reorderableWrapper = await mountAndNavigate({
      list_working_mods: () => modList([row], true),
    });

    expect(reorderableWrapper.find('[data-testid="mod-row-position-active.mod"]').exists()).toBe(
      false,
    );
    reorderableWrapper.unmount();

    const sortedWrapper = await mountAndNavigate({
      list_working_mods: () => modList([row], false),
    });

    expect(sortedWrapper.get('[data-testid="mod-row-position-active.mod"]').text()).toBe("#5");
  });

  it("offers Apply for an order that is not in ModsConfig.xml yet, such as after a move", async () => {
    const unapplied = { ...dashboardFixture(0), fileMatchesCurrent: false };
    const wrapper = await mountAndNavigate({ get_dashboard: unapplied });

    expect(wrapper.get('[data-testid="order-not-in-file"]').text()).toContain(
      "isn't in ModsConfig.xml yet",
    );
    wrapper.get('[data-testid="order-not-in-file-apply"]');
  });

  it("shows no not-in-file note while the file holds the current order", async () => {
    const wrapper = await mountAndNavigate();

    expect(wrapper.find('[data-testid="order-not-in-file"]').exists()).toBe(false);
  });

  it("shows the total and no page-cap hint when every row fits", async () => {
    const wrapper = await mountAndNavigate();

    expect(pane(wrapper, "active").get('[data-testid="mods-total"]').text()).toBe("1 mod");
    expect(pane(wrapper, "active").find('[data-testid="mods-page-cap-hint"]').exists()).toBe(false);
  });

  it("shows the page-cap hint when the backend holds more rows than it returned", async () => {
    const wrapper = await mountAndNavigate({
      list_working_mods: () => ({ total: 250, items: [modRow()] }),
    });

    expect(pane(wrapper, "active").get('[data-testid="mods-total"]').text()).toContain("250 mods");
    expect(pane(wrapper, "active").get('[data-testid="mods-page-cap-hint"]').text()).toBe(
      "(showing first 1 — narrow with search)",
    );
  });

  it("shows the Inactive tab's own total and page-cap hint", async () => {
    const wrapper = await mountAndNavigate({
      list_inactive_mods: () => ({ total: 3, items: [modRow({ modId: "inactive.mod" })] }),
    });

    await flushPromises();

    expect(pane(wrapper, "inactive").get('[data-testid="mods-total"]').text()).toContain("3 mods");
    expect(pane(wrapper, "inactive").find('[data-testid="mods-page-cap-hint"]').exists()).toBe(
      true,
    );
  });

  it("shows the query error message when the Active query fails", async () => {
    const wrapper = await mountAndNavigate({
      list_working_mods: () => {
        throw { code: "internal", message: "boom" };
      },
    });

    expect(pane(wrapper, "active").get('[data-testid="mods-error"]').text()).toBe("boom");
    expect(pane(wrapper, "active").find('[data-testid="mod-table"]').exists()).toBe(false);
  });

  it("keeps each tab's selection across a tab switch", async () => {
    const wrapper = await mountAndNavigate();
    await wrapper
      .get('[data-testid="mod-row-select-active.mod"] input[type="checkbox"]')
      .setValue(true);

    await flushPromises();
    expect(
      wrapper.get('[data-testid="activate-selected-button"]').attributes("disabled"),
    ).toBeDefined();
    await flushPromises();

    expect(
      wrapper.get('[data-testid="deactivate-selected-button"]').attributes("disabled"),
    ).toBeUndefined();
  });

  it("plans the activation with the selected ids and the dependencies choice", async () => {
    const wrapper = await mountAndNavigate();
    await flushPromises();
    await wrapper
      .get('[data-testid="mod-row-select-inactive.mod"] input[type="checkbox"]')
      .setValue(true);

    await wrapper.get('[data-testid="activate-selected-button"]').trigger("click");
    await flushPromises();

    expect(planActivateCalls).toEqual([
      { ids: ["inactive.mod"], withDependencies: false, anchor: null },
    ]);
  });

  it("a committed activation closes the dialog, clears the selection and toasts", async () => {
    const wrapper = await mountAndNavigate();
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

      expect(pane(wrapper, "active").get('[data-testid="mods-select-all-label"]').text()).toBe(
        "Select all 3 shown",
      );
      await pane(wrapper, "active").get('[data-testid="mods-select-all"] input').setValue(true);
      await flushPromises();

      expect(pane(wrapper, "active").get('[data-testid="mods-selected-count"]').text()).toBe(
        "3 selected",
      );
      await wrapper.get('[data-testid="deactivate-selected-button"]').trigger("click");
      await flushPromises();
      await wrapper.get('[data-testid="deactivate-confirm-submit"]').trigger("click");
      await flushPromises();
      expect(deactivateCalls).toEqual([{ ids: ["a.mod", "b.mod", "c.mod"] }]);
    });

    it("keeps the selected count in an always-mounted polite live region", async () => {
      const wrapper = await mountAndNavigate(threeActive);
      const region = pane(wrapper, "active").get('[data-testid="mods-selected-count"]');

      expect(region.attributes("role")).toBe("status");
      expect(region.attributes("aria-live")).toBe("polite");
      expect(region.text()).toBe("");

      await pane(wrapper, "active").get('[data-testid="mods-select-all"] input').setValue(true);
      await flushPromises();

      // Same element, now carrying text: a screen reader announces the change.
      expect(pane(wrapper, "active").get('[data-testid="mods-selected-count"]').element).toBe(
        region.element,
      );
      expect(region.text()).toBe("3 selected");
    });

    it("names the Select all checkbox by its label and shows a mixed state for a partial selection", async () => {
      const wrapper = await mountAndNavigate(threeActive);
      const checkbox = pane(wrapper, "active").get('[data-testid="mods-select-all"] input')
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
      const clear = pane(wrapper, "active").get('[data-testid="mods-clear-selection"]');
      expect(clear.attributes("disabled")).toBeDefined();

      await pane(wrapper, "active").get('[data-testid="mods-select-all"] input').setValue(true);
      await flushPromises();
      expect(clear.attributes("disabled")).toBeUndefined();
      await clear.trigger("click");
      await flushPromises();

      expect(pane(wrapper, "active").get('[data-testid="mods-selected-count"]').text()).toBe("");
      expect(
        wrapper.get('[data-testid="deactivate-selected-button"]').attributes("disabled"),
      ).toBeDefined();
    });

    it("disables the controls on an empty list", async () => {
      const wrapper = await mountAndNavigate({ list_working_mods: () => modList([]) });

      expect(pane(wrapper, "active").get('[data-testid="mods-select-all-label"]').text()).toBe(
        "Select all",
      );
      expect(
        pane(wrapper, "active").get('[data-testid="mods-select-all"] input').attributes("disabled"),
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
      await pane(wrapper, "active").get('[data-testid="mods-select-all"] input').setValue(true);
      await flushPromises();

      await pane(wrapper, "active").get('[data-testid="mods-search"]').setValue("A");
      vi.advanceTimersByTime(300);
      await flushPromises();

      expect(pane(wrapper, "active").get('[data-testid="mods-select-all-label"]').text()).toBe(
        "Select all 1 shown",
      );
      expect(pane(wrapper, "active").get('[data-testid="mods-selected-count"]').text()).toContain(
        "2 selected",
      );
      expect(pane(wrapper, "active").get('[data-testid="mods-selected-hidden"]').text()).toBe(
        "(1 not shown)",
      );

      await pane(wrapper, "active").get('[data-testid="mods-clear-selection"]').trigger("click");
      await flushPromises();
      expect(pane(wrapper, "active").get('[data-testid="mods-selected-count"]').text()).toBe("");
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
      await pane(wrapper, "active").get('[data-testid="mods-select-all"] input').setValue(true);
      await flushPromises();
      expect(pane(wrapper, "active").get('[data-testid="mods-selected-count"]').text()).toBe(
        "2 selected",
      );

      rows = [modRow({ modId: "a.mod", name: "A" })];
      await wrapper.get('[data-testid="mods-rescan-button"]').trigger("click");
      await flushPromises();

      expect(pane(wrapper, "active").get('[data-testid="mods-selected-count"]').text()).toBe(
        "1 selected",
      );
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

    afterEach(() => {
      setViewport(1024, 768);
    });

    /** Mounted at `/mods`, with `/elsewhere` one history entry behind it. */
    async function mountAfterElsewhere(viewportWidth = 1900) {
      setViewport(viewportWidth, 900);
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

    it("does not follow focus below very wide windows, where the panel is a drawer over the lists", async () => {
      vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
      const { wrapper, router } = await mountAfterElsewhere(1400);

      await arrowDownFromFirstRow(wrapper);
      vi.advanceTimersByTime(400);
      await flushPromises();

      expect(router.currentRoute.value.fullPath).toBe("/mods");
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
  describe("the info panel as a drawer", () => {
    afterEach(() => {
      setViewport(1024, 768);
    });

    it("takes focus when a row opens it, and gives it back to that row on Escape", async () => {
      setViewport(1400, 900);
      const wrapper = await mountAndNavigate(
        {
          list_working_mods: () =>
            modList([
              modRow({ modId: "active.mod", name: "Active Mod" }),
              modRow({ modId: "second.mod", name: "Second Mod" }),
            ]),
        },
        document.body,
      );
      const router = wrapper.vm.$router;
      const row = wrapper.get('[data-testid="mod-row-second.mod"]').element;

      await wrapper.get('[data-testid="mod-row-second.mod"]').trigger("keydown", { key: "Enter" });
      await flushPromises();
      expect(router.currentRoute.value.fullPath).toBe("/mods/second.mod");
      expect(document.activeElement).toBe(wrapper.get('[data-testid="mod-info-panel"]').element);

      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
      await flushPromises();

      expect(router.currentRoute.value.fullPath).toBe("/mods");
      expect(wrapper.find('[data-testid="mod-info-panel"]').exists()).toBe(false);
      expect(document.activeElement).toBe(row);
      wrapper.unmount();
    });

    it("returns focus to the Inactive row that opened it, from the close button", async () => {
      setViewport(1400, 900);
      const wrapper = await mountAndNavigate({}, document.body);
      const row = wrapper.get('[data-testid="mod-row-inactive.mod"]').element;

      await wrapper.get('[data-testid="mod-row-inactive.mod"]').trigger("click");
      await flushPromises();
      await wrapper.get('[data-testid="mod-info-panel-close"]').trigger("click");
      await flushPromises();

      expect(document.activeElement).toBe(row);
      wrapper.unmount();
    });

    it("leaves focus on the row when the panel has a column of its own", async () => {
      setViewport(1900, 900);
      const wrapper = await mountAndNavigate({}, document.body);
      const row = wrapper.get('[data-testid="mod-row-inactive.mod"]');
      (row.element as HTMLElement).focus();

      await row.trigger("click");
      await flushPromises();

      expect(document.activeElement).toBe(row.element);
      wrapper.unmount();
    });
  });

  describe("display sort", () => {
    it("sends the chosen sort with the current filters and keeps them together", async () => {
      const wrapper = await mountAndNavigate();
      await pane(wrapper, "active").get('[data-testid="mods-tag-filter"]').setValue("framework");

      await pane(wrapper, "active")
        .get('[data-testid="mods-sort-lastUpdated-descending"]')
        .trigger("click");
      await flushPromises();

      expect(listWorkingModsCalls.at(-1)).toEqual({
        search: null,
        tag: "framework",
        source: null,
        sort: { key: "lastUpdated", direction: "descending" },
      });
    });

    it("keeps one sort per list and offers no load order on the Inactive list", async () => {
      const wrapper = await mountAndNavigate();
      await pane(wrapper, "active")
        .get('[data-testid="mods-sort-packageId-ascending"]')
        .trigger("click");

      await flushPromises();

      expect(
        pane(wrapper, "inactive").find('[data-testid="mods-sort-loadOrder-ascending"]').exists(),
      ).toBe(false);
      expect(
        pane(wrapper, "inactive")
          .get('[data-testid="mods-sort-name-ascending"]')
          .attributes("aria-pressed"),
      ).toBe("true");
      expect(listInactiveModsCalls.at(-1)?.sort).toEqual(NAME_SORT);

      await flushPromises();
      expect(
        pane(wrapper, "active")
          .get('[data-testid="mods-sort-packageId-ascending"]')
          .attributes("aria-pressed"),
      ).toBe("true");
    });

    it("sorting writes nothing: no activate, deactivate or plan call is made", async () => {
      const wrapper = await mountAndNavigate();

      for (const testId of [
        "mods-sort-name-descending",
        "mods-sort-lastUpdated-ascending",
        "mods-sort-loadOrder-ascending",
      ]) {
        await wrapper.get(`[data-testid="${testId}"]`).trigger("click");
        await flushPromises();
      }

      expect(activateCalls).toEqual([]);
      expect(deactivateCalls).toEqual([]);
      expect(planActivateCalls).toEqual([]);
    });

    it("shows each row's update time only while sorted by last updated", async () => {
      const wrapper = await mountAndNavigate({
        list_working_mods: () =>
          modList([
            modRow({
              modId: "active.mod",
              updated: { kind: "known", seconds: 1_700_000_000 },
            }),
            modRow({ modId: "core.mod", updated: { kind: "shippedWithGame" } }),
          ]),
      });
      expect(wrapper.find('[data-testid="mod-row-updated-active.mod"]').exists()).toBe(false);

      await pane(wrapper, "active")
        .get('[data-testid="mods-sort-lastUpdated-ascending"]')
        .trigger("click");
      await flushPromises();

      expect(wrapper.get('[data-testid="mod-row-updated-active.mod"]').text()).toMatch(/2023/);
      expect(wrapper.get('[data-testid="mod-row-updated-core.mod"]').text()).toBe(
        "Ships with the game",
      );
    });
  });

  describe("moving mods by position", () => {
    const moveCalls: MoveRequestDto[] = [];
    const planDeactivateCalls: DeactivateRequestDto[] = [];
    const selectOrderCalls: string[] = [];
    let activeRows: ModListRowDto[] = [];

    function threeActive(): ModListRowDto[] {
      return ["a", "b", "c"].map((modId, position) => modRow({ modId, name: modId, position }));
    }

    /** Two lists: three active mods and two inactive ones, with every command a move can reach. */
    async function mountMoves(overrides: Record<string, unknown> = {}) {
      moveCalls.length = 0;
      planDeactivateCalls.length = 0;
      selectOrderCalls.length = 0;
      activeRows = threeActive();
      return mountAndNavigate({
        list_working_mods: (payload: unknown) => {
          listWorkingModsCalls.push((payload as { query: ModListQueryDto }).query);
          return modList(activeRows);
        },
        list_inactive_mods: () =>
          modList([modRow({ modId: "x", name: "x" }), modRow({ modId: "y", name: "y" })]),
        plan_activate_mods: (payload: unknown) => {
          planActivateCalls.push((payload as { request: ActivateRequestDto }).request);
          const { ids } = (payload as { request: ActivateRequestDto }).request;
          return { toAdd: ids, unresolvableDependencies: {}, alreadyActive: [] };
        },
        plan_deactivate_mods: (payload: unknown) => {
          const { ids } = (payload as { request: DeactivateRequestDto }).request;
          planDeactivateCalls.push({ ids });
          return { toRemove: ids, dependentsStillActive: {}, refused: [] };
        },
        move_mods: (payload: unknown) => {
          moveCalls.push((payload as { request: MoveRequestDto }).request);
          return { selected: "current", didSwitchToCurrent: false };
        },
        select_order: (payload: unknown) => {
          selectOrderCalls.push((payload as { source: string }).source);
          return {};
        },
        ...overrides,
      });
    }

    /** A cancelable drag event at `clientY` (40px rows: y = 40 * slot). */
    function dragEvent(element: Element, type: string, clientY: number): Event {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "clientY", { value: clientY });
      element.dispatchEvent(event);
      return event;
    }

    /** Picks up `modId` in one list and drops it at `clientY` over `target`'s table. */
    async function drag(
      wrapper: Awaited<ReturnType<typeof mountMoves>>,
      modId: string,
      target: "active" | "inactive",
      clientY: number,
    ): Promise<void> {
      await wrapper.get(`[data-testid="mod-row-${modId}"]`).trigger("dragstart");
      const table = pane(wrapper, target).get('[data-testid="mod-table"]').element;
      dragEvent(table, "dragover", clientY);
      await flushPromises();
      dragEvent(table, "drop", clientY);
      await flushPromises();
    }

    async function select(wrapper: Awaited<ReturnType<typeof mountMoves>>, modId: string) {
      await wrapper
        .get(`[data-testid="mod-row-select-${modId}"] input[type="checkbox"]`)
        .setValue(true);
    }

    function status(wrapper: Awaited<ReturnType<typeof mountMoves>>): string {
      return wrapper.get('[data-testid="mods-move-status"]').text();
    }

    it("moves a dropped active row next to the neighbour below the gap, by id", async () => {
      const wrapper = await mountMoves();

      await drag(wrapper, "a", "active", 80);

      expect(moveCalls).toEqual([{ ids: ["a"], anchor: { kind: "before", modId: "c" } }]);
    });

    it("sends nothing for a drop beside the row's own place", async () => {
      const wrapper = await mountMoves();

      await drag(wrapper, "b", "active", 40);

      expect(moveCalls).toEqual([]);
    });

    it("anchors to a shown neighbour when a filter hides rows in between", async () => {
      const wrapper = await mountMoves();
      // The view shows a and c only; b is hidden between them.
      activeRows = [activeRows[0] as ModListRowDto, activeRows[2] as ModListRowDto];
      await pane(wrapper, "active").findComponent(Select).vm.$emit("update:modelValue", "local");
      await flushPromises();

      await drag(wrapper, "a", "active", 999);

      expect(moveCalls).toEqual([{ ids: ["a"], anchor: { kind: "after", modId: "c" } }]);
    });

    it("moves the whole selection together when the dragged row is selected", async () => {
      const wrapper = await mountMoves();
      await select(wrapper, "b");
      await select(wrapper, "c");

      await drag(wrapper, "c", "active", 0);

      expect(moveCalls).toEqual([{ ids: ["b", "c"], anchor: { kind: "before", modId: "a" } }]);
    });

    it("moves only the dragged row when it is not part of the selection", async () => {
      const wrapper = await mountMoves();
      await select(wrapper, "b");

      await drag(wrapper, "a", "active", 999);

      expect(moveCalls).toEqual([{ ids: ["a"], anchor: { kind: "after", modId: "c" } }]);
    });

    it("commits a drop from Inactive straight away when the plan adds exactly the dragged mod", async () => {
      const wrapper = await mountMoves();

      await drag(wrapper, "x", "active", 40);

      expect(planActivateCalls).toEqual([
        { ids: ["x"], withDependencies: false, anchor: { kind: "before", modId: "b" } },
      ]);
      expect(activateCalls).toEqual(planActivateCalls);
      expect(wrapper.find('[data-testid="activate-confirm-dialog"]').exists()).toBe(false);
    });

    it("opens the Activate dialog for a drop whose plan adds more, and commits the same anchor on confirm", async () => {
      const wrapper = await mountMoves({
        plan_activate_mods: (payload: unknown) => {
          planActivateCalls.push((payload as { request: ActivateRequestDto }).request);
          return { toAdd: ["dep", "x"], unresolvableDependencies: {}, alreadyActive: [] };
        },
      });

      await drag(wrapper, "x", "active", 40);

      expect(activateCalls).toEqual([]);
      expect(wrapper.get('[data-testid="activate-confirm-dialog"]').text()).toContain("dep");

      await wrapper.get('[data-testid="activate-confirm-submit"]').trigger("click");
      await flushPromises();
      expect(activateCalls).toEqual([
        { ids: ["x"], withDependencies: false, anchor: { kind: "before", modId: "b" } },
      ]);
    });

    it("opens the dialog for a drop whose plan warns, even when it adds only the dragged mod", async () => {
      const wrapper = await mountMoves({
        plan_activate_mods: () => ({
          toAdd: ["x"],
          unresolvableDependencies: { x: ["ghost"] },
          alreadyActive: [],
        }),
      });

      await drag(wrapper, "x", "active", 40);

      expect(activateCalls).toEqual([]);
      wrapper.get('[data-testid="activate-confirm-dialog"]');
    });

    it("deactivates a row dropped on the Inactive list straight away when nothing depends on it", async () => {
      const wrapper = await mountMoves();

      await drag(wrapper, "a", "inactive", 10);

      expect(planDeactivateCalls).toEqual([{ ids: ["a"] }]);
      expect(deactivateCalls).toEqual([{ ids: ["a"] }]);
    });

    it("opens the Deactivate dialog instead when a mod still depends on the dropped one", async () => {
      const wrapper = await mountMoves({
        plan_deactivate_mods: () => ({
          toRemove: ["a"],
          dependentsStillActive: { a: ["b"] },
          refused: [],
        }),
      });

      await drag(wrapper, "a", "inactive", 10);

      expect(deactivateCalls).toEqual([]);
      wrapper.get('[data-testid="deactivate-confirm-dialog"]');
    });

    it("refuses to drop Core on the Inactive list: not-allowed cursor, the reason, no plan", async () => {
      const wrapper = await mountMoves({
        list_working_mods: () =>
          modList([modRow({ modId: "core", name: "core", source: "core", position: 0 })]),
      });
      await wrapper.get('[data-testid="mod-row-core"]').trigger("dragstart");
      await flushPromises();
      const table = pane(wrapper, "inactive").get('[data-testid="mod-table"]').element;

      const over = dragEvent(table, "dragover", 10);
      await flushPromises();
      dragEvent(table, "drop", 10);
      await flushPromises();

      expect(over.defaultPrevented).toBe(false);
      expect(pane(wrapper, "inactive").get('[data-testid="mod-table-refused-text"]').text()).toBe(
        "Core can't be deactivated.",
      );
      expect(planDeactivateCalls).toEqual([]);
    });

    it("takes no drop from the Inactive list onto itself", async () => {
      const wrapper = await mountMoves();

      await drag(wrapper, "x", "inactive", 10);

      expect(planActivateCalls).toEqual([]);
      expect(moveCalls).toEqual([]);
    });

    describe("while the Active list is not in load order", () => {
      const sorted = () => ({
        list_working_mods: () => modList(threeActive(), false),
      });

      it("refuses a reorder with the reason visible, and sends nothing", async () => {
        const wrapper = await mountMoves(sorted());
        await wrapper.get('[data-testid="mod-row-a"]').trigger("dragstart");
        const table = pane(wrapper, "active").get('[data-testid="mod-table"]').element;

        const over = dragEvent(table, "dragover", 80);
        await flushPromises();
        dragEvent(table, "drop", 80);
        await flushPromises();

        expect(over.defaultPrevented).toBe(false);
        expect(moveCalls).toEqual([]);
        expect(wrapper.get('[data-testid="mods-reorder-blocked"]').text()).toContain(
          "isn't sorted by load order",
        );
      });

      it("still activates a mod dropped from Inactive, at the default place", async () => {
        const wrapper = await mountMoves(sorted());

        await drag(wrapper, "x", "active", 80);

        expect(planActivateCalls).toEqual([{ ids: ["x"], withDependencies: false, anchor: null }]);
      });

      it("answers Alt+Arrow with the reason instead of moving", async () => {
        const wrapper = await mountMoves(sorted());

        await wrapper
          .get('[data-testid="mod-row-b"]')
          .trigger("keydown", { key: "ArrowUp", altKey: true });
        await flushPromises();

        expect(moveCalls).toEqual([]);
        expect(status(wrapper)).toContain("isn't sorted by load order");
      });
    });

    describe("from the keyboard", () => {
      it("Alt+ArrowUp moves the focused row before the row above it", async () => {
        const wrapper = await mountMoves();

        await wrapper
          .get('[data-testid="mod-row-b"]')
          .trigger("keydown", { key: "ArrowUp", altKey: true });
        await flushPromises();

        expect(moveCalls).toEqual([{ ids: ["b"], anchor: { kind: "before", modId: "a" } }]);
      });

      it("Alt+ArrowDown moves it after the row below, and says so at the end of the list", async () => {
        const wrapper = await mountMoves();

        await wrapper
          .get('[data-testid="mod-row-b"]')
          .trigger("keydown", { key: "ArrowDown", altKey: true });
        await flushPromises();
        await wrapper
          .get('[data-testid="mod-row-c"]')
          .trigger("keydown", { key: "ArrowDown", altKey: true });
        await flushPromises();

        expect(moveCalls).toEqual([{ ids: ["b"], anchor: { kind: "after", modId: "c" } }]);
        expect(status(wrapper)).toBe("Already at the bottom of this list");
      });

      it("announces each move through the one live region, with the new position and the total", async () => {
        const wrapper = await mountMoves({
          move_mods: () => {
            activeRows = ["b", "a", "c"].map((modId, position) =>
              modRow({ modId, name: modId, position }),
            );
            return { selected: "current", didSwitchToCurrent: false };
          },
        });

        await wrapper
          .get('[data-testid="mod-row-b"]')
          .trigger("keydown", { key: "ArrowUp", altKey: true });
        await flushPromises();

        expect(status(wrapper)).toBe("Moved b to position 1 of 3");
        expect(wrapper.find('[role="status"][data-testid="mods-move-status"]').exists()).toBe(true);
      });

      it("leaves the total out under a filter, where the list is only part of the order", async () => {
        const wrapper = await mountMoves({
          move_mods: () => {
            activeRows = ["b", "a", "c"].map((modId, position) =>
              modRow({ modId, name: modId, position }),
            );
            return { selected: "current", didSwitchToCurrent: false };
          },
        });
        await pane(wrapper, "active").findComponent(Select).vm.$emit("update:modelValue", "local");
        await flushPromises();

        await wrapper
          .get('[data-testid="mod-row-b"]')
          .trigger("keydown", { key: "ArrowUp", altKey: true });
        await flushPromises();

        expect(status(wrapper)).toBe("Moved b to position 1");
      });

      it("announces an identical edge message again by clearing the live region between the two", async () => {
        const wrapper = await mountMoves();
        const region = wrapper.get('[data-testid="mods-move-status"]').element;
        const seen: string[] = [];
        new MutationObserver(() => seen.push(region.textContent ?? "")).observe(region, {
          childList: true,
          characterData: true,
          subtree: true,
        });

        for (const _press of [1, 2]) {
          await wrapper
            .get('[data-testid="mod-row-a"]')
            .trigger("keydown", { key: "ArrowUp", altKey: true });
          await flushPromises();
        }

        expect(seen).toEqual([
          "Already at the top of this list",
          "",
          "Already at the top of this list",
        ]);
      });

      it("says how many carried mods a filter hides, without counting them in the position", async () => {
        vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
        const order = (ids: string[]) =>
          ids.map((modId, position) => modRow({ modId, name: modId, position }));
        let rows = order(["a", "b", "c", "d"]);
        const wrapper = await mountMoves({
          list_working_mods: (payload: unknown) => {
            const { search } = (payload as { query: ModListQueryDto }).query;
            return modList(rows.filter((row) => search === null || ["a", "c"].includes(row.modId)));
          },
          move_mods: () => {
            rows = order(["b", "c", "a", "d"]);
            return { selected: "current", didSwitchToCurrent: false };
          },
        });
        await select(wrapper, "b");
        await select(wrapper, "c");
        await pane(wrapper, "active").get('[data-testid="mods-search"]').setValue("ac");
        vi.advanceTimersByTime(300);
        await flushPromises();

        await wrapper
          .get('[data-testid="mod-row-c"]')
          .trigger("keydown", { key: "ArrowUp", altKey: true });
        await flushPromises();

        expect(status(wrapper)).toBe("Moved 2 mods (1 not shown), starting at position 2");
      });

      it("announces a block move by count", async () => {
        const wrapper = await mountMoves({
          move_mods: (payload: unknown) => {
            moveCalls.push((payload as { request: MoveRequestDto }).request);
            activeRows = ["c", "a", "b"].map((modId, position) =>
              modRow({ modId, name: modId, position }),
            );
            return { selected: "current", didSwitchToCurrent: false };
          },
        });
        await select(wrapper, "a");
        await select(wrapper, "b");

        await wrapper
          .get('[data-testid="mod-row-a"]')
          .trigger("keydown", { key: "ArrowDown", altKey: true });
        await flushPromises();

        expect(moveCalls).toEqual([{ ids: ["a", "b"], anchor: { kind: "after", modId: "c" } }]);
        expect(status(wrapper)).toBe("Moved 2 mods, starting at position 2 of 3");
      });
    });

    describe("from the row menu", () => {
      async function openMenu(wrapper: Awaited<ReturnType<typeof mountMoves>>, modId: string) {
        await wrapper.get(`[data-testid="mod-row-menu-${modId}"]`).trigger("click");
        await flushPromises();
      }

      it("Move to top and Move to bottom anchor at the ends of the whole list", async () => {
        const wrapper = await mountMoves();

        await openMenu(wrapper, "b");
        await wrapper.get('[data-testid="mod-move-top"]').trigger("click");
        await flushPromises();
        await openMenu(wrapper, "b");
        await wrapper.get('[data-testid="mod-move-bottom"]').trigger("click");
        await flushPromises();

        expect(moveCalls).toEqual([
          { ids: ["b"], anchor: { kind: "start" } },
          { ids: ["b"], anchor: { kind: "end" } },
        ]);
      });

      it("Move up and Move down step one shown row", async () => {
        const wrapper = await mountMoves();

        await openMenu(wrapper, "b");
        await wrapper.get('[data-testid="mod-move-up"]').trigger("click");
        await flushPromises();
        await openMenu(wrapper, "b");
        await wrapper.get('[data-testid="mod-move-down"]').trigger("click");
        await flushPromises();

        expect(moveCalls).toEqual([
          { ids: ["b"], anchor: { kind: "before", modId: "a" } },
          { ids: ["b"], anchor: { kind: "after", modId: "c" } },
        ]);
      });

      it("Move after... lists the other active mods and places the row after the one picked", async () => {
        const wrapper = await mountMoves();

        await openMenu(wrapper, "a");
        await wrapper.get('[data-testid="mod-move-after"]').trigger("click");
        await flushPromises();

        const offered = wrapper
          .findAll('[data-testid^="mod-position-after-"]')
          .map((button) => button.attributes("data-testid"));
        expect(offered).toEqual(["mod-position-after-b", "mod-position-after-c"]);

        await wrapper.get('[data-testid="mod-position-after-c"]').trigger("click");
        await flushPromises();
        expect(moveCalls).toEqual([{ ids: ["a"], anchor: { kind: "after", modId: "c" } }]);
      });

      it("is disabled, with the reason, while the list is not in load order", async () => {
        const wrapper = await mountMoves({
          list_working_mods: () => modList(threeActive(), false),
        });

        await openMenu(wrapper, "b");

        expect(wrapper.get('[data-testid="mod-move-menu-reason"]').text()).toContain(
          "Sort by load order",
        );
        const top = wrapper.get('[data-testid="mod-move-top"]').element.closest("li");
        expect(top?.getAttribute("aria-disabled")).toBe("true");
      });

      it("Activate at... on an inactive row offers the ends and every active mod", async () => {
        const wrapper = await mountMoves();

        await wrapper.get('[data-testid="mod-row-menu-x"]').trigger("click");
        await flushPromises();
        await wrapper.get('[data-testid="mod-position-start"]').trigger("click");
        await flushPromises();

        expect(planActivateCalls).toEqual([
          { ids: ["x"], withDependencies: false, anchor: { kind: "start" } },
        ]);
        expect(activateCalls).toEqual(planActivateCalls);
      });
    });

    describe("the switch to your own order", () => {
      const switched = {
        move_mods: (payload: unknown) => {
          moveCalls.push((payload as { request: MoveRequestDto }).request);
          return { selected: "current", didSwitchToCurrent: true };
        },
      };

      it("says so after the first reorder, and View suggested selects the suggested order", async () => {
        const wrapper = await mountMoves(switched);
        expect(wrapper.find('[data-testid="order-switched-note"]').exists()).toBe(false);

        await wrapper
          .get('[data-testid="mod-row-b"]')
          .trigger("keydown", { key: "ArrowUp", altKey: true });
        await flushPromises();

        expect(wrapper.get('[data-testid="order-switched-note"]').text()).toContain(
          "Now using your own order",
        );

        await wrapper.get('[data-testid="order-switched-view-suggested"]').trigger("click");
        await flushPromises();
        expect(selectOrderCalls).toEqual(["suggested"]);
        expect(wrapper.find('[data-testid="order-switched-note"]').exists()).toBe(false);
      });

      it("stays quiet when the move did not switch anything", async () => {
        const wrapper = await mountMoves();

        await wrapper
          .get('[data-testid="mod-row-b"]')
          .trigger("keydown", { key: "ArrowUp", altKey: true });
        await flushPromises();

        expect(wrapper.find('[data-testid="order-switched-note"]').exists()).toBe(false);
      });

      it("also says so after an anchored drop from Inactive that switched the selection", async () => {
        const wrapper = await mountMoves({
          activate_mods: (payload: unknown) => {
            activateCalls.push((payload as { request: ActivateRequestDto }).request);
            return {
              pending: {
                unscanned: { added: ["x"], removed: [] },
                unapplied: { added: [], removed: [] },
                orderDiffersFromFile: false,
              },
              selected: "current",
              didSwitchToCurrent: true,
            };
          },
        });

        await drag(wrapper, "x", "active", 40);

        wrapper.get('[data-testid="order-switched-note"]');
      });
    });

    it("tells the user and refreshes the lists when the anchor went stale", async () => {
      const wrapper = await mountMoves({
        move_mods: () => {
          throw { code: "move_anchor_not_active", message: "a is no longer active" };
        },
      });
      const before = listWorkingModsCalls.length;

      await wrapper
        .get('[data-testid="mod-row-b"]')
        .trigger("keydown", { key: "ArrowUp", altKey: true });
      await flushPromises();

      expect(toastAddSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          severity: "error",
          summary: "That mod is no longer in the active list",
        }),
      );
      expect(listWorkingModsCalls.length).toBeGreaterThan(before);
    });
  });

  describe("the mod info panel on side-by-side widths", () => {
    afterEach(() => {
      setViewport(1024, 768);
    });

    async function openPanelAt(width: number) {
      setViewport(width, 900);
      const wrapper = mountModsPage();
      await wrapper.vm.$router.push({ name: "mod-detail", params: { modId: "active.mod" } });
      await flushPromises();
      return wrapper;
    }

    it("floats over the lists as a drawer below very wide windows", async () => {
      const wrapper = await openPanelAt(1400);

      expect(wrapper.get('[data-testid="mod-info-panel"]').classes()).toContain("fixed");
    });

    it("takes its own column on a very wide window", async () => {
      const wrapper = await openPanelAt(1900);

      const classes = wrapper.get('[data-testid="mod-info-panel"]').classes();
      expect(classes).not.toContain("fixed");
      expect(classes).toContain("shrink-0");
    });

    it("closes the drawer on Escape from anywhere on the page", async () => {
      const wrapper = await openPanelAt(1400);

      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
      await flushPromises();

      expect(wrapper.vm.$router.currentRoute.value.name).toBe("mods");
    });
  });
});
