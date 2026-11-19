import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import SourceBadge from "@/components/mods/SourceBadge.vue";
import type { SourceDto } from "@/types/generated/SourceDto";

const CASES: { source: SourceDto; text: string; icon: string }[] = [
  { source: "core", text: "RimWorld Core", icon: "pi-box" },
  { source: "dlc", text: "RimWorld DLC", icon: "pi-th-large" },
  { source: "local", text: "Local mod", icon: "pi-folder" },
  { source: "workshop", text: "Steam Workshop", icon: "pi-cloud" },
];
const UNKNOWN_TEXT = "Origin unknown (not installed)";

describe("SourceBadge", () => {
  it.each(CASES)("labels a $source mod with its descriptive label", ({ source, text }) => {
    const wrapper = mount(SourceBadge, { props: { source } });

    expect(wrapper.attributes("role")).toBe("img");
    expect(wrapper.attributes("aria-label")).toBe(text);
    expect(wrapper.attributes("title")).toBe(text);
    expect(wrapper.attributes("data-source")).toBe(source);
  });

  it.each([...CASES, { source: null, text: UNKNOWN_TEXT, icon: "pi-question-circle" }])(
    "draws the $icon icon for source $source, hidden from assistive tech",
    ({ source, icon }) => {
      const wrapper = mount(SourceBadge, { props: { source } });

      const drawn = wrapper.get("i");
      expect(drawn.attributes("aria-hidden")).toBe("true");
      expect(drawn.classes()).toContain(icon);
    },
  );

  it("shows the label as visible text, hidden from assistive tech so the name is read once", () => {
    const wrapper = mount(SourceBadge, { props: { source: "workshop" } });

    const visible = wrapper.get('[data-testid="source-badge-label"]');
    expect(visible.text()).toBe("Steam Workshop");
    expect(visible.attributes("aria-hidden")).toBe("true");
  });

  it("widens by the width of its list (a container query), not of the window", () => {
    const wrapper = mount(SourceBadge, { props: { source: "workshop" } });

    // Two lists sit side by side, so the window width says nothing about the room one has.
    expect(wrapper.get('[data-testid="source-badge-label"]').classes()).toContain("@lg:inline");
    expect(wrapper.classes()).toContain("@lg:w-36");
    expect(wrapper.classes().some((name) => name.startsWith("xl:"))).toBe(false);
  });

  it("shows an explicit origin-unknown state for a missing mod, not a known source", () => {
    const wrapper = mount(SourceBadge, { props: { source: null } });

    expect(wrapper.attributes("aria-label")).toBe(UNKNOWN_TEXT);
    expect(wrapper.attributes("title")).toBe(UNKNOWN_TEXT);
    expect(wrapper.attributes("data-source")).toBe("unknown");
  });
});
