import Aura from "@primevue/themes/aura";
import { mount } from "@vue/test-utils";
import PrimeVue from "primevue/config";
import { describe, expect, it } from "vitest";
import { computed } from "vue";
import { createI18n } from "vue-i18n";

import ModSelectionBar from "@/components/mods/ModSelectionBar.vue";

/**
 * The selection sentence puts two numbers in one line: how many are selected and how many of
 * those a filter hides. Each is worded by its own plural rule, so a language that changes the noun
 * with the number (ru, pl, uk) can agree both. These messages give the two parts different words
 * for one and for many, so a count borrowed from the other part shows at once.
 */
const messages = {
  mods: {
    selection: {
      selectAllShown: "all {count} | all {count}",
      selectAllEmpty: "all",
      selectedCount: "{count} selected one | {count} selected many",
      selectedCountHidden: "{selected} <{hidden}>",
      hiddenPart: "{count} hidden one | {count} hidden many",
      clear: "clear",
    },
  },
};

function mountBar(selectedCount: number, hiddenSelectedCount: number) {
  return mount(ModSelectionBar, {
    props: {
      selection: {
        shownCount: computed(() => 10),
        selectedCount: computed(() => selectedCount),
        isCountPaused: computed(() => false),
        areAllShownSelected: computed(() => false),
        areSomeShownSelected: computed(() => selectedCount > 0),
        hiddenSelectedCount: computed(() => hiddenSelectedCount),
        selectAllShown: () => {},
        deselectShown: () => {},
        clear: () => {},
      },
    },
    global: {
      plugins: [
        [PrimeVue, { theme: { preset: Aura } }],
        createI18n({ legacy: false, locale: "en", messages: { en: messages } }),
      ],
    },
  });
}

describe("ModSelectionBar", () => {
  it.each([
    { selected: 5, hidden: 1, text: "5 selected many <1 hidden one>" },
    { selected: 1, hidden: 4, text: "1 selected one <4 hidden many>" },
  ])("words each number by its own plural: $text", ({ selected, hidden, text }) => {
    const wrapper = mountBar(selected, hidden);

    expect(wrapper.get('[data-testid="mods-selected-count"]').text()).toBe(text);
  });

  it("says only how many are selected while nothing is hidden", () => {
    const wrapper = mountBar(3, 0);

    expect(wrapper.get('[data-testid="mods-selected-count"]').text()).toBe("3 selected many");
  });
});
