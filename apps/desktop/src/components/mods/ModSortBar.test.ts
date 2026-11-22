import Aura from "@primevue/themes/aura";
import { mount } from "@vue/test-utils";
import PrimeVue from "primevue/config";
import Select from "primevue/select";
import { afterEach, describe, expect, it, vi } from "vitest";
import { nextTick } from "vue";

import ModSortBar from "@/components/mods/ModSortBar.vue";
import type { ListSortDto } from "@/types/generated/ListSortDto";
import type { ListSortKeyDto } from "@/types/generated/ListSortKeyDto";
import { ACTIVE_SORT_KEYS, INACTIVE_SORT_KEYS } from "@/utils/modListSort";

const NAME_ASC: ListSortDto = { key: "name", direction: "ascending" };

function mountBar(keys: readonly ListSortKeyDto[] = ACTIVE_SORT_KEYS, sort = NAME_ASC) {
  return mount(ModSortBar, { props: { keys, sort } });
}

function button(wrapper: ReturnType<typeof mountBar>, key: ListSortKeyDto) {
  return wrapper.get(`[data-testid="mods-sort-${key}"]`);
}

const IDLE_NAMES: readonly { key: ListSortKeyDto; label: string }[] = [
  { key: "loadOrder", label: "Sort by load order, first to last" },
  { key: "name", label: "Sort by name, A to Z" },
  { key: "packageId", label: "Sort by package ID, A to Z" },
  { key: "lastUpdated", label: "Sort by last updated, oldest first" },
];

const ACTIVE_NAMES: readonly { key: ListSortKeyDto; asc: string; desc: string }[] = [
  {
    key: "loadOrder",
    asc: "Sorted by load order, first to last. Click for last to first.",
    desc: "Sorted by load order, last to first. Click for first to last.",
  },
  {
    key: "name",
    asc: "Sorted by name, A to Z. Click for Z to A.",
    desc: "Sorted by name, Z to A. Click for A to Z.",
  },
  {
    key: "packageId",
    asc: "Sorted by package ID, A to Z. Click for Z to A.",
    desc: "Sorted by package ID, Z to A. Click for A to Z.",
  },
  {
    key: "lastUpdated",
    asc: "Sorted by last updated, oldest first. Click for newest first.",
    desc: "Sorted by last updated, newest first. Click for oldest first.",
  },
];

describe("ModSortBar", () => {
  it.each(IDLE_NAMES)("names what a click selects on the idle $key button", ({ key, label }) => {
    const otherKey: ListSortKeyDto = key === "name" ? "packageId" : "name";
    const wrapper = mountBar(ACTIVE_SORT_KEYS, { key: otherKey, direction: "descending" });

    expect(button(wrapper, key).attributes("aria-label")).toBe(label);
    expect(button(wrapper, key).attributes("title")).toBe(label);
  });

  it.each(ACTIVE_NAMES)(
    "names the direction and the next click on the active $key button",
    ({ key, asc, desc }) => {
      const ascending = mountBar(ACTIVE_SORT_KEYS, { key, direction: "ascending" });
      const descending = mountBar(ACTIVE_SORT_KEYS, { key, direction: "descending" });

      expect(button(ascending, key).attributes("aria-label")).toBe(asc);
      expect(button(ascending, key).attributes("title")).toBe(asc);
      expect(button(descending, key).attributes("aria-label")).toBe(desc);
      expect(button(descending, key).attributes("title")).toBe(desc);
    },
  );

  it("names the active criterion in exactly one button and exposes no pressed state", () => {
    const wrapper = mountBar(ACTIVE_SORT_KEYS, { key: "packageId", direction: "descending" });

    const sortedBy = wrapper
      .findAll("button")
      .filter((candidate) => candidate.attributes("aria-label")?.startsWith("Sorted by"))
      .map((candidate) => candidate.attributes("data-testid"));

    expect(sortedBy).toEqual(["mods-sort-packageId"]);
    for (const candidate of wrapper.findAll("button")) {
      expect(candidate.attributes("aria-pressed")).toBeUndefined();
    }
  });

  it("gives the active button a thicker border and a direction arrow, not just a colour", () => {
    const ascending = mountBar(ACTIVE_SORT_KEYS, { key: "name", direction: "ascending" });
    const descending = mountBar(ACTIVE_SORT_KEYS, { key: "name", direction: "descending" });

    expect(button(ascending, "name").classes()).toContain("border-2");
    expect(button(ascending, "packageId").classes()).not.toContain("border-2");
    expect(button(ascending, "name").find("i").classes()).toContain("pi-arrow-up");
    expect(button(descending, "name").find("i").classes()).toContain("pi-arrow-down");
    expect(button(ascending, "packageId").find("i").classes()).toContain("pi-sort-alt");
  });

  it("offers one button per criterion and omits load order for Inactive", () => {
    expect(mountBar(ACTIVE_SORT_KEYS).findAll("button")).toHaveLength(4);

    const inactive = mountBar(INACTIVE_SORT_KEYS);

    expect(inactive.findAll("button")).toHaveLength(3);
    expect(inactive.find('[data-testid="mods-sort-loadOrder"]').exists()).toBe(false);
  });

  it.each(["ascending", "descending"] as const)(
    "selects another criterion ascending when it is clicked from %s",
    async (from) => {
      const wrapper = mountBar(ACTIVE_SORT_KEYS, { key: "name", direction: from });

      await button(wrapper, "lastUpdated").trigger("click");

      expect(wrapper.emitted("update:sort")).toEqual([
        [{ key: "lastUpdated", direction: "ascending" }],
      ]);
    },
  );

  it.each([
    ["ascending", "descending"],
    ["descending", "ascending"],
  ] as const)("flips the active criterion from %s to %s on click", async (from, to) => {
    const wrapper = mountBar(ACTIVE_SORT_KEYS, { key: "name", direction: from });

    await button(wrapper, "name").trigger("click");

    expect(wrapper.emitted("update:sort")).toEqual([[{ key: "name", direction: to }]]);
  });

  it("is keyboard operable: every button is a native button in the tab order", () => {
    const wrapper = mountBar();

    for (const candidate of wrapper.findAll("button")) {
      expect(candidate.element.tagName).toBe("BUTTON");
      expect(candidate.attributes("type")).toBe("button");
      expect(candidate.attributes("tabindex")).toBeUndefined();
      expect(candidate.attributes("disabled")).toBeUndefined();
    }
  });

  it("is a labelled group", () => {
    const wrapper = mountBar();

    expect(wrapper.get('[role="group"]').attributes("aria-label")).toBe("Sort order");
  });
});

/** Mounts the bar in a pane `paneWidth` wide whose buttons need `naturalWidth` (the size is measured, not styled). */
async function mountMeasured(paneWidth: number, naturalWidth: number, sort = NAME_ASC) {
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(paneWidth);
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(naturalWidth);
  const wrapper = mount(ModSortBar, {
    props: { keys: ACTIVE_SORT_KEYS, sort },
    global: { plugins: [[PrimeVue, { theme: { preset: Aura } }]] },
  });
  // The measurement runs on mount and flips the form on the next render.
  await nextTick();
  return wrapper;
}

describe("ModSortBar when the buttons do not fit", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("keeps the buttons while they fit the pane", async () => {
    const wrapper = await mountMeasured(600, 480);

    expect(wrapper.find('[data-testid="mods-sort-select"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="mods-sort-direction"]').exists()).toBe(false);
  });

  it("collapses to a select and a direction button when they are wider than the pane", async () => {
    const wrapper = await mountMeasured(376, 528);

    expect(wrapper.find('[data-testid="mods-sort-select"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="mods-sort-direction"]').exists()).toBe(true);
  });

  it("keeps the measured buttons out of reach while collapsed", async () => {
    const wrapper = await mountMeasured(376, 528);

    const measured = wrapper.get('[data-testid="mods-sort-name"]').element.closest("[inert]");
    expect(measured).not.toBeNull();
    expect(measured?.className).toContain("invisible");
  });

  it("flips the direction of the current criterion from the direction button", async () => {
    const wrapper = await mountMeasured(376, 528, { key: "name", direction: "ascending" });

    await wrapper.get('[data-testid="mods-sort-direction"]').trigger("click");

    expect(wrapper.emitted("update:sort")).toEqual([[{ key: "name", direction: "descending" }]]);
  });

  it("names the direction button like the active criterion button", async () => {
    const wrapper = await mountMeasured(376, 528, { key: "name", direction: "ascending" });

    expect(wrapper.get('[data-testid="mods-sort-direction"]').attributes("aria-label")).toBe(
      "Sorted by name, A to Z. Click for Z to A.",
    );
  });

  it("selects another criterion ascending", async () => {
    const wrapper = await mountMeasured(376, 528, { key: "name", direction: "descending" });

    wrapper.getComponent(Select).vm.$emit("update:modelValue", "lastUpdated");

    expect(wrapper.emitted("update:sort")).toEqual([
      [{ key: "lastUpdated", direction: "ascending" }],
    ]);
  });

  it("does not flip the direction when the current criterion is picked again", async () => {
    const wrapper = await mountMeasured(376, 528, { key: "name", direction: "ascending" });

    wrapper.getComponent(Select).vm.$emit("update:modelValue", "name");

    expect(wrapper.emitted("update:sort")).toBeUndefined();
  });

  /** Mounts the bar attached to the document at a 600 px pane; `resize` fires the observers at a new width. */
  async function mountResizable() {
    const callbacks: Array<() => void> = [];
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(callback: () => void) {
          callbacks.push(callback);
        }
        observe(): void {}
        unobserve(): void {}
        disconnect(): void {}
      },
    );
    const widths = { pane: 600 };
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(() => widths.pane);
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(528);
    const wrapper = mount(ModSortBar, {
      props: { keys: ACTIVE_SORT_KEYS, sort: NAME_ASC },
      global: { plugins: [[PrimeVue, { theme: { preset: Aura } }]] },
      attachTo: document.body,
    });
    await nextTick();
    const resize = async (pane: number) => {
      widths.pane = pane;
      callbacks.forEach((callback) => {
        callback();
      });
      await nextTick();
      await nextTick();
    };
    return { wrapper, resize };
  }

  const focused = () => document.activeElement?.getAttribute("data-testid");

  it("moves focus to the counterpart control when the form swaps under it", async () => {
    const { wrapper, resize } = await mountResizable();
    (wrapper.get('[data-testid="mods-sort-name"]').element as HTMLElement).focus();

    await resize(376);
    expect(focused()).toBe("mods-sort-direction");

    await resize(600);
    expect(focused()).toBe("mods-sort-name");
    wrapper.unmount();
    vi.unstubAllGlobals();
  });

  it("leaves focus outside the bar alone when the form swaps", async () => {
    const { wrapper, resize } = await mountResizable();
    const outside = document.createElement("button");
    outside.setAttribute("data-testid", "outside-button");
    document.body.appendChild(outside);
    outside.focus();

    await resize(376);
    expect(document.activeElement).toBe(outside);

    await resize(600);
    expect(document.activeElement).toBe(outside);
    outside.remove();
    wrapper.unmount();
    vi.unstubAllGlobals();
  });
});
