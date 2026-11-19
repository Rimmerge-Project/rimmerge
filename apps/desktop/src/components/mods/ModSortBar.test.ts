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

const NAMES: readonly { key: ListSortKeyDto; asc: string; desc: string }[] = [
  {
    key: "loadOrder",
    asc: "Sort by load order, first to last",
    desc: "Sort by load order, last to first",
  },
  { key: "name", asc: "Sort by name, A to Z", desc: "Sort by name, Z to A" },
  { key: "packageId", asc: "Sort by package ID, A to Z", desc: "Sort by package ID, Z to A" },
  {
    key: "lastUpdated",
    asc: "Sort by last updated, oldest first",
    desc: "Sort by last updated, newest first",
  },
];

describe("ModSortBar", () => {
  it.each(NAMES)("names both the criterion and the direction for $key", ({ key, asc, desc }) => {
    const wrapper = mountBar();

    const ascending = wrapper.get(`[data-testid="mods-sort-${key}-ascending"]`);
    const descending = wrapper.get(`[data-testid="mods-sort-${key}-descending"]`);

    expect(ascending.attributes("aria-label")).toBe(asc);
    expect(ascending.attributes("title")).toBe(asc);
    expect(descending.attributes("aria-label")).toBe(desc);
    expect(descending.attributes("title")).toBe(desc);
  });

  it("marks exactly the active arrow as pressed", () => {
    const wrapper = mountBar(ACTIVE_SORT_KEYS, { key: "packageId", direction: "descending" });

    const pressed = wrapper
      .findAll("button")
      .filter((button) => button.attributes("aria-pressed") === "true")
      .map((button) => button.attributes("data-testid"));

    expect(pressed).toEqual(["mods-sort-packageId-descending"]);
  });

  it("gives the pressed arrow a thicker border, not just a colour", () => {
    const wrapper = mountBar(ACTIVE_SORT_KEYS, { key: "name", direction: "ascending" });

    expect(wrapper.get('[data-testid="mods-sort-name-ascending"]').classes()).toContain("border-2");
    expect(wrapper.get('[data-testid="mods-sort-name-descending"]').classes()).not.toContain(
      "border-2",
    );
  });

  it("offers every criterion twice for Active and omits load order for Inactive", () => {
    expect(mountBar(ACTIVE_SORT_KEYS).findAll("button")).toHaveLength(8);

    const inactive = mountBar(INACTIVE_SORT_KEYS);

    expect(inactive.findAll("button")).toHaveLength(6);
    expect(inactive.find('[data-testid="mods-sort-loadOrder-ascending"]').exists()).toBe(false);
  });

  it("emits the chosen criterion and direction when an arrow is clicked", async () => {
    const wrapper = mount(ModSortBar, { props: { keys: ACTIVE_SORT_KEYS, sort: NAME_ASC } });

    await wrapper.get('[data-testid="mods-sort-lastUpdated-descending"]').trigger("click");

    expect(wrapper.emitted("update:sort")).toEqual([
      [{ key: "lastUpdated", direction: "descending" }],
    ]);
  });

  it("is keyboard operable: every arrow is a native button in the tab order", () => {
    const wrapper = mountBar();

    for (const button of wrapper.findAll("button")) {
      expect(button.element.tagName).toBe("BUTTON");
      expect(button.attributes("type")).toBe("button");
      expect(button.attributes("tabindex")).toBeUndefined();
      expect(button.attributes("disabled")).toBeUndefined();
    }
  });

  it("is a labelled group", () => {
    const wrapper = mountBar();

    expect(wrapper.get('[role="group"]').attributes("aria-label")).toBe("Sort order");
  });
});
