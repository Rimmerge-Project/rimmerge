import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { defineComponent, h } from "vue";

import { useTranslateMessage } from "@/composables/useTranslateMessage";

const Host = defineComponent({
  setup() {
    const tm = useTranslateMessage();
    return { tm };
  },
  render() {
    return h("span", [
      this.tm({ key: "shell.brand" }),
      "|",
      this.tm({ key: "shell.nav.dashboard", params: {} }),
      "|",
      this.tm({ key: "setup.scanFinishedToast", params: { count: 2 }, count: 2 }),
      "|",
      this.tm({ key: "setup.scanFinishedToast", params: { count: 1 }, count: 1 }),
    ]);
  },
});

describe("useTranslateMessage", () => {
  it("renders a descriptor with no params, one with params, and a plural one selected by its own count", () => {
    const wrapper = mount(Host);

    expect(wrapper.text()).toBe(
      "Rimmerge|Dashboard|Scan finished with 2 notes|Scan finished with 1 note",
    );
  });
});
