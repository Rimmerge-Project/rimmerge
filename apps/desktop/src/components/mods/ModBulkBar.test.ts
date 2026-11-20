import { mount } from "@vue/test-utils";
import PrimeVue from "primevue/config";
import { describe, expect, it } from "vitest";

import ModBulkBar from "@/components/mods/ModBulkBar.vue";

function mountBar(hasOrganizationActions: boolean, kind: "active" | "inactive" = "active") {
  return mount(ModBulkBar, {
    props: {
      kind,
      listName: kind === "active" ? "Active" : "Inactive",
      hasOrganizationActions,
    },
    global: { plugins: [PrimeVue] },
  });
}

describe("ModBulkBar", () => {
  it("offers every organization action and tags when organization is usable", () => {
    const wrapper = mountBar(true);

    expect(wrapper.text()).toContain("Add to group…");
    expect(wrapper.text()).toContain("Remove from group…");
    expect(wrapper.text()).toContain("Move to group…");
    expect(wrapper.text()).toContain("Colour…");
    expect(wrapper.text()).toContain("Tags…");
  });

  it("names each list's bar after the list, so the two groups never share an accessible name", () => {
    const active = mountBar(true, "active").get('[role="group"]');
    const inactive = mountBar(true, "inactive").get('[role="group"]');

    expect(active.attributes("aria-label")).toBe(
      "Actions for the selected mods in the Active list",
    );
    expect(inactive.attributes("aria-label")).toBe(
      "Actions for the selected mods in the Inactive list",
    );
  });

  it("offers only tags when organization is not usable, because tags never need it", () => {
    const wrapper = mountBar(false);

    expect(wrapper.find('[data-testid="bulk-add-to-group-active"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="bulk-color-active"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="bulk-tags-active"]').exists()).toBe(true);
  });

  it.each([
    ["bulk-add-to-group-active", "addToGroup"],
    ["bulk-remove-from-group-active", "removeFromGroup"],
    ["bulk-move-to-group-active", "moveToGroup"],
    ["bulk-color-active", "color"],
    ["bulk-tags-active", "tags"],
  ])("%s asks for %s and nothing else", async (testId, action) => {
    const wrapper = mountBar(true);

    await wrapper.get(`[data-testid="${testId}"]`).trigger("click");

    expect(wrapper.emitted("action")).toEqual([[action]]);
  });
});
