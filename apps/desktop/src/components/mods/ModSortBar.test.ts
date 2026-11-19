import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

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
