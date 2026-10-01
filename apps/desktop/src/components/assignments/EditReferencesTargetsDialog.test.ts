import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import EditReferencesTargetsDialog from "@/components/assignments/EditReferencesTargetsDialog.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { AssignmentDetailDto } from "@/types/generated/AssignmentDetailDto";
import type { AssignmentUpdateResultDto } from "@/types/generated/AssignmentUpdateResultDto";
import type { UpdateAssignmentRequestDto } from "@/types/generated/UpdateAssignmentRequestDto";

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
    sections: [],
    createdAt: "2026-09-06T00:00:00Z",
    ...overrides,
  };
}

function baseUpdateResult(
  overrides: Partial<AssignmentUpdateResultDto> = {},
): AssignmentUpdateResultDto {
  return {
    assignment: assignment(),
    schemaChanges: {},
    strandedValues: [],
    droppedRows: [],
    ...overrides,
  };
}

function mountDialog(
  props: InstanceType<typeof EditReferencesTargetsDialog>["$props"],
  fixtures: Record<string, unknown> = {},
) {
  installMockIpc({
    list_mods: () => ({ total: 0, items: [] }),
    ...fixtures,
  });
  return mount(EditReferencesTargetsDialog, {
    props,
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true },
    },
  });
}

describe("EditReferencesTargetsDialog", () => {
  afterEach(() => {
    clearMocks();
  });

  it("seeds R/T chips from the project's own current refs/targets when opened", async () => {
    const wrapper = mountDialog({ assignment: assignment(), visible: true });
    await flushPromises();

    expect(wrapper.get('[data-testid="edit-refs-member-example.framework"]').text()).toContain(
      "Example",
    );
    expect(wrapper.get('[data-testid="edit-targets-member-target.races"]').text()).toContain(
      "Target Races",
    );
  });

  it("removing a ref chip and saving records update_assignment with the trimmed refs, leaving name/author/description/excludedRefs untouched", async () => {
    const calls: UpdateAssignmentRequestDto[] = [];
    const wrapper = mountDialog(
      { assignment: assignment(), visible: true },
      {
        update_assignment: (payload: unknown) => {
          calls.push((payload as { request: UpdateAssignmentRequestDto }).request);
          return baseUpdateResult();
        },
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="edit-refs-member-remove-example.framework"]').trigger("click");
    await wrapper.get('[data-testid="edit-refs-targets-save"]').trigger("click");
    await flushPromises();

    expect(calls).toEqual([
      {
        assignmentId: "abc123def456",
        name: null,
        author: null,
        description: null,
        refs: [],
        excludedRefs: null,
        targets: ["target.races"],
      },
    ]);
  });

  it("discloses stranded values and dropped rows after a save that caused them", async () => {
    const wrapper = mountDialog(
      { assignment: assignment(), visible: true },
      {
        update_assignment: () =>
          baseUpdateResult({
            schemaChanges: {
              "example.PartAssignmentDef": {
                added: ["chanceparts"],
                removed: [],
                reclassified: ["parts"],
              },
            },
            strandedValues: [
              { defType: "example.PartAssignmentDef", row: "ThingDef/Race0", path: "parts" },
            ],
            droppedRows: [
              {
                defType: "example.PartAssignmentDef",
                target: {
                  keyField: "speciesNames",
                  def: { defType: "ThingDef", defName: "Race1" },
                },
              },
            ],
          }),
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="edit-refs-targets-save"]').trigger("click");
    await flushPromises();

    const schemaChange = wrapper.get('[data-testid="schema-change-example.PartAssignmentDef"]');
    expect(schemaChange.text()).toContain("1 field added");
    expect(schemaChange.text()).toContain("1 field reclassified");
    expect(schemaChange.text()).toContain("parts");

    const stranded = wrapper.get('[data-testid="edit-refs-targets-stranded"]');
    expect(stranded.text()).toContain("1 stored value was dropped");
    expect(stranded.text()).toContain("ThingDef/Race0");

    const dropped = wrapper.get('[data-testid="edit-refs-targets-dropped"]');
    expect(dropped.text()).toContain("1 row was dropped");
    expect(dropped.text()).toContain("ThingDef/Race1");

    expect(wrapper.find('[data-testid="edit-refs-targets-clean"]').exists()).toBe(false);
  });

  it("shows a clean confirmation when a save drops or strands nothing", async () => {
    const wrapper = mountDialog(
      { assignment: assignment(), visible: true },
      { update_assignment: () => baseUpdateResult() },
    );
    await flushPromises();

    await wrapper.get('[data-testid="edit-refs-targets-save"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="edit-refs-targets-clean"]').text()).toBe(
      "Nothing was dropped or stranded by this change.",
    );
  });

  it("shows the save error, then clears it and shows the outcome once a retry succeeds", async () => {
    let shouldFail = true;
    const wrapper = mountDialog(
      { assignment: assignment(), visible: true },
      {
        update_assignment: () => {
          if (shouldFail) {
            throw { code: "invalid_input", message: "refs must not be empty" };
          }
          return baseUpdateResult();
        },
      },
    );
    await flushPromises();

    await wrapper.get('[data-testid="edit-refs-targets-save"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="edit-refs-targets-error"]').text()).toContain(
      "refs must not be empty",
    );
    expect(wrapper.find('[data-testid="edit-refs-targets-outcome"]').exists()).toBe(false);

    shouldFail = false;
    await wrapper.get('[data-testid="edit-refs-targets-save"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="edit-refs-targets-error"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="edit-refs-targets-clean"]').exists()).toBe(true);
  });

  it("re-seeds from the project's own current R/T and clears the previous outcome each time it opens", async () => {
    const wrapper = mountDialog(
      { assignment: assignment(), visible: false },
      { update_assignment: () => baseUpdateResult({ droppedRows: [] }) },
    );
    await flushPromises();

    await wrapper.setProps({ visible: true });
    await flushPromises();
    await wrapper.get('[data-testid="edit-refs-targets-save"]').trigger("click");
    await flushPromises();
    expect(wrapper.find('[data-testid="edit-refs-targets-outcome"]').exists()).toBe(true);

    await wrapper.setProps({ visible: false });
    await wrapper.setProps({ visible: true });
    await flushPromises();

    expect(wrapper.find('[data-testid="edit-refs-targets-outcome"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="edit-refs-member-example.framework"]').exists()).toBe(true);
  });
});
