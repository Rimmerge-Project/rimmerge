import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import CoverageList from "@/components/assignments/CoverageList.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { CoverageRowDto } from "@/types/generated/CoverageRowDto";

const UNCOVERED: CoverageRowDto = {
  target: { keyField: "raceNames", def: { defType: "ThingDef", defName: "Elf" } },
  owner: "target.races",
  matches: [],
  intent: "cover",
  winner: null,
  hasRow: false,
};

const COVERED_BY_THIS_PROJECT: CoverageRowDto = {
  target: { keyField: "raceNames", def: { defType: "ThingDef", defName: "Dwarf" } },
  owner: "target.races",
  matches: [],
  intent: "cover",
  winner: null,
  hasRow: true,
};

const OVERRIDE_ROW: CoverageRowDto = {
  target: { keyField: "raceNames", def: { defType: "ThingDef", defName: "Orc" } },
  owner: "target.races",
  matches: [{ owner: "addon.elves", instanceDefName: "Group_Orc", keyField: "raceNames" }],
  intent: "override",
  winner: {
    kind: "existing",
    match: { owner: "addon.elves", instanceDefName: "Group_Orc", keyField: "raceNames" },
  },
  hasRow: false,
};

function mountList(rows: CoverageRowDto[]) {
  installMockIpc({ list_mod_names: {} });
  return mount(CoverageList, {
    props: { rows },
    global: { plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]] },
  });
}

describe("CoverageList", () => {
  afterEach(() => {
    clearMocks();
  });

  it("badges an uncovered row distinctly from a covered-by-this-project row", async () => {
    const wrapper = mountList([UNCOVERED, COVERED_BY_THIS_PROJECT]);
    await flushPromises();

    const uncoveredCell = wrapper.get('[data-testid="coverage-cell-Elf"]');
    expect(uncoveredCell.text()).toBe("uncovered");
    expect(uncoveredCell.classes()).toContain("text-status-input");

    const ownRowCell = wrapper.get('[data-testid="coverage-cell-Dwarf"]');
    expect(ownRowCell.text()).toBe("this project");
  });

  it("shows the covering owner and the precedence winner for an override row", async () => {
    const wrapper = mountList([OVERRIDE_ROW]);
    await flushPromises();

    const cell = wrapper.get('[data-testid="coverage-cell-Orc"]');
    expect(cell.text()).toContain("covered by addon.elves");
    expect(wrapper.get('[data-testid="coverage-row-Orc"]').text()).toContain(
      "addon.elves would win",
    );
  });

  it("filters by search text over the target's own defName", async () => {
    const wrapper = mountList([UNCOVERED, OVERRIDE_ROW]);
    await flushPromises();

    await wrapper.get('[data-testid="coverage-search"]').setValue("orc");

    expect(wrapper.find('[data-testid="coverage-row-Elf"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="coverage-row-Orc"]').exists()).toBe(true);
  });

  it("emits select with the clicked row", async () => {
    const wrapper = mountList([UNCOVERED]);
    await flushPromises();

    await wrapper.get('[data-testid="coverage-row-Elf"]').trigger("click");

    expect(wrapper.emitted("select")).toEqual([[UNCOVERED]]);
  });

  it("marks the selected row with aria-current, not background color alone", async () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mount(CoverageList, {
      props: { rows: [UNCOVERED, OVERRIDE_ROW], selectedTarget: OVERRIDE_ROW.target },
      global: { plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]] },
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="coverage-row-Orc"]').attributes("aria-current")).toBe("true");
    expect(wrapper.get('[data-testid="coverage-row-Elf"]').attributes("aria-current")).toBe(
      "false",
    );
  });

  it("renders one decorative thumbnail per row, inside the row's own button", async () => {
    const wrapper = mountList([UNCOVERED, OVERRIDE_ROW]);
    await flushPromises();

    for (const name of ["Elf", "Orc"]) {
      const thumbnail = wrapper.get(
        `[data-testid="coverage-row-${name}"] [data-testid="def-thumbnail"]`,
      );
      expect(thumbnail.attributes("aria-hidden")).toBe("true");
      expect(thumbnail.classes()).toContain("shrink-0");
    }
    expect(wrapper.findAll('[data-testid="def-thumbnail"]')).toHaveLength(2);
  });
});
