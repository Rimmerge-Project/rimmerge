import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import RuleTable from "@/components/rules/RuleTable.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { PlacementRuleDto } from "@/types/generated/PlacementRuleDto";
import type { RuleSetDto } from "@/types/generated/RuleSetDto";

/** See `OrderTable.test.ts`'s identical note: `useModLabel` needs Pinia + Pinia Colada and a registered `list_mod_names` fixture. */
function mountTable(rules: RuleSetDto, useImportedPairs: boolean, useImportedPlacements: boolean) {
  installMockIpc({ list_mod_names: {} });
  return mount(RuleTable, {
    props: { rules, useImportedPairs, useImportedPlacements },
    global: { plugins: [createPinia(), PiniaColada] },
  });
}

const importedPlacement: PlacementRuleDto = {
  modId: "a.mod",
  placement: "bottom",
  origin: "rimSortCommunity",
  comment: null,
  promotedFrom: null,
  alreadyPromoted: false,
};

const rulesFixture: RuleSetDto = {
  pairs: [],
  placements: [importedPlacement],
  incompatibles: [{ a: "x.mod", b: "y.mod", origin: "rimSortUser" }],
  warnings: [],
};

describe("RuleTable", () => {
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      value: 400,
    });
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
      configurable: true,
      value: 600,
    });
  });

  afterEach(() => {
    clearMocks();
    Reflect.deleteProperty(HTMLElement.prototype, "offsetHeight");
    Reflect.deleteProperty(HTMLElement.prototype, "offsetWidth");
  });

  // Greying is scoped to the data cells,
  // never the row's own Promote/Delete buttons (WCAG 4.5:1 for
  // interactive elements) — the row itself carries a `title` (and a
  // visually-hidden span) explaining why instead.
  it("greys the data cells, but not the buttons, of an imported placement row while its toggle is off", () => {
    const wrapper = mountTable(rulesFixture, false, false);

    const row = wrapper.get('[data-testid="placement-rule-a.mod-rimSortCommunity"]');
    expect(row.attributes("title")).toBe("Not in effect: imported placements are off");
    expect(row.text()).toContain("Not in effect: imported placements are off");
    expect(row.findAll("td")[0]?.classes()).toContain("opacity-50");

    const promoteButton = wrapper.get(
      '[data-testid="placement-rule-a.mod-rimSortCommunity-promote"]',
    );
    expect(promoteButton.classes()).not.toContain("opacity-50");
  });

  it("does not grey an imported placement row while its toggle is on", () => {
    const wrapper = mountTable(rulesFixture, false, true);

    const row = wrapper.get('[data-testid="placement-rule-a.mod-rimSortCommunity"]');
    expect(row.attributes("title")).toBeUndefined();
    expect(row.findAll("td")[0]?.classes()).not.toContain("opacity-50");
  });

  it("emits promote with the placement's key when Promote is clicked", async () => {
    const wrapper = mountTable(rulesFixture, false, false);

    await wrapper
      .get('[data-testid="placement-rule-a.mod-rimSortCommunity-promote"]')
      .trigger("click");

    expect(wrapper.emitted("promote")).toEqual([[{ kind: "placement", modId: "a.mod" }]]);
  });

  it("emits delete with the placement's key and origin", async () => {
    const wrapper = mountTable(rulesFixture, false, false);

    await wrapper
      .get('[data-testid="placement-rule-a.mod-rimSortCommunity"]')
      .findAll("button")
      .at(-1)
      ?.trigger("click");

    expect(wrapper.emitted("delete")).toEqual([
      [{ key: { kind: "placement", modId: "a.mod" }, origin: "rimSortCommunity" }],
    ]);
  });

  it("emits delete with the incompatible pair's key and origin, and never offers to promote it", () => {
    const wrapper = mountTable(rulesFixture, false, false);

    expect(wrapper.find('[data-testid="incompatible-rule-x.mod-y.mod"]').text()).not.toContain(
      "Promote",
    );
  });

  // `alreadyPromoted`/`promotedFrom`
  // arrive pre-computed on each row (over the full, unfiltered rule set,
  // backend-side) rather than being scanned from sibling rows client-side
  // — so a filtered set holding only one row of a promoted pair must
  // still render correctly.
  it("shows 'Promoted' and disabled on a filtered imported placement row with no sibling present", async () => {
    const filtered: RuleSetDto = {
      ...rulesFixture,
      placements: [{ ...importedPlacement, alreadyPromoted: true }],
    };
    const wrapper = mountTable(filtered, false, false);

    const promoteButton = wrapper.get(
      '[data-testid="placement-rule-a.mod-rimSortCommunity-promote"]',
    );
    expect(promoteButton.attributes("disabled")).toBeDefined();
    expect(promoteButton.text()).toBe("Promoted");
  });

  it("shows the '(promoted from …)' line on a filtered userDecision placement row with no sibling present", () => {
    const filtered: RuleSetDto = {
      ...rulesFixture,
      placements: [
        {
          ...importedPlacement,
          origin: "userDecision",
          promotedFrom: "rimSortCommunity",
        },
      ],
    };
    const wrapper = mountTable(filtered, false, false);

    // The translated origin label ("RimSort community"), not the raw
    // wire value — `RuleTable.vue` renders `promotedFrom` through
    // `ruleOriginLabel`, same as the `origin` column itself.
    expect(
      wrapper.get('[data-testid="placement-rule-a.mod-userDecision-promoted-from"]').text(),
    ).toContain("RimSort community");
  });

  // The rule-level promotes-dependents companion: the promoted-dependent
  // count is keyed by modId,
  // independent of origin, and absent entirely (rather than `0`) for the
  // common uncontested-pin case.
  it("shows the promoted-dependent count on a pin that has one", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mount(RuleTable, {
      props: {
        rules: rulesFixture,
        useImportedPairs: false,
        useImportedPlacements: false,
        dependentCounts: { "a.mod": 114 },
      },
      global: { plugins: [createPinia(), PiniaColada] },
    });

    expect(
      wrapper.get('[data-testid="placement-rule-a.mod-rimSortCommunity-promotes"]').text(),
    ).toContain("114 other mods");
  });

  it("shows no promoted-dependent count when the map carries nothing for that mod", () => {
    const wrapper = mountTable(rulesFixture, false, false);

    expect(
      wrapper.find('[data-testid="placement-rule-a.mod-rimSortCommunity-promotes"]').exists(),
    ).toBe(false);
  });
});
