import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";

import ModDescription from "@/components/mods/ModDescription.vue";
import { createAppI18n, registerAppI18n } from "@/i18n/i18n";
import type { DescriptionDto } from "@/types/generated/DescriptionDto";

registerAppI18n(createAppI18n());

function mountDescription(description: DescriptionDto | null) {
  return mount(ModDescription, { props: { description } });
}

describe("ModDescription", () => {
  it("shows the empty-state message when there is no description", () => {
    const wrapper = mountDescription(null);

    expect(wrapper.find('[data-testid="mod-description-empty"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="mod-description-text"]').exists()).toBe(false);
  });

  it("shows the empty-state message when the description has no runs", () => {
    const wrapper = mountDescription({ runs: [], truncated: false });

    expect(wrapper.find('[data-testid="mod-description-empty"]').exists()).toBe(true);
  });

  it("renders plain runs as text, applying bold/italic classes per run", () => {
    const wrapper = mountDescription({
      runs: [
        { text: "Hello ", bold: false, italic: false },
        { text: "bold", bold: true, italic: false },
        { text: " and ", bold: false, italic: false },
        { text: "italic", bold: false, italic: true },
      ],
      truncated: false,
    });

    const text = wrapper.get('[data-testid="mod-description-text"]');
    expect(text.text()).toBe("Hello bold and italic");
    const spans = text.findAll("span");
    expect(spans[1]?.classes()).toContain("font-bold");
    expect(spans[3]?.classes()).toContain("italic");
  });

  it("shows the truncated notice only when the description was truncated", () => {
    const truncated = mountDescription({
      runs: [{ text: "x", bold: false, italic: false }],
      truncated: true,
    });
    expect(truncated.find('[data-testid="mod-description-truncated"]').exists()).toBe(true);

    const notTruncated = mountDescription({
      runs: [{ text: "x", bold: false, italic: false }],
      truncated: false,
    });
    expect(notTruncated.find('[data-testid="mod-description-truncated"]').exists()).toBe(false);
  });

  describe("show more toggle", () => {
    const ONE_RUN: DescriptionDto = {
      runs: [{ text: "x", bold: false, italic: false }],
      truncated: false,
    };

    function stubHeights(scrollHeight: number, clientHeight: number): void {
      // Shadowed on HTMLElement.prototype (removed in afterEach): the layout-less test DOM
      // reports 0 for both, and which prototype owns the getters varies by DOM.
      Object.defineProperty(HTMLElement.prototype, "scrollHeight", {
        value: scrollHeight,
        configurable: true,
      });
      Object.defineProperty(HTMLElement.prototype, "clientHeight", {
        value: clientHeight,
        configurable: true,
      });
    }

    afterEach(() => {
      Reflect.deleteProperty(HTMLElement.prototype, "scrollHeight");
      Reflect.deleteProperty(HTMLElement.prototype, "clientHeight");
    });

    it("is absent when the text fits inside the clamp", () => {
      stubHeights(40, 40);
      const wrapper = mountDescription(ONE_RUN);

      expect(wrapper.find('[data-testid="mod-description-toggle"]').exists()).toBe(false);
    });

    it("is absent when the text is within one pixel of the clamp", () => {
      stubHeights(241, 240);
      const wrapper = mountDescription(ONE_RUN);

      expect(wrapper.find('[data-testid="mod-description-toggle"]').exists()).toBe(false);
    });

    it("toggles between clamped and expanded view, staying visible once expanded", async () => {
      stubHeights(600, 240);
      const wrapper = mountDescription(ONE_RUN);
      await wrapper.vm.$nextTick();

      const text = wrapper.get('[data-testid="mod-description-text"]');
      expect(text.classes()).toContain("line-clamp-12");

      await wrapper.get('[data-testid="mod-description-toggle"]').trigger("click");
      expect(text.classes().some((cls) => cls.startsWith("line-clamp-"))).toBe(false);
      // Expanded, the element no longer overflows its own height.
      expect(wrapper.get('[data-testid="mod-description-toggle"]').text()).toBe("Show less");

      // Expanded, the element no longer overflows its own height; a same-text
      // refetch keeps the expanded state and its "Show less" toggle.
      stubHeights(600, 600);
      await wrapper.setProps({ description: { ...ONE_RUN } });
      await wrapper.vm.$nextTick();
      expect(wrapper.get('[data-testid="mod-description-toggle"]').text()).toBe("Show less");
      expect(
        wrapper.get('[data-testid="mod-description-toggle"]').attributes("aria-expanded"),
      ).toBe("true");

      await wrapper.get('[data-testid="mod-description-toggle"]').trigger("click");
      expect(text.classes()).toContain("line-clamp-12");
    });
  });

  it("XSS regression: a run containing a literal <img onerror=...> string renders as inert text, never a real img element", () => {
    const hostile = '<img src=x onerror="window.__pwned = true">';
    const wrapper = mountDescription({
      runs: [{ text: hostile, bold: false, italic: false }],
      truncated: false,
    });

    const text = wrapper.get('[data-testid="mod-description-text"]');
    // The hostile string reaches the DOM as literal text content, not
    // parsed markup — no `<img>` element is ever created from it.
    expect(text.find("img").exists()).toBe(false);
    expect(text.text()).toBe(hostile);
    expect(text.html()).not.toContain("<img");
  });

  it("XSS regression: a run containing a literal <script> string never executes or creates a script element", () => {
    const hostile = "<script>window.__pwned = true</script>";
    const wrapper = mountDescription({
      runs: [{ text: hostile, bold: false, italic: false }],
      truncated: false,
    });

    const text = wrapper.get('[data-testid="mod-description-text"]');
    expect(text.find("script").exists()).toBe(false);
    expect(text.text()).toBe(hostile);
  });
});
