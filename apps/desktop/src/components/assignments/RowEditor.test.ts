import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import RowEditor from "@/components/assignments/RowEditor.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { AssignmentDetailDto } from "@/types/generated/AssignmentDetailDto";
import type { AssignmentRowDto } from "@/types/generated/AssignmentRowDto";
import type { ClearAssignmentRowRequestDto } from "@/types/generated/ClearAssignmentRowRequestDto";
import type { CoverageRowDto } from "@/types/generated/CoverageRowDto";
import type { ListItemsPageDto } from "@/types/generated/ListItemsPageDto";
import type { SetAssignmentRowRequestDto } from "@/types/generated/SetAssignmentRowRequestDto";

function assignment(overrides: Partial<AssignmentDetailDto> = {}): AssignmentDetailDto {
  return {
    id: "abc123def456",
    name: "Example race patch",
    packageId: "mypatch.parts",
    displayName: "Sample Part Patch",
    refs: [{ modId: "example.framework", name: "Example" }],
    excludedRefs: [],
    targets: [{ modId: "target.races", name: "Target Races" }],
    exportDir: null,
    updatedAt: "2026-09-06T00:00:00Z",
    author: "Rimmerge",
    description: "",
    folderName: "mypatch_parts",
    sections: [
      {
        defType: "example.PartAssignmentDef",
        schema: {
          defType: "example.PartAssignmentDef",
          refs: ["example.framework"],
          fields: {
            speciesNames: {
              role: { type: "targetKey", defType: "ThingDef" },
              cardinality: "list",
              observed: [5, 5],
              inferredRole: null,
            },
            parts: {
              role: { type: "itemSlot", defType: "example.PartDef" },
              cardinality: "list",
              observed: [5, 5],
              inferredRole: null,
            },
            chanceparts: {
              role: { type: "chances", forSlot: "parts" },
              cardinality: "list",
              observed: [5, 5],
              inferredRole: null,
            },
            enabled: {
              role: { type: "scalar", kind: { type: "bool" }, default: "true" },
              cardinality: "scalar",
              observed: [5, 5],
              inferredRole: null,
            },
            moveSpeed: {
              role: { type: "scalar", kind: { type: "number" }, default: "13" },
              cardinality: "scalar",
              observed: [5, 5],
              inferredRole: null,
            },
            critChance: {
              role: { type: "scalar", kind: { type: "number" }, default: "0.5" },
              cardinality: "scalar",
              observed: [5, 5],
              inferredRole: null,
            },
          },
          targetShapes: {},
        },
        isStandalone: false,
        rows: [],
        standaloneRows: [],
      },
    ],
    createdAt: "2026-09-06T00:00:00Z",
    ...overrides,
  };
}

function coverageRow(overrides: Partial<CoverageRowDto> = {}): CoverageRowDto {
  return {
    target: { keyField: "speciesNames", def: { defType: "ThingDef", defName: "Race0" } },
    owner: "target.races",
    matches: [],
    intent: "cover",
    winner: null,
    hasRow: false,
    ...overrides,
  };
}

function itemsPage(names: string[]): ListItemsPageDto {
  return {
    total: names.length,
    items: names.map((name) => ({
      def: { defType: "example.PartDef", defName: name },
      owner: "fixture.parts",
      hint: null,
      own: false,
    })),
  };
}

function mountEditor(
  props: InstanceType<typeof RowEditor>["$props"],
  fixtures: Record<string, unknown> = {},
) {
  installMockIpc({
    list_mod_names: {},
    resolve_def_graphic: { kind: "noGraphic" },
    ...fixtures,
  });
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/defs/:defRef", name: "def", component: { template: "<div />" } }],
  });
  return mount(RowEditor, {
    props,
    global: {
      plugins: [createPinia(), PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
}

describe("RowEditor", () => {
  afterEach(() => {
    clearMocks();
  });

  it("paginates the item picker, loading a second page on demand", async () => {
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow(),
        sectionType: "example.PartAssignmentDef",
      },
      {
        list_assignment_items: (payload: unknown) => {
          const request = payload as { request: { filter: { offset: number; limit: number } } };
          const { filter } = request.request;
          const all = ["Part0", "Part1", "Part2"];
          const page = all.slice(filter.offset, filter.offset + filter.limit);
          return {
            total: all.length,
            items: page.map((name) => ({
              def: { defType: "example.PartDef", defName: name },
              owner: "fixture.parts",
              hint: null,
              own: false,
            })),
          } satisfies ListItemsPageDto;
        },
      },
    );
    await flushPromises();

    const list = wrapper.get('[data-testid="item-picker-list-example.PartDef"]');
    expect(list.text()).toContain("Part0");
    expect(list.text()).toContain("Part1");
    expect(list.text()).toContain("Part2");
  });

  it("checking an item in the picker records it as a chosen name", async () => {
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow(),
        sectionType: "example.PartAssignmentDef",
      },
      { list_assignment_items: () => itemsPage(["Part0", "Part1"]) },
    );
    await flushPromises();

    await wrapper.get('[data-testid="item-picker-checkbox-example.PartDef-Part0"]').setValue(true);

    const checkbox = wrapper.get('[data-testid="item-picker-checkbox-example.PartDef-Part0"]');
    expect((checkbox.element as HTMLInputElement).checked).toBe(true);
  });

  it("shows the chance-length-mismatch message straight from set_assignment_row's own error", async () => {
    const calls: SetAssignmentRowRequestDto[] = [];
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow(),
        sectionType: "example.PartAssignmentDef",
      },
      {
        list_assignment_items: () => itemsPage([]),
        set_assignment_row: (payload: unknown) => {
          calls.push((payload as { request: SetAssignmentRowRequestDto }).request);
          throw {
            code: "invalid_input",
            message: "chanceparts: expected 0 chance values to match its slot, found 1",
          };
        },
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="row-editor-save-button"]').trigger("click");
    await flushPromises();

    expect(calls).toHaveLength(1);
    expect(wrapper.get('[data-testid="row-editor-error"]').text()).toContain(
      "expected 0 chance values to match its slot, found 1",
    );
  });

  it("renders a bool scalar field as a toggle and round-trips true/false as literal RowValue::Text", async () => {
    const calls: SetAssignmentRowRequestDto[] = [];
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow(),
        sectionType: "example.PartAssignmentDef",
      },
      {
        list_assignment_items: () => itemsPage([]),
        set_assignment_row: (payload: unknown) => {
          calls.push((payload as { request: SetAssignmentRowRequestDto }).request);
          return null;
        },
      },
    );
    await flushPromises();

    const toggle = wrapper.get('[data-testid="row-editor-scalar-enabled"] input');
    expect((toggle.element as HTMLInputElement).type).toBe("checkbox");
    // Untouched: defaults to the schema's own default ("true"), not yet written to the draft.
    expect((toggle.element as HTMLInputElement).checked).toBe(true);

    await toggle.setValue(false);
    await wrapper.get('[data-testid="row-editor-save-button"]').trigger("click");
    await flushPromises();

    expect(calls).toHaveLength(1);
    expect(calls[0]?.row.values["enabled"]).toEqual({ kind: "text", text: "false" });

    await toggle.setValue(true);
    await wrapper.get('[data-testid="row-editor-save-button"]').trigger("click");
    await flushPromises();

    expect(calls[1]?.row.values["enabled"]).toEqual({ kind: "text", text: "true" });
  });

  it("keeps a genuinely fractional stored value fractional even under an Int-looking default", async () => {
    const fractionalAssignment = assignment();
    const section = fractionalAssignment.sections[0];
    if (!section) {
      throw new Error("expected a section in the fixture");
    }
    section.rows = [
      {
        target: { keyField: "speciesNames", def: { defType: "ThingDef", defName: "Race0" } },
        row: {
          // moveSpeed's own schema default ("13") looks integer-like, but
          // this stored row's own value is genuinely fractional (from a
          // different source instance than the one the default was
          // inferred from) — `looksIntegerLike` alone would pin
          // `maxFractionDigits` to 0 and display a lying "0".
          values: { moveSpeed: { kind: "text", text: "0.35" } },
          defName: "sample_own_row",
          note: null,
        },
      },
    ];

    const wrapper = mountEditor(
      {
        assignment: fractionalAssignment,
        coverageRow: coverageRow({ hasRow: true }),
        sectionType: "example.PartAssignmentDef",
      },
      { list_assignment_items: () => itemsPage([]) },
    );
    await flushPromises();

    const input = wrapper.get('[data-testid="row-editor-scalar-moveSpeed"] input');
    expect((input.element as HTMLInputElement).value).toBe("0.35");
  });

  it("renders an Int-looking number field that never produces a fractional value", async () => {
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow(),
        sectionType: "example.PartAssignmentDef",
      },
      { list_assignment_items: () => itemsPage([]) },
    );
    await flushPromises();

    const input = wrapper.get('[data-testid="row-editor-scalar-moveSpeed"] input');
    await input.setValue("7.9");
    await input.trigger("blur");

    // maxFractionDigits is pinned to 0 for a whole-number-looking default
    // ("13"): the control itself never lands on a fractional value.
    expect(Number.isInteger(Number((input.element as HTMLInputElement).value))).toBe(true);
  });

  it("rejects non-numeric input in a number field, leaving the draft unset rather than forwarding it", async () => {
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow(),
        sectionType: "example.PartAssignmentDef",
      },
      { list_assignment_items: () => itemsPage([]) },
    );
    await flushPromises();

    const input = wrapper.get('[data-testid="row-editor-scalar-moveSpeed"] input');
    await input.setValue("fast");
    await input.trigger("blur");

    expect((input.element as HTMLInputElement).value).toBe("");
  });

  // NOTE: this test (and the chance-field one below) only has teeth on a
  // host whose default ICU locale doesn't already use `.` as the decimal
  // separator — Vitest here runs under Node's own default (en-US), so a
  // regression that dropped `locale="en-US"` from the `InputNumber` would
  // NOT be caught by this suite, only by a run on a machine whose OS
  // locale differs. Left
  // as a disclosed gap rather than stubbing the host locale, which would
  // make every other numeric assertion in this file locale-sensitive too.
  it("allows fractional values in a non-Int-looking number field", async () => {
    const calls: SetAssignmentRowRequestDto[] = [];
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow(),
        sectionType: "example.PartAssignmentDef",
      },
      {
        list_assignment_items: () => itemsPage([]),
        set_assignment_row: (payload: unknown) => {
          calls.push((payload as { request: SetAssignmentRowRequestDto }).request);
          return null;
        },
      },
    );
    await flushPromises();

    const input = wrapper.get('[data-testid="row-editor-scalar-critChance"] input');
    await input.setValue("0.35");
    await input.trigger("blur");
    await wrapper.get('[data-testid="row-editor-save-button"]').trigger("click");
    await flushPromises();

    expect(calls[0]?.row.values["critChance"]).toEqual({ kind: "text", text: "0.35" });
  });

  it("renders chance fields as InputNumber, writing 0 (not NaN/null) when a box is cleared", async () => {
    const calls: SetAssignmentRowRequestDto[] = [];
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow(),
        sectionType: "example.PartAssignmentDef",
      },
      {
        list_assignment_items: () => itemsPage(["Part0"]),
        set_assignment_row: (payload: unknown) => {
          calls.push((payload as { request: SetAssignmentRowRequestDto }).request);
          return null;
        },
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="item-picker-checkbox-example.PartDef-Part0"]').setValue(true);
    await flushPromises();

    const chanceInput = wrapper.get('[data-testid="row-editor-chance-chanceparts-0"] input');
    await chanceInput.setValue("0.35");
    await chanceInput.trigger("blur");
    await wrapper.get('[data-testid="row-editor-save-button"]').trigger("click");
    await flushPromises();

    expect(calls[0]?.row.values["chanceparts"]).toEqual({ kind: "numbers", numbers: [0.35] });

    // The control is an `InputNumber`, not an `InputText` feeding
    // `Number(text)` straight into the payload: that would turn a cleared
    // box into `Number("") === 0` (an indistinguishable, silent real chance)
    // and a non-numeric or locale-comma keystroke into `NaN`, which
    // serialises to `null` and fails serde with a generic "Failed to save
    // this row." `InputNumber` can't accept that keystroke, so the only
    // reachable "cleared" outcome is an explicit `0`.
    // "cleared" outcome is an explicit `0`.
    await chanceInput.setValue("");
    await chanceInput.trigger("blur");
    await wrapper.get('[data-testid="row-editor-save-button"]').trigger("click");
    await flushPromises();

    expect(calls[1]?.row.values["chanceparts"]).toEqual({ kind: "numbers", numbers: [0] });
  });

  it("auto-prefills an uncovered target from the winning existing instance, labeling each field with the source mod", async () => {
    const copyCalls: unknown[] = [];
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow({
          matches: [
            {
              owner: "example.framework",
              instanceDefName: "Group_Example",
              keyField: "speciesNames",
            },
          ],
          winner: {
            kind: "existing",
            match: {
              owner: "example.framework",
              instanceDefName: "Group_Example",
              keyField: "speciesNames",
            },
          },
        }),
        sectionType: "example.PartAssignmentDef",
      },
      {
        list_assignment_items: () => itemsPage([]),
        copy_assignment_row_from: (payload: unknown) => {
          copyCalls.push(payload);
          return {
            row: {
              values: {
                parts: { kind: "names", names: ["Part0"] },
                enabled: { kind: "text", text: "true" },
              },
              defName: "Group_Example_copy",
              note: null,
            },
            dropped: [],
          };
        },
      },
    );
    await flushPromises();

    expect(copyCalls).toHaveLength(1);
    expect(copyCalls[0]).toMatchObject({ request: { sourceDefName: "Group_Example" } });

    expect(wrapper.get('[data-testid="row-editor-def-name-input"]').element).toHaveProperty(
      "value",
      "Group_Example_copy",
    );
    const label = wrapper.get('[data-testid="row-editor-prefill-enabled"]');
    expect(label.text()).toContain("example.framework");
    expect(wrapper.get('[data-testid="row-editor-prefill-parts"]').text()).toContain(
      "example.framework",
    );

    // Editing the field un-labels it: it's authored now, not borrowed.
    await wrapper.get('[data-testid="row-editor-scalar-enabled"] input').setValue(false);
    expect(wrapper.find('[data-testid="row-editor-prefill-enabled"]').exists()).toBe(false);
    // A field the user hasn't touched keeps its label.
    expect(wrapper.find('[data-testid="row-editor-prefill-parts"]').exists()).toBe(true);
  });

  it("surfaces item-slot values a copy-from call dropped because the def isn't active", async () => {
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow({
          matches: [
            {
              owner: "example.framework",
              instanceDefName: "Group_Example",
              keyField: "speciesNames",
            },
          ],
        }),
        sectionType: "example.PartAssignmentDef",
      },
      {
        list_assignment_items: () => itemsPage(["Part0"]),
        copy_assignment_row_from: () => ({
          row: {
            values: { parts: { kind: "names", names: ["Part0"] } },
            defName: "Group_Example_copy",
            note: null,
          },
          dropped: [{ path: "parts", defType: "example.PartDef", name: "PartInactive" }],
        }),
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="row-editor-copy-from-Group_Example"]').trigger("click");
    await flushPromises();

    const disclosure = wrapper.get('[data-testid="row-editor-copy-dropped-parts"]');
    expect(disclosure.text()).toContain("1 borrowed value");
    expect(disclosure.text()).toContain("PartInactive");

    // Editing the field clears the disclosure — the value is gone either
    // way, but a fresh pick isn't the borrowed value being complained
    // about any more.
    await wrapper.get('[data-testid="item-picker-checkbox-example.PartDef-Part0"]').setValue(false);
    expect(wrapper.find('[data-testid="row-editor-copy-dropped-parts"]').exists()).toBe(false);
  });

  it("never auto-prefills a target this project already has its own row for", async () => {
    const copyCalls: unknown[] = [];
    const existingAssignment = assignment();
    const section = existingAssignment.sections[0];
    if (!section) {
      throw new Error("expected a section in the fixture");
    }
    section.rows = [
      {
        target: { keyField: "speciesNames", def: { defType: "ThingDef", defName: "Race0" } },
        row: {
          values: { enabled: { kind: "text", text: "false" } },
          defName: "sample_own_row",
          note: null,
        },
      },
    ];

    const wrapper = mountEditor(
      {
        assignment: existingAssignment,
        coverageRow: coverageRow({
          hasRow: true,
          intent: "override",
          matches: [
            {
              owner: "example.framework",
              instanceDefName: "Group_Example",
              keyField: "speciesNames",
            },
          ],
          winner: {
            kind: "existing",
            match: {
              owner: "example.framework",
              instanceDefName: "Group_Example",
              keyField: "speciesNames",
            },
          },
        }),
        sectionType: "example.PartAssignmentDef",
      },
      {
        list_assignment_items: () => itemsPage([]),
        copy_assignment_row_from: (payload: unknown) => {
          copyCalls.push(payload);
          throw new Error("must not be called for an already-authored row");
        },
      },
    );
    await flushPromises();

    expect(copyCalls).toHaveLength(0);
    expect(wrapper.get('[data-testid="row-editor-def-name-input"]').element).toHaveProperty(
      "value",
      "sample_own_row",
    );
    expect(wrapper.find('[data-testid="row-editor-prefill-enabled"]').exists()).toBe(false);
    // The winner is still discoverable as an explicit action, marked distinctly.
    expect(wrapper.get('[data-testid="row-editor-copy-from-winner-badge"]').text()).toContain(
      "winner",
    );
  });

  it("shows the override badge and existing matches when the target is already covered", () => {
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow({
          intent: "override",
          matches: [
            { owner: "other.mod", instanceDefName: "Group_Other", keyField: "speciesNames" },
          ],
        }),
        sectionType: "example.PartAssignmentDef",
      },
      { list_assignment_items: () => itemsPage([]) },
    );

    expect(wrapper.get('[data-testid="row-editor-override-badge"]').text()).toContain(
      "1 existing match",
    );
    expect(wrapper.get('[data-testid="row-editor-copy-from-Group_Other"]').text()).toContain(
      "other.mod",
    );
  });

  it("a coverage refetch that hands back a new-but-equal target object never discards the in-progress draft", async () => {
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow(),
        sectionType: "example.PartAssignmentDef",
      },
      { list_assignment_items: () => itemsPage([]) },
    );
    await flushPromises();

    await wrapper.get('[data-testid="row-editor-def-name-input"]').setValue("UserTypedThis");

    // Simulates a Pinia Colada refetch: `AssignmentEditorPage.vue`'s
    // `selectedRow` computed hands down a brand-new `CoverageRowDto`
    // object — deep-equal, same target, no structural sharing — every
    // time the coverage query refetches (`staleTime`, window focus, or
    // any mutation invalidating it). Watching `() =>
    // coverageRow.target` by reference would fire `resetDraft` right here.
    await wrapper.setProps({ coverageRow: coverageRow() });
    await flushPromises();

    expect(wrapper.get('[data-testid="row-editor-def-name-input"]').element).toHaveProperty(
      "value",
      "UserTypedThis",
    );
  });

  it("a user edit started before a slower auto-prefill resolves wins — the prefill is discarded", async () => {
    const deferredCopy: {
      resolve: ((result: { row: AssignmentRowDto; dropped: unknown[] }) => void) | null;
    } = { resolve: null };
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow({
          matches: [
            {
              owner: "example.framework",
              instanceDefName: "Group_Example",
              keyField: "speciesNames",
            },
          ],
          winner: {
            kind: "existing",
            match: {
              owner: "example.framework",
              instanceDefName: "Group_Example",
              keyField: "speciesNames",
            },
          },
        }),
        sectionType: "example.PartAssignmentDef",
      },
      {
        list_assignment_items: () => itemsPage([]),
        // Never resolves on its own — models a real install's XML read
        // (`spawn_blocking`, genuinely hundreds of ms) outliving a user's
        // own keystrokes.
        copy_assignment_row_from: () =>
          new Promise<{ row: AssignmentRowDto; dropped: unknown[] }>((resolve) => {
            deferredCopy.resolve = resolve;
          }),
      },
    );
    await flushPromises();

    // The user starts typing their own name before the auto-prefill's
    // fetch resolves.
    await wrapper
      .get('[data-testid="row-editor-def-name-input"]')
      .setValue("MyOwnCarefullyChosenName");

    deferredCopy.resolve?.({
      row: {
        values: { enabled: { kind: "text", text: "true" } },
        defName: "Group_Example_copy",
        note: null,
      },
      dropped: [],
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="row-editor-def-name-input"]').element).toHaveProperty(
      "value",
      "MyOwnCarefullyChosenName",
    );
    expect(wrapper.find('[data-testid="row-editor-prefill-enabled"]').exists()).toBe(false);
  });

  it("clamps a stored two-name scalar item slot to its first name and surfaces what was dropped", async () => {
    const scalarAssignment = assignment();
    const section = scalarAssignment.sections[0];
    if (!section) {
      throw new Error("expected a section in the fixture");
    }
    const partsField = section.schema.fields["parts"];
    if (!partsField) {
      throw new Error("expected a parts field in the fixture");
    }
    partsField.cardinality = "scalar";
    section.rows = [
      {
        target: { keyField: "speciesNames", def: { defType: "ThingDef", defName: "Race0" } },
        row: {
          // A row from before this constraint existed (or written by an
          // older client/the CLI): two names in a field the schema now
          // says is `Cardinality::Scalar`.
          values: { parts: { kind: "names", names: ["Part0", "Part1"] } },
          defName: "sample_own_row",
          note: null,
        },
      },
    ];

    const wrapper = mountEditor(
      {
        assignment: scalarAssignment,
        coverageRow: coverageRow({ hasRow: true }),
        sectionType: "example.PartAssignmentDef",
      },
      { list_assignment_items: () => itemsPage(["Part0", "Part1"]) },
    );
    await flushPromises();

    const checkedRadios = wrapper
      .findAll('[data-testid^="item-picker-checkbox-example.PartDef-"]')
      .filter((input) => (input.element as HTMLInputElement).checked);
    expect(checkedRadios).toHaveLength(1);

    const clamped = wrapper.get('[data-testid="row-editor-clamped-parts"]');
    expect(clamped.text()).toContain("Part1");
  });

  it("'Clear selection' on a scalar item slot omits the field entirely, not an empty names array", async () => {
    const calls: SetAssignmentRowRequestDto[] = [];
    const scalarAssignment = assignment();
    const section = scalarAssignment.sections[0];
    if (!section) {
      throw new Error("expected a section in the fixture");
    }
    const partsField = section.schema.fields["parts"];
    if (!partsField) {
      throw new Error("expected a parts field in the fixture");
    }
    partsField.cardinality = "scalar";

    const wrapper = mountEditor(
      {
        assignment: scalarAssignment,
        coverageRow: coverageRow(),
        sectionType: "example.PartAssignmentDef",
      },
      {
        list_assignment_items: () => itemsPage(["Part0"]),
        set_assignment_row: (payload: unknown) => {
          calls.push((payload as { request: SetAssignmentRowRequestDto }).request);
          return null;
        },
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="item-picker-checkbox-example.PartDef-Part0"]').setValue(true);
    await wrapper.get('[data-testid="item-picker-clear-example.PartDef"]').trigger("click");
    await wrapper.get('[data-testid="row-editor-save-button"]').trigger("click");
    await flushPromises();

    expect(calls).toHaveLength(1);
    expect(calls[0]?.row.values["parts"]).toBeUndefined();
  });

  // -- Free-standing rows ----------------------------------------------

  /** A section flipped `isStandalone` (its own `speciesNames` target-key field dropped, since a real free-standing section's schema never has one) — same item-slot/chance/scalar fields otherwise, so every existing control assertion above applies unchanged to this mode too. */
  function standaloneAssignment(standaloneRows: AssignmentRowDto[] = []): AssignmentDetailDto {
    const base = assignment();
    const section = base.sections[0];
    if (!section) {
      throw new Error("expected a section in the fixture");
    }
    const { speciesNames: _raceNames, ...rest } = section.schema.fields;
    return {
      ...base,
      sections: [
        {
          ...section,
          schema: { ...section.schema, fields: rest },
          isStandalone: true,
          rows: [],
          standaloneRows: standaloneRows.map((row) => ({ row })),
        },
      ],
    };
  }

  it("creates a brand-new free-standing row addressed by its own typed defName", async () => {
    const calls: SetAssignmentRowRequestDto[] = [];
    const wrapper = mountEditor(
      {
        assignment: standaloneAssignment(),
        sectionType: "example.PartAssignmentDef",
        ownDefName: null,
      },
      {
        list_assignment_items: () => itemsPage([]),
        set_assignment_row: (payload: unknown) => {
          calls.push((payload as { request: SetAssignmentRowRequestDto }).request);
          return null;
        },
      },
    );
    await flushPromises();

    expect(wrapper.get('[data-testid="row-editor"]').text()).toContain("New row");
    // No coverage queue for a free-standing row — neither of these
    // target-only sections has anything to render.
    expect(wrapper.find('[data-testid="row-editor-override-badge"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="row-editor-copy-from"]').exists()).toBe(false);

    await wrapper
      .get('[data-testid="row-editor-def-name-input"]')
      .setValue("SamplePartDef_NewRace");
    await wrapper.get('[data-testid="row-editor-save-button"]').trigger("click");
    await flushPromises();

    expect(calls).toHaveLength(1);
    expect(calls[0]?.target).toBeNull();
    expect(calls[0]?.row.defName).toBe("SamplePartDef_NewRace");
    expect(wrapper.emitted("ownRowSaved")).toEqual([["SamplePartDef_NewRace"]]);
  });

  it("loads an existing free-standing row by its own defName and clears it", async () => {
    const clearCalls: ClearAssignmentRowRequestDto[] = [];
    const existing: AssignmentRowDto = {
      values: { enabled: { kind: "text", text: "true" } },
      defName: "ExistingOwnRow",
      note: "hand-authored",
    };
    const wrapper = mountEditor(
      {
        assignment: standaloneAssignment([existing]),
        sectionType: "example.PartAssignmentDef",
        ownDefName: "ExistingOwnRow",
      },
      {
        list_assignment_items: () => itemsPage([]),
        clear_assignment_row: (payload: unknown) => {
          clearCalls.push((payload as { request: ClearAssignmentRowRequestDto }).request);
          return null;
        },
      },
    );
    await flushPromises();

    expect(wrapper.get('[data-testid="row-editor-def-name-input"]').element).toHaveProperty(
      "value",
      "ExistingOwnRow",
    );
    const toggle = wrapper.get('[data-testid="row-editor-scalar-enabled"] input');
    expect((toggle.element as HTMLInputElement).checked).toBe(true);

    await wrapper.get('[data-testid="row-editor-clear-button"]').trigger("click");
    await flushPromises();

    expect(clearCalls).toEqual([
      {
        assignmentId: "abc123def456",
        section: "example.PartAssignmentDef",
        target: null,
        defName: "ExistingOwnRow",
      },
    ]);
    expect(wrapper.emitted("ownRowCleared")).toHaveLength(1);
  });

  it("shows the texture viewer for a target row, asking for that target's def and owner", async () => {
    const graphicCalls: unknown[] = [];
    const wrapper = mountEditor(
      {
        assignment: assignment(),
        coverageRow: coverageRow(),
        sectionType: "example.PartAssignmentDef",
      },
      {
        list_assignment_items: () => itemsPage([]),
        resolve_def_graphic: (payload: unknown) => {
          graphicCalls.push(payload);
          return { kind: "noGraphic" };
        },
      },
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="def-graphic-viewer"]').exists()).toBe(true);
    expect(graphicCalls).toEqual([{ request: { defRef: "ThingDef/Race0" } }]);
  });

  it("shows no texture viewer for a free-standing row, which has no def on disk", async () => {
    const graphicCalls: unknown[] = [];
    const wrapper = mountEditor(
      {
        assignment: standaloneAssignment([]),
        sectionType: "example.PartAssignmentDef",
        ownDefName: null,
      },
      {
        list_assignment_items: () => itemsPage([]),
        resolve_def_graphic: (payload: unknown) => {
          graphicCalls.push(payload);
          return { kind: "noGraphic" };
        },
      },
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="def-graphic-viewer"]').exists()).toBe(false);
    expect(graphicCalls).toEqual([]);
  });
});
