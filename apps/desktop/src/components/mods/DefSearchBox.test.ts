import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import DefSearchBox from "@/components/mods/DefSearchBox.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { DefSearchHitDto } from "@/types/generated/DefSearchHitDto";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  clearMocks();
  vi.useRealTimers();
});

function mountBox(hits: DefSearchHitDto[]) {
  installMockIpc({ search_defs: () => hits });
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/defs/:defRef", name: "def", component: { template: "<div />" } }],
  });
  const wrapper = mount(DefSearchBox, {
    global: { plugins: [createPinia(), PiniaColada, router] },
  });
  return { wrapper, router };
}

/** Advances the search box's own 200ms debounce and lets the resulting query settle. */
async function flush(): Promise<void> {
  await vi.advanceTimersByTimeAsync(200);
  await flushPromises();
}

describe("DefSearchBox", () => {
  it("shows no results before a query is typed", async () => {
    const { wrapper } = mountBox([{ defRef: "ThingDef/Wall", owners: 1 }]);
    await wrapper.get('[data-testid="def-search-input"]').trigger("focus");
    await flush();

    expect(wrapper.find('[data-testid="def-search-results"]').exists()).toBe(false);
  });

  it("lists hits linking to the def page once a query is typed", async () => {
    const { wrapper } = mountBox([{ defRef: "ThingDef/Wall", owners: 2 }]);
    const input = wrapper.get('[data-testid="def-search-input"]');
    await input.trigger("focus");
    await input.setValue("wall");
    await flush();

    const link = wrapper.get('[data-testid="def-search-result-ThingDef/Wall"]');
    expect(link.text()).toContain("ThingDef/Wall");
    expect(link.text()).toContain("2");
    expect(link.attributes("href")).toBe("/defs/ThingDef%2FWall");
  });

  it("exposes combobox/listbox semantics that track the open state and active option", async () => {
    const { wrapper } = mountBox([
      { defRef: "ThingDef/Wall", owners: 1 },
      { defRef: "ThingDef/Door", owners: 1 },
    ]);
    const input = wrapper.get('[data-testid="def-search-input"]');
    expect(input.attributes("role")).toBe("combobox");
    expect(input.attributes("aria-expanded")).toBe("false");

    await input.trigger("focus");
    await input.setValue("d");
    await flush();

    expect(input.attributes("aria-expanded")).toBe("true");
    const listbox = wrapper.get('[data-testid="def-search-results"]');
    expect(listbox.attributes("role")).toBe("listbox");
    expect(listbox.attributes("id")).toBe(input.attributes("aria-controls"));

    const options = wrapper.findAll('[data-testid="def-search-results"] li');
    expect(options).toHaveLength(2);
    expect(options[0]?.attributes("role")).toBe("option");
    expect(options[0]?.attributes("aria-selected")).toBe("true");
    expect(options[0]?.attributes("id")).toBe(input.attributes("aria-activedescendant"));

    await input.trigger("keydown", { key: "ArrowDown" });
    expect(options[0]?.attributes("aria-selected")).toBe("false");
    expect(options[1]?.attributes("aria-selected")).toBe("true");
  });

  it("keyboard selection navigates to the active hit", async () => {
    const { wrapper, router } = mountBox([
      { defRef: "ThingDef/Wall", owners: 1 },
      { defRef: "ThingDef/Door", owners: 1 },
    ]);
    const input = wrapper.get('[data-testid="def-search-input"]');
    await input.trigger("focus");
    await input.setValue("wall");
    await flush();

    // Starts active on the first hit; one ArrowDown moves to the second.
    await input.trigger("keydown", { key: "ArrowDown" });
    await input.trigger("keydown", { key: "Enter" });
    await flushPromises();

    expect(router.currentRoute.value.fullPath).toBe("/defs/ThingDef%2FDoor");
    expect(wrapper.find('[data-testid="def-search-results"]').exists()).toBe(false);
  });

  it("escape closes the list without navigating", async () => {
    const { wrapper, router } = mountBox([{ defRef: "ThingDef/Wall", owners: 1 }]);
    const input = wrapper.get('[data-testid="def-search-input"]');
    await input.trigger("focus");
    await input.setValue("wall");
    await flush();
    expect(wrapper.find('[data-testid="def-search-results"]').exists()).toBe(true);

    await input.trigger("keydown", { key: "Escape" });

    expect(wrapper.find('[data-testid="def-search-results"]').exists()).toBe(false);
    expect(router.currentRoute.value.fullPath).toBe("/");
  });

  it("mouse click navigates to the clicked hit (the mousedown-race regression)", async () => {
    const { wrapper, router } = mountBox([{ defRef: "ThingDef/Wall", owners: 1 }]);
    const input = wrapper.get('[data-testid="def-search-input"]');
    await input.trigger("focus");
    await input.setValue("wall");
    await flush();

    const result = wrapper.get('[data-testid="def-search-result-ThingDef/Wall"]');
    // Real browser order: `mousedown` (which this component prevents the
    // default of, so the input never blurs) fires before `click`.
    await result.trigger("mousedown");
    await result.trigger("click");
    await flushPromises();

    expect(router.currentRoute.value.fullPath).toBe("/defs/ThingDef%2FWall");
  });
});
