import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it, vi } from "vitest";

import AssignmentExportPanel from "@/components/assignments/AssignmentExportPanel.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ExportAssignmentReportDto } from "@/types/generated/ExportAssignmentReportDto";
import type { ExportAssignmentRequestDto } from "@/types/generated/ExportAssignmentRequestDto";
import type { PendingActiveChangesDto } from "@/types/generated/PendingActiveChangesDto";

const { pickFolderMock } = vi.hoisted(() => ({ pickFolderMock: vi.fn() }));
vi.mock("@/services/dialogs", () => ({ pickFolder: pickFolderMock }));

const ASSIGNMENT_ID = "abc123def456";

function mountPanel(
  exportAssignment?: (payload: unknown) => unknown,
  pendingActiveChanges?: PendingActiveChangesDto,
) {
  const calls: ExportAssignmentRequestDto[] = [];
  installMockIpc({
    get_pending_active_changes: pendingActiveChanges ?? {
      unscanned: { added: [], removed: [] },
      unapplied: { added: [], removed: [] },
    },
    export_assignment: (payload: unknown) => {
      const request = (payload as { request: ExportAssignmentRequestDto }).request;
      calls.push(request);
      return exportAssignment
        ? exportAssignment(payload)
        : ({
            exportPath: `${request.outDir}/mypatch_parts`,
            installedPath: null,
            modsConfigBackupPath: null,
            skipped: [],
            files: ["About/About.xml"],
            contentSha256: "0".repeat(64),
          } satisfies ExportAssignmentReportDto);
    },
  });
  const wrapper = mount(AssignmentExportPanel, {
    props: { assignmentId: ASSIGNMENT_ID, exportDir: null },
    global: { plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]] },
  });
  return { wrapper, calls };
}

describe("AssignmentExportPanel", () => {
  afterEach(() => {
    clearMocks();
    pickFolderMock.mockReset();
  });

  it("shows the pending-activation-changes note only once Install is checked, while the working set is stale", async () => {
    const { wrapper } = mountPanel(undefined, {
      unscanned: { added: ["a.mod"], removed: [] },
      unapplied: { added: [], removed: [] },
    });
    await flushPromises();

    expect(wrapper.find('[data-testid="assignment-export-stale-note"]').exists()).toBe(false);

    await wrapper
      .get('[data-testid="assignment-export-install-checkbox"]')
      .find("input")
      .setValue(true);

    expect(wrapper.get('[data-testid="assignment-export-stale-note"]').text()).toContain(
      "not included",
    );
  });

  it("sends the exact export_assignment payload and shows the returned path", async () => {
    const { wrapper, calls } = mountPanel();

    await wrapper.get('[data-testid="assignment-export-dir-input"]').setValue("C:/exports/example");
    await wrapper.get('[data-testid="assignment-export-button"]').trigger("click");
    await flushPromises();

    expect(calls).toEqual([
      { assignmentId: ASSIGNMENT_ID, outDir: "C:/exports/example", install: false, force: false },
    ]);
    expect(wrapper.get('[data-testid="assignment-export-last-path"]').text()).toContain(
      "C:/exports/example/mypatch_parts",
    );
  });

  it("shows every skipped field after an export that skips some", async () => {
    const { wrapper } = mountPanel((payload) => {
      const request = (payload as { request: ExportAssignmentRequestDto }).request;
      return {
        exportPath: `${request.outDir}/mypatch_parts`,
        installedPath: null,
        modsConfigBackupPath: null,
        skipped: [
          {
            defType: "example.PartAssignmentDef",
            target: { keyField: "speciesNames", def: { defType: "ThingDef", defName: "Elf" } },
            ownDefName: null,
            path: "parts",
            reason: "item(s) Wrench no longer exist in the active list",
          },
        ],
        files: ["About/About.xml"],
        contentSha256: "0".repeat(64),
      } satisfies ExportAssignmentReportDto;
    });

    await wrapper.get('[data-testid="assignment-export-dir-input"]').setValue("C:/exports/example");
    await wrapper.get('[data-testid="assignment-export-button"]').trigger("click");
    await flushPromises();

    const skipped = wrapper.get('[data-testid="assignment-export-skipped"]');
    expect(skipped.text()).toContain("ThingDef/Elf");
    expect(skipped.text()).toContain("parts");
    expect(skipped.text()).toContain("no longer exist in the active list");
  });

  it("shows a free-standing skip by its own bare defName, not a target (nit)", async () => {
    const { wrapper } = mountPanel((payload) => {
      const request = (payload as { request: ExportAssignmentRequestDto }).request;
      return {
        exportPath: `${request.outDir}/mypatch_parts`,
        installedPath: null,
        modsConfigBackupPath: null,
        skipped: [
          {
            defType: "example.PartDef",
            target: null,
            ownDefName: "sample_own_part",
            path: "flavorText",
            reason: "referenced mod no longer active",
          },
        ],
        files: ["About/About.xml"],
        contentSha256: "0".repeat(64),
      } satisfies ExportAssignmentReportDto;
    });

    await wrapper.get('[data-testid="assignment-export-dir-input"]').setValue("C:/exports/example");
    await wrapper.get('[data-testid="assignment-export-button"]').trigger("click");
    await flushPromises();

    const skipped = wrapper.get('[data-testid="assignment-export-skipped"]');
    expect(skipped.text()).toContain("sample_own_part");
    expect(skipped.text()).toContain("flavorText");
  });

  it("shows no skipped section when nothing was skipped", async () => {
    const { wrapper } = mountPanel();

    await wrapper.get('[data-testid="assignment-export-dir-input"]').setValue("C:/exports/example");
    await wrapper.get('[data-testid="assignment-export-button"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="assignment-export-skipped"]').exists()).toBe(false);
  });

  it("shows the running-game refusal and force-confirm button, then retries with force", async () => {
    let attempt = 0;
    const { wrapper, calls } = mountPanel((payload) => {
      attempt += 1;
      const request = (payload as { request: ExportAssignmentRequestDto }).request;
      if (attempt === 1) {
        throw { code: "rimworld_running", message: "RimWorld is running" };
      }
      return {
        exportPath: `${request.outDir}/mypatch_parts`,
        installedPath: "C:/RimWorld/Mods/mypatch_parts",
        modsConfigBackupPath: "C:/RimWorld/ModsConfig.xml.bak",
        skipped: [],
        files: [],
        contentSha256: "0".repeat(64),
      } satisfies ExportAssignmentReportDto;
    });

    await wrapper.get('[data-testid="assignment-export-dir-input"]').setValue("C:/exports/example");
    await wrapper
      .get('[data-testid="assignment-export-install-checkbox"]')
      .find("input")
      .setValue(true);
    await wrapper.get('[data-testid="assignment-export-button"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="assignment-export-force-warning"]').exists()).toBe(true);

    await wrapper.get('[data-testid="assignment-export-force-button"]').trigger("click");
    await flushPromises();

    expect(calls).toEqual([
      { assignmentId: ASSIGNMENT_ID, outDir: "C:/exports/example", install: true, force: false },
      { assignmentId: ASSIGNMENT_ID, outDir: "C:/exports/example", install: true, force: true },
    ]);
  });

  it("fills the directory field from the folder picker", async () => {
    pickFolderMock.mockResolvedValue("C:/chosen/dir");
    const { wrapper } = mountPanel();

    await wrapper.get('[data-testid="assignment-export-choose-button"]').trigger("click");
    await flushPromises();

    expect(
      (wrapper.get('[data-testid="assignment-export-dir-input"]').element as HTMLInputElement)
        .value,
    ).toBe("C:/chosen/dir");
  });
});
