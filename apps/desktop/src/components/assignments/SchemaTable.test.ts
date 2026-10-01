import Aura from "@primevue/themes/aura";
import { mount } from "@vue/test-utils";
import PrimeVue from "primevue/config";
import Select from "primevue/select";
import { describe, expect, it } from "vitest";

import SchemaTable from "@/components/assignments/SchemaTable.vue";
import type { AssignmentSchemaDto } from "@/types/generated/AssignmentSchemaDto";

function schema(overrides: Partial<AssignmentSchemaDto> = {}): AssignmentSchemaDto {
  return {
    defType: "example.PartAssignmentDef",
    refs: ["example.framework"],
    fields: {
      speciesNames: {
        role: { type: "opaque" },
        cardinality: "list",
        observed: [5, 5],
        inferredRole: null,
      },
    },
    targetShapes: {},
    ...overrides,
  };
}

function mountTable(props: InstanceType<typeof SchemaTable>["$props"]) {
  return mount(SchemaTable, {
    props,
    global: { plugins: [[PrimeVue, { theme: { preset: Aura } }]] },
  });
}

describe("SchemaTable", () => {
  it("reclassifying a field's role round-trips through update:schema, remembering the original inferred role", async () => {
    const wrapper = mountTable({ schema: schema() });
    const row = wrapper.get('[data-testid="schema-row-speciesNames"]');
    const select = row.findComponent(Select);

    await select.vm.$emit("update:modelValue", "targetKey");

    const emitted = wrapper.emitted("update:schema");
    expect(emitted).toHaveLength(1);
    const updated = emitted?.[0]?.[0] as AssignmentSchemaDto;
    expect(updated.fields["speciesNames"]?.role).toEqual({ type: "targetKey", defType: "" });
    expect(updated.fields["speciesNames"]?.inferredRole).toEqual({ type: "opaque" });

    // Reclassifying a second time keeps the *original* inferred role
    // (`opaque`), never overwriting it with the intermediate `targetKey`.
    await wrapper.setProps({ schema: updated });
    const secondSelect = wrapper
      .get('[data-testid="schema-row-speciesNames"]')
      .findComponent(Select);
    await secondSelect.vm.$emit("update:modelValue", "itemSlot");
    const secondUpdate = wrapper.emitted("update:schema")?.[1]?.[0] as AssignmentSchemaDto;
    expect(secondUpdate.fields["speciesNames"]?.inferredRole).toEqual({ type: "opaque" });
  });

  it("reclassifying to the same role kind is a no-op on inferredRole (mirrors confirm_role's role != spec.role guard)", async () => {
    const wrapper = mountTable({ schema: schema() }); // starts `opaque`
    const select = wrapper.get('[data-testid="schema-row-speciesNames"]').findComponent(Select);

    await select.vm.$emit("update:modelValue", "opaque");

    const updated = wrapper.emitted("update:schema")?.[0]?.[0] as AssignmentSchemaDto;
    expect(updated.fields["speciesNames"]?.role).toEqual({ type: "opaque" });
    expect(updated.fields["speciesNames"]?.inferredRole).toBeNull();
  });

  it("shows a caveat per unreadable target", () => {
    const wrapper = mountTable({
      schema: schema(),
      unreadableTargets: [
        { field: "speciesNames", defType: "ThingDef", defName: "Race0", reason: "stale file" },
      ],
    });

    const caveats = wrapper.get('[data-testid="schema-unreadable-targets"]');
    expect(caveats.text()).toContain("Race0");
    expect(caveats.text()).toContain("stale file");
  });

  it("shows no caveat panel when nothing is unreadable", () => {
    const wrapper = mountTable({ schema: schema() });

    expect(wrapper.find('[data-testid="schema-unreadable-targets"]').exists()).toBe(false);
  });

  it("adding a field rejects a path the schema already has", async () => {
    const wrapper = mountTable({ schema: schema() });

    await wrapper.get('[data-testid="schema-new-field-path"]').setValue("speciesNames");
    await wrapper.get('[data-testid="schema-add-field-button"]').trigger("click");

    expect(wrapper.get('[data-testid="schema-add-field-error"]').text()).toContain(
      "already a field",
    );
    expect(wrapper.emitted("update:schema")).toBeUndefined();
  });

  it("adding a new scalar field emits update:schema with an unobserved field", async () => {
    const wrapper = mountTable({ schema: schema() });

    await wrapper.get('[data-testid="schema-new-field-path"]').setValue("customField");
    await wrapper.get('[data-testid="schema-add-field-button"]').trigger("click");

    const updated = wrapper.emitted("update:schema")?.[0]?.[0] as AssignmentSchemaDto;
    expect(updated.fields["customField"]).toEqual({
      role: { type: "scalar", kind: { type: "text" }, default: null },
      cardinality: "scalar",
      observed: [0, 0],
      inferredRole: null,
    });
  });

  it("hiding a field removes it from the emitted schema", async () => {
    const wrapper = mountTable({ schema: schema() });

    await wrapper.get('[data-testid="schema-remove-speciesNames"]').trigger("click");

    const updated = wrapper.emitted("update:schema")?.[0]?.[0] as AssignmentSchemaDto;
    expect(updated.fields["speciesNames"]).toBeUndefined();
  });
});
