import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import XmlPreview from "@/components/merge/XmlPreview.vue";

describe("XmlPreview", () => {
  it("renders one line number per line, in order, alongside the line's text", () => {
    const wrapper = mount(XmlPreview, {
      props: { content: '<?xml version="1.0"?>\n<Patch>\n<Operation />\n</Patch>' },
    });

    const text = wrapper.get('[data-testid="xml-preview"]').text();
    expect(text).toContain('1<?xml version="1.0"?>');
    expect(text).toContain("2<Patch>");
    expect(text).toContain("3<Operation />");
    expect(text).toContain("4</Patch>");
  });

  it("strips exactly one trailing newline so no spurious empty last line renders", () => {
    const wrapper = mount(XmlPreview, {
      props: { content: "<Patch>\n</Patch>\n" },
    });

    const lineNumbers = wrapper
      .findAll('[data-testid="xml-preview"] span.text-text-faint')
      .map((span) => span.text());
    expect(lineNumbers).toEqual(["1", "2"]);
  });

  it("is keyboard-focusable as a labeled region", () => {
    const wrapper = mount(XmlPreview, {
      props: { content: "<Patch />" },
    });

    const region = wrapper.get('[data-testid="xml-preview"]');
    expect(region.attributes("tabindex")).toBe("0");
    expect(region.attributes("role")).toBe("region");
    expect(region.attributes("aria-label")).toBe("File preview");
  });
});
