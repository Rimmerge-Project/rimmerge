import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import AddPairRuleDialog from "@/components/rules/AddPairRuleDialog.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ModSummaryDto } from "@/types/generated/ModSummaryDto";
import type { PairRuleDto } from "@/types/generated/PairRuleDto";

function mod(overrides: Partial<ModSummaryDto> = {}): ModSummaryDto {
  return {
    modId: "c.mod",
    name: "Mod C",
    source: "workshop",
    tags: [],
    hardDependents: 0,
    missing: false,
    generated: null,
    workshopId: null,
    ...overrides,
  };
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

async function mountDialog(props: { pending?: boolean; existingPairs?: PairRuleDto[] } = {}) {
  installMockIpc({
    list_mods: {
      total: 2,
      items: [mod(), mod({ modId: "d.mod", name: "Mod D" })],
    },
  });
  const wrapper = mount(AddPairRuleDialog, {
    props: {
      visible: true,
      pending: props.pending ?? false,
      existingPairs: props.existingPairs ?? [],
    },
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true },
    },
  });
  await flush();
  await wrapper.vm.$nextTick();
  return wrapper;
}

/** Picks a mod through one of the two reused `ModPicker`s' own search-and-click flow. */
async function pickMod(
  wrapper: Awaited<ReturnType<typeof mountDialog>>,
  side: "after" | "before",
  modId: string,
): Promise<void> {
  await wrapper.get(`[data-testid="add-pair-rule-${side}-search"]`).setValue("mod");
  await flush();
  await wrapper.vm.$nextTick();
  await wrapper.get(`[data-testid="add-pair-rule-${side}-add-${modId}"]`).trigger("click");
}

describe("AddPairRuleDialog", () => {
  afterEach(() => {
    clearMocks();
  });

  it("sends the exact expected rule on a valid submit", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper, "after", "c.mod");
    await pickMod(wrapper, "before", "d.mod");
    await wrapper.get('[data-testid="add-pair-rule-comment"]').setValue("c.mod needs d.mod first");

    await wrapper.get('[data-testid="add-pair-rule-form"]').trigger("submit");

    expect(wrapper.emitted("add")).toEqual([
      [
        {
          kind: "pair",
          after: "c.mod",
          before: "d.mod",
          origin: "userDecision",
          comment: "c.mod needs d.mod first",
          promotedFrom: null,
          alreadyPromoted: false,
          overridesDeclared: false,
        },
      ],
    ]);
  });

  it("sends overridesDeclared: true when the checkbox is checked, and resets it on reopen", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper, "after", "c.mod");
    await pickMod(wrapper, "before", "d.mod");
    await wrapper.get('[data-testid="add-pair-rule-override-declared"] input').setValue(true);

    await wrapper.get('[data-testid="add-pair-rule-form"]').trigger("submit");

    expect(
      (wrapper.emitted("add")?.[0]?.[0] as { overridesDeclared: boolean } | undefined)
        ?.overridesDeclared,
    ).toBe(true);

    // Same single-reset-point guarantee as every other field
    // (`cancel emits no add and leaves nothing pinned when reopened`).
    await wrapper.setProps({ visible: false });
    await wrapper.setProps({ visible: true });
    expect(
      (
        wrapper.get('[data-testid="add-pair-rule-override-declared"] input')
          .element as HTMLInputElement
      ).checked,
    ).toBe(false);
  });

  it("omits a blank comment as null rather than an empty string", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper, "after", "c.mod");
    await pickMod(wrapper, "before", "d.mod");

    await wrapper.get('[data-testid="add-pair-rule-form"]').trigger("submit");

    const emitted = wrapper.emitted("add");
    expect(emitted).toBeDefined();
    expect((emitted?.[0]?.[0] as { comment: string | null } | undefined)?.comment).toBeNull();
  });

  it("rejects an invalid submit with only one side picked", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper, "after", "c.mod");

    await wrapper.get('[data-testid="add-pair-rule-form"]').trigger("submit");

    expect(wrapper.emitted("add")).toBeUndefined();
    expect(
      wrapper.get('[data-testid="add-pair-rule-submit"]').attributes("disabled"),
    ).toBeDefined();
  });

  it("rejects picking the same mod on both sides", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper, "after", "c.mod");
    // The before picker excludes whatever's already picked as `after`
    // only within its own instance's `members`, which is always empty —
    // the two pickers are independent, so `c.mod` is still offered here.
    await pickMod(wrapper, "before", "c.mod");

    expect(wrapper.get('[data-testid="add-pair-rule-same-mod-error"]').text()).toContain(
      "two different mods",
    );
    expect(
      wrapper.get('[data-testid="add-pair-rule-submit"]').attributes("disabled"),
    ).toBeDefined();

    await wrapper.get('[data-testid="add-pair-rule-form"]').trigger("submit");
    expect(wrapper.emitted("add")).toBeUndefined();
  });

  // The parent (`RulesPage.vue`) owns the actual `upsert_rule` mutation
  // and feeds its own `isLoading` back in as `pending` — mirrors
  // `AddPlacementRuleDialog.test.ts`'s identical guard.
  it("rejects a duplicate submit while the parent reports one already pending", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper, "after", "c.mod");
    await pickMod(wrapper, "before", "d.mod");

    await wrapper.get('[data-testid="add-pair-rule-form"]').trigger("submit");
    expect(wrapper.emitted("add")).toHaveLength(1);

    await wrapper.setProps({ pending: true });
    await wrapper.get('[data-testid="add-pair-rule-form"]').trigger("submit");

    expect(wrapper.emitted("add")).toHaveLength(1);
  });

  it("discloses a replace instead of blocking it when the picked pair already has a rule", async () => {
    const wrapper = await mountDialog({
      existingPairs: [
        {
          after: "c.mod",
          before: "d.mod",
          origin: "rimSortCommunity",
          comment: null,
          promotedFrom: null,
          alreadyPromoted: false,
          overridesDeclared: false,
        },
      ],
    });
    await pickMod(wrapper, "after", "c.mod");
    await pickMod(wrapper, "before", "d.mod");

    // The translated origin label, not the raw wire value — the dialog
    // renders `conflictingRule.origin` through `ruleOriginLabel`.
    expect(wrapper.get('[data-testid="add-pair-rule-replace-warning"]').text()).toContain(
      "RimSort community",
    );

    // Disclosure only — submitting still works, matching `upsert_rule`'s
    // own replace-not-refuse semantics.
    await wrapper.get('[data-testid="add-pair-rule-form"]').trigger("submit");
    expect(wrapper.emitted("add")).toHaveLength(1);
  });

  it("cancel emits no add and leaves nothing pinned when reopened", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper, "after", "c.mod");
    await pickMod(wrapper, "before", "d.mod");
    await wrapper.get('[data-testid="add-pair-rule-comment"]').setValue("draft note");

    await wrapper.get('[data-testid="add-pair-rule-cancel"]').trigger("click");

    expect(wrapper.emitted("add")).toBeUndefined();
    expect(wrapper.emitted("update:visible")).toEqual([[false]]);

    // Reopening (the parent flips `visible` back to `true`) starts from
    // a clean form — the cancelled picks/comment must not resurface.
    await wrapper.setProps({ visible: false });
    await wrapper.setProps({ visible: true });
    expect(
      (wrapper.get('[data-testid="add-pair-rule-comment"]').element as HTMLInputElement).value,
    ).toBe("");
    expect(
      wrapper.get('[data-testid="add-pair-rule-submit"]').attributes("disabled"),
    ).toBeDefined();
  });
});
