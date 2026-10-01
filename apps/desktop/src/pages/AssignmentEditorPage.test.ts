import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import RadioButton from "primevue/radiobutton";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import AssignmentEditorPage from "@/pages/AssignmentEditorPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { AssignmentDetailDto } from "@/types/generated/AssignmentDetailDto";
import type { AssignmentSectionDto } from "@/types/generated/AssignmentSectionDto";
import type { CoverageDto } from "@/types/generated/CoverageDto";
import type { ListAssignmentCandidatesResponseDto } from "@/types/generated/ListAssignmentCandidatesResponseDto";
import type { RemoveAssignmentSectionRequestDto } from "@/types/generated/RemoveAssignmentSectionRequestDto";

const ASSIGNMENT_ID = "abc123def456";

function raceGroupSection(): AssignmentSectionDto {
  return {
    defType: "example.PartAssignmentDef",
    schema: {
      defType: "example.PartAssignmentDef",
      refs: ["example.framework"],
      fields: {
        speciesNames: {
          role: { type: "targetKey", defType: "ThingDef" },
          cardinality: "list",
          observed: [2, 2],
          inferredRole: null,
        },
        parts: {
          role: { type: "itemSlot", defType: "example.PartDef" },
          cardinality: "list",
          observed: [2, 2],
          inferredRole: null,
        },
      },
      targetShapes: {},
    },
    isStandalone: false,
    rows: [],
    standaloneRows: [],
  };
}

function partSection(): AssignmentSectionDto {
  return {
    defType: "example.PartDef",
    schema: {
      defType: "example.PartDef",
      refs: ["fixture.parts"],
      fields: {
        effect: {
          role: { type: "scalar", kind: { type: "text" }, default: null },
          cardinality: "scalar",
          observed: [1, 1],
          inferredRole: null,
        },
      },
      targetShapes: {},
    },
    isStandalone: true,
    rows: [],
    standaloneRows: [{ row: { values: {}, defName: "mypatch_parts_Own0", note: "seed" } }],
  };
}

function assignment(overrides: Partial<AssignmentDetailDto> = {}): AssignmentDetailDto {
  return {
    id: ASSIGNMENT_ID,
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
    sections: [raceGroupSection(), partSection()],
    createdAt: "2026-09-06T00:00:00Z",
    ...overrides,
  };
}

function coverage(): CoverageDto {
  return {
    applicable: true,
    rows: [
      {
        target: { keyField: "speciesNames", def: { defType: "ThingDef", defName: "Race0" } },
        owner: "target.races",
        matches: [],
        intent: "cover",
        winner: null,
        hasRow: false,
      },
      {
        target: { keyField: "speciesNames", def: { defType: "ThingDef", defName: "Race1" } },
        owner: "target.races",
        matches: [],
        intent: "cover",
        winner: null,
        hasRow: false,
      },
    ],
  };
}

async function mountEditorPage(
  detail: AssignmentDetailDto,
  extraFixtures: Record<string, unknown> = {},
) {
  installMockIpc({
    list_mod_names: {},
    get_pending_active_changes: {
      unscanned: { added: [], removed: [] },
      unapplied: { added: [], removed: [] },
    },
    get_assignment: () => detail,
    get_assignment_coverage: () => coverage(),
    list_assignment_items: () => ({ total: 0, items: [] }),
    delete_assignment: () => null,
    resolve_def_graphic: { kind: "noGraphic" },
    ...extraFixtures,
  });

  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/assignments", name: "assignments", component: { template: "<div />" } },
      {
        path: "/assignments/:assignmentId",
        name: "assignment-detail",
        component: { template: "<div />" },
      },
      { path: "/defs/:defRef", name: "def", component: { template: "<div />" } },
    ],
  });
  await router.push({ name: "assignment-detail", params: { assignmentId: ASSIGNMENT_ID } });

  const wrapper = mount(AssignmentEditorPage, {
    attachTo: document.body,
    global: {
      plugins: [createPinia(), PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }]],
      // Dialog content is teleported (to `body` by default) — stubbing
      // `teleport` keeps it in place in the render tree so `wrapper.get`/
      // `.find` can reach it, mirroring `RulesPage.test.ts`'s own need
      // for the same reason (a `Dialog` this page also has to interact
      // with in its tests).
      stubs: { teleport: true },
    },
  });
  await flushPromises();
  await wrapper.vm.$nextTick();
  return wrapper;
}

describe("AssignmentEditorPage", () => {
  afterEach(() => {
    clearMocks();
    document.body.innerHTML = "";
  });

  it("renders one tab per section, in the project's own order", async () => {
    const wrapper = await mountEditorPage(assignment());

    expect(wrapper.get('[data-testid="section-tab-example.PartAssignmentDef"]').text()).toBe(
      "example.PartAssignmentDef",
    );
    expect(wrapper.get('[data-testid="section-tab-example.PartDef"]').text()).toBe(
      "example.PartDef",
    );
  });

  it("a target-keyed section shows the coverage queue and row editor", async () => {
    const wrapper = await mountEditorPage(assignment());

    expect(wrapper.find('[data-testid="coverage-list"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="row-editor"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="standalone-section-rows"]').exists()).toBe(false);
  });

  it("switching to a free-standing section's tab lists its own rows and has no coverage queue", async () => {
    const wrapper = await mountEditorPage(assignment());

    await wrapper.get('[data-testid="section-tab-example.PartDef"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="standalone-section-rows"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="standalone-row-mypatch_parts_Own0"]').text()).toContain(
      "mypatch_parts_Own0",
    );
    expect(wrapper.find('[data-testid="coverage-list"]').exists()).toBe(false);
  });

  it("selecting an existing free-standing row opens it in the row editor", async () => {
    const wrapper = await mountEditorPage(assignment());
    await wrapper.get('[data-testid="section-tab-example.PartDef"]').trigger("click");
    await flushPromises();

    await wrapper.get('[data-testid="standalone-row-mypatch_parts_Own0"]').trigger("click");
    await flushPromises();

    const editor = wrapper.get('[data-testid="row-editor"]');
    expect(editor.text()).toContain("mypatch_parts_Own0");
    expect(wrapper.get('[data-testid="row-editor-def-name-input"]').element).toHaveProperty(
      "value",
      "mypatch_parts_Own0",
    );
  });

  it("'New row' opens a blank row editor for a free-standing section, with no coverage-only sections showing", async () => {
    const wrapper = await mountEditorPage(assignment());
    await wrapper.get('[data-testid="section-tab-example.PartDef"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="row-editor"]').exists()).toBe(false);

    await wrapper.get('[data-testid="standalone-new-row-button"]').trigger("click");
    await flushPromises();

    const editor = wrapper.get('[data-testid="row-editor"]');
    expect(editor.text()).toContain("New row");
    expect(editor.find('[data-testid="row-editor-override-badge"]').exists()).toBe(false);
    expect(editor.find('[data-testid="row-editor-copy-from"]').exists()).toBe(false);
  });

  it("creating a new free-standing row records set_assignment_row with target null, and selects the saved row", async () => {
    const savedRow = { values: {}, defName: "mypatch_parts_Own1", note: null };
    const afterSave = assignment({
      sections: [
        raceGroupSection(),
        {
          ...partSection(),
          standaloneRows: [...partSection().standaloneRows, { row: savedRow }],
        },
      ],
    });
    let current = assignment();
    const setCalls: unknown[] = [];
    const wrapper = await mountEditorPage(current, {
      get_assignment: () => current,
      set_assignment_row: (payload: unknown) => {
        setCalls.push((payload as { request: unknown }).request);
        current = afterSave;
        return null;
      },
    });
    await wrapper.get('[data-testid="section-tab-example.PartDef"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-testid="standalone-new-row-button"]').trigger("click");
    await flushPromises();

    await wrapper.get('[data-testid="row-editor-def-name-input"]').setValue("mypatch_parts_Own1");
    await wrapper.get('[data-testid="row-editor-save-button"]').trigger("click");
    await flushPromises();

    expect(setCalls).toEqual([
      {
        assignmentId: ASSIGNMENT_ID,
        section: "example.PartDef",
        target: null,
        row: { values: {}, defName: "mypatch_parts_Own1", note: null },
      },
    ]);
    expect(
      wrapper.get('[data-testid="standalone-row-mypatch_parts_Own1"]').attributes("aria-current"),
    ).toBe("true");
  });

  it("clearing a free-standing row records clear_assignment_row and deselects it", async () => {
    const afterClear = assignment({
      sections: [raceGroupSection(), { ...partSection(), standaloneRows: [] }],
    });
    let current = assignment();
    const clearCalls: unknown[] = [];
    const wrapper = await mountEditorPage(current, {
      get_assignment: () => current,
      clear_assignment_row: (payload: unknown) => {
        clearCalls.push((payload as { request: unknown }).request);
        current = afterClear;
        return null;
      },
    });
    await wrapper.get('[data-testid="section-tab-example.PartDef"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-testid="standalone-row-mypatch_parts_Own0"]').trigger("click");
    await flushPromises();

    await wrapper.get('[data-testid="row-editor-clear-button"]').trigger("click");
    await flushPromises();

    expect(clearCalls).toEqual([
      {
        assignmentId: ASSIGNMENT_ID,
        section: "example.PartDef",
        target: null,
        defName: "mypatch_parts_Own0",
      },
    ]);
    expect(wrapper.find('[data-testid="row-editor"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="standalone-section-empty"]').text()).toBe("No rows yet.");
  });

  it("j/k step through the coverage queue", async () => {
    const wrapper = await mountEditorPage(assignment());

    expect(wrapper.get('[data-testid="row-editor"]').text()).toContain("Race0");

    document.body.dispatchEvent(
      new KeyboardEvent("keydown", { key: "j", cancelable: true, bubbles: true }),
    );
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="row-editor"]').text()).toContain("Race1");
  });

  it("Enter focuses the row editor's own first item picker", async () => {
    const wrapper = await mountEditorPage(assignment());

    document.body.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", cancelable: true, bubbles: true }),
    );
    await wrapper.vm.$nextTick();

    expect(document.activeElement).toBe(
      wrapper.get('[data-testid="item-picker-search-example.PartDef"]').element,
    );
  });

  it("adding a section lists candidates, excludes existing sections, and adds the chosen one", async () => {
    const candidatesResponse: ListAssignmentCandidatesResponseDto = {
      effectiveRefs: [{ modId: "example.framework", name: "Example" }],
      candidates: [
        { defType: "example.PartAssignmentDef", instanceCount: 2, owners: [] },
        { defType: "example.OtherGroupDef", instanceCount: 3, owners: [] },
      ],
    };
    const afterAdd = assignment({
      sections: [
        raceGroupSection(),
        partSection(),
        { ...raceGroupSection(), defType: "example.OtherGroupDef" },
      ],
    });
    // `get_assignment` must be stateful, not a fixed closure over the
    // pre-add detail: `useAssignmentQuery`'s own cache is what the page
    // re-renders from once the mutation invalidates it, never the
    // mutation's own return value directly.
    let current = assignment();
    const wrapper = await mountEditorPage(current, {
      get_assignment: () => current,
      list_assignment_candidates: () => candidatesResponse,
      add_assignment_section: () => {
        current = afterAdd;
        return afterAdd;
      },
    });

    await wrapper.get('[data-testid="open-add-section-button"]').trigger("click");
    await flushPromises();

    // Already a section on this project — never offered again.
    expect(
      wrapper.find('[data-testid="add-section-candidate-example.PartAssignmentDef"]').exists(),
    ).toBe(false);
    const radio = wrapper
      .findAllComponents(RadioButton)
      .find((component) => component.props("value") === "example.OtherGroupDef");
    await radio?.vm.$emit("update:modelValue", "example.OtherGroupDef");
    await wrapper.get('[data-testid="add-section-confirm"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="add-section-dialog"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="section-tab-example.OtherGroupDef"]').text()).toBe(
      "example.OtherGroupDef",
    );
  });

  it("shows a generic empty state when no candidates remain, even once this project already has targets set", async () => {
    // The empty state must not claim a non-empty target set is *why*
    // nothing qualifies — `AddAssignmentSection` accepts an explicit
    // free-standing add regardless of T. `assignment()` already carries one
    // target, so this pins that the claim never appears.
    const wrapper = await mountEditorPage(assignment(), {
      list_assignment_candidates: () => ({ effectiveRefs: [], candidates: [] }),
    });

    await wrapper.get('[data-testid="open-add-section-button"]').trigger("click");
    await flushPromises();

    const empty = wrapper.get('[data-testid="add-section-empty"]');
    expect(empty.text()).toContain(
      "every def type this project's own reference mods own is already a section",
    );
    expect(empty.text()).not.toContain("target set is empty");
  });

  it("removing a still-referenced section shows the referencing rows, then 'remove anyway' retries with force", async () => {
    const removeCalls: RemoveAssignmentSectionRequestDto[] = [];
    const afterRemove = assignment({ sections: [raceGroupSection()] });
    let current = assignment();
    const wrapper = await mountEditorPage(current, {
      get_assignment: () => current,
      remove_assignment_section: (payload: unknown) => {
        const { request } = payload as { request: RemoveAssignmentSectionRequestDto };
        removeCalls.push(request);
        if (!request.force) {
          throw {
            code: "assignment_section_in_use",
            message: "still referenced by another section",
            detail: {
              kind: "assignmentSectionInUse",
              referencedBy: [
                { defType: "example.PartAssignmentDef", row: "ThingDef/Race0", path: "parts" },
              ],
            },
          };
        }
        current = afterRemove;
        return { removed: true, assignment: afterRemove };
      },
    });

    await wrapper.get('[data-testid="section-tab-example.PartDef"]').trigger("click");
    await wrapper.get('[data-testid="open-remove-section-button"]').trigger("click");
    await wrapper.get('[data-testid="remove-section-confirm"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="remove-section-in-use"]').text()).toContain("dangling");
    expect(wrapper.get('[data-testid="remove-section-reference-0"]').text()).toContain(
      "ThingDef/Race0",
    );

    await wrapper.get('[data-testid="remove-section-force"]').trigger("click");
    await flushPromises();

    expect(removeCalls).toHaveLength(2);
    expect(removeCalls[0]?.force).toBe(false);
    expect(removeCalls[1]?.force).toBe(true);
    expect(wrapper.find('[data-testid="remove-section-dialog"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="section-tab-example.PartDef"]').exists()).toBe(false);
  });
});
