import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import BaseProgressBar, { type Progress } from "@/components/base/BaseProgressBar.vue";

function mountBar(progress: Progress, extra: Record<string, unknown> = {}) {
  return mount(BaseProgressBar, { props: { progress, label: "Scanning mods", ...extra } });
}

/** The bar's own text, which PrimeVue renders inside the fill. */
function labelOf(wrapper: ReturnType<typeof mountBar>): string {
  return wrapper.get('[role="progressbar"]').text();
}

function valueNowOf(wrapper: ReturnType<typeof mountBar>): string | undefined {
  return wrapper.get('[role="progressbar"]').attributes("aria-valuenow");
}

describe("BaseProgressBar", () => {
  describe("label", () => {
    it("renders a whole percentage without a decimal place", () => {
      expect(labelOf(mountBar({ done: 1, total: 2 }))).toBe("50%");
    });

    it("truncates a repeating fraction to one decimal place", () => {
      expect(labelOf(mountBar({ done: 1, total: 3 }))).toBe("33.3%");
    });

    it("truncates the real scan's long expansion to one decimal place", () => {
      // 1136 / 1144 — the kind of long expansion the user saw as a bare `99`.
      expect(labelOf(mountBar({ done: 1136, total: 1144 }))).toBe("99.3%");
    });

    it("never claims 100% while work is still outstanding", () => {
      expect(labelOf(mountBar({ done: 9996, total: 10000 }))).toBe("99.9%");
    });

    it("renders a complete bar as 100%, never 100.0%", () => {
      expect(labelOf(mountBar({ done: 7, total: 7 }))).toBe("100%");
    });

    it("takes an explicit percentage instead of a done/total pair", () => {
      expect(labelOf(mountBar({ percent: 12.3456 }))).toBe("12.3%");
    });
  });

  describe("percent", () => {
    it("renders a job with nothing to do yet as empty, not NaN", () => {
      expect(valueNowOf(mountBar({ done: 0, total: 0 }))).toBe("0");
    });

    it("renders a non-finite percentage as empty rather than passing it through", () => {
      // A non-finite number means the caller's own arithmetic broke, so
      // it reads as "unknown" (0), never as "definitely finished".
      expect(valueNowOf(mountBar({ percent: Number.NaN }))).toBe("0");
      expect(valueNowOf(mountBar({ percent: Number.POSITIVE_INFINITY }))).toBe("0");
    });

    it("clamps a negative percentage to zero", () => {
      expect(valueNowOf(mountBar({ percent: -20 }))).toBe("0");
    });

    it("clamps a percentage past the end of the bar", () => {
      expect(valueNowOf(mountBar({ done: 12, total: 10 }))).toBe("100");
      expect(labelOf(mountBar({ percent: 140 }))).toBe("100%");
    });
  });

  describe("indeterminate", () => {
    it("renders a stage at 0 of 1 as indeterminate, with no percentage", () => {
      const wrapper = mountBar({ done: 0, total: 1 });
      const bar = wrapper.get('[role="progressbar"]');

      expect(bar.classes()).toContain("p-progressbar-indeterminate");
      expect(bar.attributes("aria-valuenow")).toBeUndefined();
      expect(bar.text()).toBe("");
      expect(bar.attributes("aria-label")).toBe("Scanning mods");
    });

    it("keeps the test id on the indeterminate bar itself", () => {
      const wrapper = mountBar({ done: 0, total: 1 }, { "data-testid": "setup-progress" });

      expect(wrapper.get('[data-testid="setup-progress"]').attributes("role")).toBe("progressbar");
    });

    it("renders a finished 1 of 1 as a full determinate bar", () => {
      const bar = mountBar({ done: 1, total: 1 }).get('[role="progressbar"]');

      expect(bar.classes()).not.toContain("p-progressbar-indeterminate");
      expect(bar.attributes("aria-valuenow")).toBe("100");
    });

    it("keeps 0 of a larger total determinate", () => {
      const bar = mountBar({ done: 0, total: 2 }).get('[role="progressbar"]');

      expect(bar.classes()).not.toContain("p-progressbar-indeterminate");
      expect(bar.attributes("aria-valuenow")).toBe("0");
    });

    it("keeps an explicit percentage determinate", () => {
      const bar = mountBar({ percent: 0 }).get('[role="progressbar"]');

      expect(bar.classes()).not.toContain("p-progressbar-indeterminate");
    });
  });

  describe("fill", () => {
    it("moves the fill with the label instead of trailing it by a second", () => {
      // PrimeVue's own stylesheet sets `transition: width 1s ease-in-out`
      // here; a ~1000-tick scan then shows a short bar labelled "99%".
      const wrapper = mountBar({ done: 1136, total: 1144 });
      const fill = wrapper.get('[role="progressbar"] > div');

      expect(fill.attributes("style")).toContain("transition: width 120ms linear");
    });
  });

  describe("accessibility", () => {
    it("names the bar so its progressbar role is not anonymous", () => {
      const wrapper = mountBar({ done: 1, total: 2 }, { label: "Verifying order" });

      expect(wrapper.get('[role="progressbar"]').attributes("aria-label")).toBe("Verifying order");
    });

    it("puts the call site's test id on the bar itself, not a wrapper", () => {
      const wrapper = mountBar({ done: 1, total: 2 }, { "data-testid": "setup-progress" });

      expect(wrapper.get('[data-testid="setup-progress"]').attributes("role")).toBe("progressbar");
    });
  });

  describe("caption", () => {
    it("renders the caption under the bar", () => {
      const wrapper = mountBar({ done: 1, total: 2 }, { caption: "Reading mods…" });

      expect(wrapper.get('[data-testid="base-progress-caption"]').text()).toBe("Reading mods…");
    });

    it("renders no caption element when none is given", () => {
      const wrapper = mountBar({ done: 1, total: 2 });

      expect(wrapper.find('[data-testid="base-progress-caption"]').exists()).toBe(false);
    });
  });
});
