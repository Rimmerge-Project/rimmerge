import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import AssignmentListPage from "@/pages/AssignmentListPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { AssignmentSummaryDto } from "@/types/generated/AssignmentSummaryDto";

function summary(overrides: Partial<AssignmentSummaryDto> = {}): AssignmentSummaryDto {
  return {
    id: "abc123def456",
    name: "Example race patch",
    packageId: "mypatch.parts",
    displayName: "Sample Part Patch",
    defTypes: ["example.PartAssignmentDef"],
    refs: [{ modId: "example.framework", name: "Example" }],
    targets: [{ modId: "target.races", name: "Target Races" }],
    rowCount: 0,
    isStandalone: false,
    uncoveredCount: 3,
    exportDir: null,
    updatedAt: "2026-09-06T00:00:00Z",
    ...overrides,
  };
}

async function mountListPage(assignments: AssignmentSummaryDto[]) {
  installMockIpc({ list_assignments: () => assignments });
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/assignments", name: "assignments", component: { template: "<div />" } },
      { path: "/assignments/new", name: "assignment-new", component: { template: "<div />" } },
      {
        path: "/assignments/:assignmentId",
        name: "assignment-detail",
        component: { template: "<div />" },
      },
    ],
  });
  await router.push({ name: "assignments" });
  const wrapper = mount(AssignmentListPage, {
    global: {
      plugins: [createPinia(), PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
  await flushPromises();
  return wrapper;
}

describe("AssignmentListPage", () => {
  afterEach(() => {
    clearMocks();
  });

  it("shows the numeric uncovered count for a target-linked project", async () => {
    const wrapper = await mountListPage([summary({ uncoveredCount: 3 })]);

    const cell = wrapper.get('[data-testid="assignment-row-abc123def456"]');
    expect(cell.text()).toContain("3");
    expect(cell.text()).not.toContain("n/a");
  });

  it("shows 'n/a' when coverage isn't applicable (no target-keyed section at all)", async () => {
    const wrapper = await mountListPage([
      summary({ isStandalone: true, uncoveredCount: null, defTypes: ["example.PartDef"] }),
    ]);

    expect(wrapper.get('[data-testid="assignment-row-abc123def456"]').text()).toContain("n/a");
  });

  it("lists every section's own def type", async () => {
    const wrapper = await mountListPage([
      summary({ defTypes: ["example.PartAssignmentDef", "example.PartDef"] }),
    ]);

    expect(wrapper.get('[data-testid="assignment-row-abc123def456"]').text()).toContain(
      "example.PartAssignmentDef and example.PartDef",
    );
  });
});
