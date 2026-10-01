import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import ModPicker from "@/components/mods/ModPicker.vue";
import { installMockIpc } from "@/services/ipc.mock";

function mountPicker(targetLabel: string, testidPrefix: string) {
  installMockIpc({ list_mods: { total: 0, items: [] } });
  return mount(ModPicker, {
    props: { members: [], targetLabel, testidPrefix, dataTestid: "picker" },
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
}

describe("ModPicker", () => {
  afterEach(() => {
    clearMocks();
  });

  it("names its search box after the field the caller passes, never the internal test id", () => {
    const wrapper = mountPicker("Reference mods (R)", "edit-refs");

    const label = wrapper.get('[data-testid="edit-refs-search"]').attributes("aria-label");

    expect(label).toBe("Search mods for Reference mods (R)");
    expect(label).not.toContain("edit-refs");
  });

  it("gives two pickers with different fields different accessible names", () => {
    const after = mountPicker("Loads after (the dependent mod)", "add-pair-rule-after");
    const before = mountPicker("Loads before (the dependency)", "add-pair-rule-before");

    expect(after.get('[data-testid="add-pair-rule-after-search"]').attributes("aria-label")).toBe(
      "Search mods for Loads after (the dependent mod)",
    );
    expect(before.get('[data-testid="add-pair-rule-before-search"]').attributes("aria-label")).toBe(
      "Search mods for Loads before (the dependency)",
    );
  });
});
