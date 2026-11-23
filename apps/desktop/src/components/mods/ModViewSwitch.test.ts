import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";

import ModViewSwitch from "@/components/mods/ModViewSwitch.vue";
import type { PaneView } from "@/utils/panePrefs";

let wrappers: ReturnType<typeof mount>[] = [];

function mountSwitch(view: PaneView, isGroupsAvailable = true) {
  const wrapper = mount(ModViewSwitch, {
    props: {
      view,
      isGroupsAvailable,
      "onUpdate:view": (next: PaneView) => wrapper.setProps({ view: next }),
    },
    attachTo: document.body,
  });
  wrappers.push(wrapper);
  return wrapper;
}

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
});

describe("ModViewSwitch", () => {
  it("is a radiogroup of List and Groups with the current view checked and the only tab stop", () => {
    const wrapper = mountSwitch("list");

    expect(wrapper.find('[role="radiogroup"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="mods-view-list"]').attributes("aria-checked")).toBe("true");
    expect(wrapper.get('[data-testid="mods-view-list"]').attributes("tabindex")).toBe("0");
    expect(wrapper.get('[data-testid="mods-view-groups"]').attributes("aria-checked")).toBe(
      "false",
    );
    expect(wrapper.get('[data-testid="mods-view-groups"]').attributes("tabindex")).toBe("-1");
  });

  it("chooses Groups on a click", async () => {
    const wrapper = mountSwitch("list");

    await wrapper.get('[data-testid="mods-view-groups"]').trigger("click");

    expect(wrapper.emitted("update:view")).toEqual([["groups"]]);
  });

  it("moves to the other view with an arrow key and focuses it", async () => {
    const wrapper = mountSwitch("list");
    wrapper.get<HTMLElement>('[data-testid="mods-view-list"]').element.focus();

    await wrapper.get('[data-testid="mods-view-list"]').trigger("keydown", { key: "ArrowRight" });
    await wrapper.vm.$nextTick();

    expect(wrapper.emitted("update:view")).toEqual([["groups"]]);
    expect(document.activeElement).toBe(wrapper.get('[data-testid="mods-view-groups"]').element);
  });

  it("refuses Groups, by click and by arrow, while groups are not usable, and says why", async () => {
    const wrapper = mountSwitch("list", false);

    await wrapper.get('[data-testid="mods-view-groups"]').trigger("click");
    await wrapper.get('[data-testid="mods-view-list"]').trigger("keydown", { key: "ArrowRight" });

    expect(wrapper.emitted("update:view")).toBeUndefined();
    expect(wrapper.get('[data-testid="mods-view-groups"]').attributes("aria-disabled")).toBe(
      "true",
    );
    expect(wrapper.get('[data-testid="mods-view-unavailable"]').text()).toContain("can't be used");
  });

  it("offers Collapse all and Expand all only in Groups view", async () => {
    const list = mountSwitch("list");
    const groups = mountSwitch("groups");

    expect(list.find('[data-testid="mods-groups-collapse-all"]').exists()).toBe(false);
    await groups.get('[data-testid="mods-groups-collapse-all"]').trigger("click");
    await groups.get('[data-testid="mods-groups-expand-all"]').trigger("click");

    expect(groups.emitted("collapseAll")).toHaveLength(1);
    expect(groups.emitted("expandAll")).toHaveLength(1);
  });
});
