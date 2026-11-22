import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";

import MarkedText from "@/components/mods/MarkedText.vue";

/**
 * The component renders several root nodes, and a wrapper's `text()` trims each of them, so it is
 * mounted inside a host span, the way a row uses it, to read the text as one.
 */
function inHost(props: { text: string; ranges?: { start: number; end: number }[] }) {
  return mount(defineComponent({ render: () => h("span", [h(MarkedText, props)]) }));
}

function render(text: string, ranges: { start: number; end: number }[]) {
  return inHost({ text, ranges });
}

const MATCH = '[data-testid="search-match"]';

afterEach(() => {
  vi.restoreAllMocks();
});

describe("MarkedText", () => {
  it("renders the text unchanged, with a styled span (not a mark element) per match", () => {
    const wrapper = render("Alpha Alpha", [
      { start: 0, end: 5 },
      { start: 6, end: 11 },
    ]);

    expect(wrapper.text()).toBe("Alpha Alpha");
    expect(wrapper.findAll(MATCH).map((node) => node.text())).toEqual(["Alpha", "Alpha"]);
    expect(wrapper.find("mark").exists()).toBe(false);
  });

  it("marks a match at the start and one at the end", () => {
    const wrapper = render("abcde", [
      { start: 0, end: 1 },
      { start: 4, end: 5 },
    ]);

    expect(wrapper.findAll(MATCH).map((node) => node.text())).toEqual(["a", "e"]);
    expect(wrapper.text()).toBe("abcde");
  });

  it("marks adjacent ranges as two runs of the same text", () => {
    const wrapper = render("aaaa", [
      { start: 0, end: 2 },
      { start: 2, end: 4 },
    ]);

    expect(wrapper.findAll(MATCH)).toHaveLength(2);
    expect(wrapper.text()).toBe("aaaa");
  });

  it("marks the whole text", () => {
    const wrapper = render("Alpha", [{ start: 0, end: 5 }]);

    expect(wrapper.findAll(MATCH).map((node) => node.text())).toEqual(["Alpha"]);
  });

  it("renders plain text and no span without ranges", () => {
    const wrapper = inHost({ text: "Alpha" });

    expect(wrapper.text()).toBe("Alpha");
    expect(wrapper.find(MATCH).exists()).toBe(false);
  });

  it("draws an out-of-range range as no mark at all, with the text whole", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});

    const wrapper = render("Alpha", [{ start: 2, end: 40 }]);

    expect(wrapper.text()).toBe("Alpha");
    expect(wrapper.find(MATCH).exists()).toBe(false);
  });

  it("renders a text containing markup as text, never as elements", () => {
    const wrapper = render("<img src=x onerror=alert(1)>", [{ start: 1, end: 4 }]);

    expect(wrapper.find("img").exists()).toBe(false);
    expect(wrapper.text()).toBe("<img src=x onerror=alert(1)>");
    expect(wrapper.find(MATCH).text()).toBe("img");
  });

  it("gives the marked span a background and an underline, so colour is not the only cue", () => {
    const wrapper = render("Alpha", [{ start: 0, end: 2 }]);

    const classes = wrapper.find(MATCH).classes();
    expect(classes).toContain("bg-search-match");
    expect(classes).toContain("underline");
  });
});
