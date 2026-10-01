import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import PairRuleTable from "@/components/rules/PairRuleTable.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { PairRuleDto } from "@/types/generated/PairRuleDto";

/** See `OrderTable.test.ts`'s identical note: `useModLabel` needs Pinia + Pinia Colada and a registered `list_mod_names` fixture. */
function mountTable(pairs: PairRuleDto[], useImportedPairs: boolean) {
  installMockIpc({ list_mod_names: {} });
  return mount(PairRuleTable, {
    props: { pairs, useImportedPairs },
    global: { plugins: [createPinia(), PiniaColada] },
  });
}

const importedPair: PairRuleDto = {
  after: "a.mod",
  before: "b.mod",
  origin: "rimSortCommunity",
  comment: "known-good order",
  promotedFrom: null,
  alreadyPromoted: false,
  overridesDeclared: false,
};

describe("PairRuleTable", () => {
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
  // never the row's own Promote/Delete buttons — a greyed-but-disabled-
  // looking button would fail WCAG's 4.5:1 contrast requirement for
  // interactive elements. The row itself carries a `title` (and a
  // visually-hidden span) explaining why, instead.
  it("greys the data cells, but not the buttons, of an imported row while its toggle is off", async () => {
    const wrapper = mountTable([importedPair], false);
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    const row = wrapper.get('[data-testid="pair-rule-a.mod-b.mod-rimSortCommunity"]');
    expect(row.attributes("title")).toBe("Not in effect: imported pairs are off");
    expect(row.text()).toContain("Not in effect: imported pairs are off");

    const cells = row.findAll('[role="cell"]');
    expect(cells[0]?.classes()).toContain("opacity-50");
    const promoteButton = wrapper.get(
      '[data-testid="pair-rule-a.mod-b.mod-rimSortCommunity-promote"]',
    );
    expect(promoteButton.classes()).not.toContain("opacity-50");
  });

  it("does not grey an imported row while its toggle is on", async () => {
    const wrapper = mountTable([importedPair], true);
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    const row = wrapper.get('[data-testid="pair-rule-a.mod-b.mod-rimSortCommunity"]');
    expect(row.attributes("title")).toBeUndefined();
    expect(row.findAll('[role="cell"]')[0]?.classes()).not.toContain("opacity-50");
  });

  it("never greys a userDecision row, regardless of the toggle", async () => {
    const userPair: PairRuleDto = { ...importedPair, origin: "userDecision" };
    const wrapper = mountTable([userPair], false);
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    const row = wrapper.get('[data-testid="pair-rule-a.mod-b.mod-userDecision"]');
    expect(row.attributes("title")).toBeUndefined();
    expect(row.findAll('[role="cell"]')[0]?.classes()).not.toContain("opacity-50");
  });

  // The declared-edge override: a flagged row
  // shows a loud badge; an ordinary row shows none.
  it("shows an 'overrides declared' badge only on a flagged row", async () => {
    const overridePair: PairRuleDto = {
      ...importedPair,
      origin: "userDecision",
      overridesDeclared: true,
    };
    const wrapper = mountTable([overridePair, { ...importedPair, before: "z.mod" }], false);
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    expect(
      wrapper.get('[data-testid="pair-rule-a.mod-b.mod-userDecision-overrides-declared"]').text(),
    ).toContain("overrides declared");
    expect(
      wrapper
        .find('[data-testid="pair-rule-a.mod-z.mod-rimSortCommunity-overrides-declared"]')
        .exists(),
    ).toBe(false);
  });

  it("emits promote with the row's key when Promote is clicked", async () => {
    const wrapper = mountTable([importedPair], false);
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    await wrapper
      .get('[data-testid="pair-rule-a.mod-b.mod-rimSortCommunity-promote"]')
      .trigger("click");

    expect(wrapper.emitted("promote")).toEqual([
      [{ kind: "pair", after: "a.mod", before: "b.mod" }],
    ]);
  });

  it("disables Promote and names the source once a userDecision copy exists at the same key", async () => {
    const promotedImport: PairRuleDto = { ...importedPair, alreadyPromoted: true };
    const userCopy: PairRuleDto = {
      ...importedPair,
      origin: "userDecision",
      promotedFrom: "rimSortCommunity",
    };
    const wrapper = mountTable([promotedImport, userCopy], false);
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    const promoteButton = wrapper.get(
      '[data-testid="pair-rule-a.mod-b.mod-rimSortCommunity-promote"]',
    );
    expect(promoteButton.attributes("disabled")).toBeDefined();
    expect(promoteButton.text()).toBe("Promoted");

    // The translated origin label ("RimSort community"), not the raw
    // wire value — `PairRuleTable.vue` renders `promotedFrom` through
    // `ruleOriginLabel`, same as the `origin` column itself.
    expect(
      wrapper.get('[data-testid="pair-rule-a.mod-b.mod-userDecision-promoted-from"]').text(),
    ).toContain("RimSort community");
  });

  // `alreadyPromoted`/`promotedFrom` arrive on the row itself, computed
  // backend-side over the *full* rule set before any filter — scanning
  // `pairs` client-side for a sibling row at the same key would break the
  // moment a per-origin filter (the rules page's own origin tabs) left only
  // one of the two rows on screen, so a table mounted with only one row of
  // a promoted pair must still render correctly.
  it("shows 'Promoted' and disabled on a filtered imported row with no sibling present", async () => {
    const wrapper = mountTable([{ ...importedPair, alreadyPromoted: true }], false);
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    const promoteButton = wrapper.get(
      '[data-testid="pair-rule-a.mod-b.mod-rimSortCommunity-promote"]',
    );
    expect(promoteButton.attributes("disabled")).toBeDefined();
    expect(promoteButton.text()).toBe("Promoted");
  });

  it("shows the '(promoted from …)' line on a filtered userDecision row with no sibling present", async () => {
    const userCopy: PairRuleDto = {
      ...importedPair,
      origin: "userDecision",
      promotedFrom: "rimSortCommunity",
    };
    const wrapper = mountTable([userCopy], false);
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    // The translated origin label, not the raw wire value — see the
    // identical note above.
    expect(
      wrapper.get('[data-testid="pair-rule-a.mod-b.mod-userDecision-promoted-from"]').text(),
    ).toContain("RimSort community");
  });

  it("emits delete with the row's key and origin", async () => {
    const wrapper = mountTable([importedPair], false);
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    await wrapper
      .get('[data-testid="pair-rule-a.mod-b.mod-rimSortCommunity"]')
      .findAll("button")
      .at(-1)
      ?.trigger("click");

    expect(wrapper.emitted("delete")).toEqual([
      [{ key: { kind: "pair", after: "a.mod", before: "b.mod" }, origin: "rimSortCommunity" }],
    ]);
  });
});
