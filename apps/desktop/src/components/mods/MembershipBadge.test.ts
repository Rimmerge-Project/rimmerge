import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import MembershipBadge from "@/components/mods/MembershipBadge.vue";
import SourceBadge from "@/components/mods/SourceBadge.vue";

describe("MembershipBadge", () => {
  it("says Added, with a plus icon and a tooltip about applying", () => {
    const wrapper = mount(MembershipBadge, { props: { mark: "added" } });

    expect(wrapper.text()).toContain("Added");
    expect(wrapper.attributes("title")).toBe(
      "Not in your saved active mod list yet. Applying adds it.",
    );
    expect(wrapper.attributes("data-mark")).toBe("added");
    expect(wrapper.get("i").classes()).toContain("pi-plus");
    expect(wrapper.get("i").attributes("aria-hidden")).toBe("true");
  });

  it("says Removed, with a minus icon and a tooltip about applying", () => {
    const wrapper = mount(MembershipBadge, { props: { mark: "removed" } });

    expect(wrapper.text()).toContain("Removed");
    expect(wrapper.attributes("title")).toBe("In your saved active mod list. Applying removes it.");
    expect(wrapper.get("i").classes()).toContain("pi-minus");
  });

  it("draws nothing for a row that matches the saved list", () => {
    const wrapper = mount(MembershipBadge, { props: { mark: "none" } });

    expect(wrapper.find('[data-testid="membership-badge"]').exists()).toBe(false);
  });

  it("gives a screen reader the meaning, not only the word", () => {
    const wrapper = mount(MembershipBadge, { props: { mark: "added" } });

    expect(wrapper.get(".sr-only").text()).toContain("Applying adds it.");
  });

  it("is a different kind of element than the neutral source badge, so the two sit side by side", () => {
    const membership = mount(MembershipBadge, { props: { mark: "added" } });
    const source = mount(SourceBadge, { props: { source: "workshop" } });

    expect(membership.attributes("data-testid")).not.toBe(source.attributes("data-testid"));
    expect(membership.attributes("role")).toBeUndefined();
    expect(membership.classes().join(" ")).toContain("bg-status-auto-soft");
    expect(source.classes().join(" ")).not.toContain("bg-status");
  });
});
