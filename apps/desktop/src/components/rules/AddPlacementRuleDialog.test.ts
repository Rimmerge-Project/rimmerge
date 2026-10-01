import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import Select from "primevue/select";
import { afterEach, describe, expect, it } from "vitest";

import AddPlacementRuleDialog from "@/components/rules/AddPlacementRuleDialog.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ModSummaryDto } from "@/types/generated/ModSummaryDto";
import type { PlacementRuleDto } from "@/types/generated/PlacementRuleDto";

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

async function mountDialog(
  props: { pending?: boolean; existingPlacements?: PlacementRuleDto[] } = {},
) {
  installMockIpc({ list_mods: { total: 1, items: [mod()] } });
  const wrapper = mount(AddPlacementRuleDialog, {
    props: {
      visible: true,
      pending: props.pending ?? false,
      existingPlacements: props.existingPlacements ?? [],
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

/** Picks `c.mod` through the reused `ModPicker`'s own search-and-click flow. */
async function pickMod(wrapper: Awaited<ReturnType<typeof mountDialog>>): Promise<void> {
  await wrapper.get('[data-testid="add-rule-mod-search"]').setValue("mod");
  await flush();
  await wrapper.vm.$nextTick();
  await wrapper.get('[data-testid="add-rule-mod-add-c.mod"]').trigger("click");
}

describe("AddPlacementRuleDialog", () => {
  afterEach(() => {
    clearMocks();
  });

  it("sends the exact expected rule on a valid submit", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper);
    await wrapper.findComponent(Select).vm.$emit("update:modelValue", "bottom");
    await wrapper.get('[data-testid="add-rule-comment"]').setValue("keep it last");

    await wrapper.get('[data-testid="add-rule-form"]').trigger("submit");

    expect(wrapper.emitted("add")).toEqual([
      [
        {
          kind: "placement",
          modId: "c.mod",
          placement: "bottom",
          origin: "userDecision",
          comment: "keep it last",
          promotedFrom: null,
          alreadyPromoted: false,
        },
      ],
    ]);
  });

  it("omits a blank comment as null rather than an empty string", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper);
    await wrapper.findComponent(Select).vm.$emit("update:modelValue", "top");

    await wrapper.get('[data-testid="add-rule-form"]').trigger("submit");

    const emitted = wrapper.emitted("add");
    expect(emitted).toBeDefined();
    expect((emitted?.[0]?.[0] as { comment: string | null } | undefined)?.comment).toBeNull();
  });

  it("rejects an invalid submit with no mod selected", async () => {
    const wrapper = await mountDialog();
    await wrapper.findComponent(Select).vm.$emit("update:modelValue", "bottom");

    await wrapper.get('[data-testid="add-rule-form"]').trigger("submit");

    expect(wrapper.emitted("add")).toBeUndefined();
    expect(wrapper.get('[data-testid="add-rule-submit"]').attributes("disabled")).toBeDefined();
  });

  it("rejects an invalid submit with no placement chosen", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper);

    await wrapper.get('[data-testid="add-rule-form"]').trigger("submit");

    expect(wrapper.emitted("add")).toBeUndefined();
  });

  // The parent (`RulesPage.vue`) owns the actual `upsert_rule` mutation
  // and feeds its own `isLoading` back in as `pending` — this pins the
  // guard that keeps a second click (or a resubmit before the first
  // request settles) from ever firing a duplicate `add` for the same
  // pick, without needing to fake a real async race.
  it("rejects a duplicate submit while the parent reports one already pending", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper);
    await wrapper.findComponent(Select).vm.$emit("update:modelValue", "bottom");

    await wrapper.get('[data-testid="add-rule-form"]').trigger("submit");
    expect(wrapper.emitted("add")).toHaveLength(1);

    await wrapper.setProps({ pending: true });
    await wrapper.get('[data-testid="add-rule-form"]').trigger("submit");

    expect(wrapper.emitted("add")).toHaveLength(1);
  });

  it("discloses a replace instead of blocking it when the picked mod already has a rule", async () => {
    const wrapper = await mountDialog({
      existingPlacements: [
        {
          modId: "c.mod",
          placement: "top",
          origin: "rimSortCommunity",
          comment: null,
          promotedFrom: null,
          alreadyPromoted: false,
        },
      ],
    });
    await pickMod(wrapper);
    await wrapper.findComponent(Select).vm.$emit("update:modelValue", "bottom");

    // The translated origin label, not the raw wire value — the dialog
    // renders `conflictingRule.origin` through `ruleOriginLabel`.
    expect(wrapper.get('[data-testid="add-rule-replace-warning"]').text()).toContain(
      "RimSort community",
    );

    // Disclosure only — submitting still works, matching `upsert_rule`'s
    // own replace-not-refuse semantics (see the component's own doc
    // comment on why this must not diverge from `rule set-placement`).
    await wrapper.get('[data-testid="add-rule-form"]').trigger("submit");
    expect(wrapper.emitted("add")).toHaveLength(1);
  });

  it("cancel emits no add and leaves nothing pinned when reopened", async () => {
    const wrapper = await mountDialog();
    await pickMod(wrapper);
    await wrapper.findComponent(Select).vm.$emit("update:modelValue", "bottom");
    await wrapper.get('[data-testid="add-rule-comment"]').setValue("draft note");

    await wrapper.get('[data-testid="add-rule-cancel"]').trigger("click");

    expect(wrapper.emitted("add")).toBeUndefined();
    expect(wrapper.emitted("update:visible")).toEqual([[false]]);

    // Reopening (the parent flips `visible` back to `true`) starts from
    // a clean form — the cancelled pick/comment must not resurface.
    await wrapper.setProps({ visible: false });
    await wrapper.setProps({ visible: true });
    expect(
      (wrapper.get('[data-testid="add-rule-comment"]').element as HTMLInputElement).value,
    ).toBe("");
    expect(wrapper.get('[data-testid="add-rule-submit"]').attributes("disabled")).toBeDefined();
  });
});
