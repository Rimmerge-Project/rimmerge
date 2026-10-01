import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import PatchExportPanel from "@/components/patches/PatchExportPanel.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ExportPatchReportDto } from "@/types/generated/ExportPatchReportDto";
import type { ExportPatchRequestDto } from "@/types/generated/ExportPatchRequestDto";
import type { PatchRenderDto } from "@/types/generated/PatchRenderDto";
import type { PendingActiveChangesDto } from "@/types/generated/PendingActiveChangesDto";

const { pickFolderMock } = vi.hoisted(() => ({ pickFolderMock: vi.fn() }));
vi.mock("@/services/dialogs", () => ({ pickFolder: pickFolderMock }));

const PATCH_ID = "abc123def456";

function renderFixture(overrides: Partial<PatchRenderDto> = {}): PatchRenderDto {
  return {
    packageId: "author.wallcompat",
    folderName: "author_wallcompat",
    displayName: "Wall Compatibility Patch",
    dependencies: [
      { modId: "mod.010", name: "Mod 010" },
      { modId: "mod.011", name: "Mod 011" },
    ],
    entries: [
      {
        key: "def_override:ThingDef/Wall:[mod.010,mod.011]",
        defKey: { defType: "ThingDef", defName: "Wall" },
        kind: "defOverride",
        state: { kind: "complete", opCount: 1 },
        opCount: 1,
        dependsOn: ["mod.011"],
        patchFile: "Patches/rimmerge_ThingDef.xml",
        structuralGuardField: null,
      },
    ],
    files: ["About/About.xml", "Patches/rimmerge_ThingDef.xml", "rimmerge.json"],
    skipped: [],
    decisionsSha256: "hash-1",
    caveats: [],
    ...overrides,
  };
}

function mountPanel(
  render: PatchRenderDto,
  options: {
    exportDir?: string | null;
    exportPatch?: (payload: unknown) => unknown;
    pendingActiveChanges?: PendingActiveChangesDto;
  } = {},
) {
  const calls: ExportPatchRequestDto[] = [];
  installMockIpc({
    list_mod_names: {},
    get_pending_active_changes: options.pendingActiveChanges ?? {
      unscanned: { added: [], removed: [] },
      unapplied: { added: [], removed: [] },
    },
    get_patch_render: () => render,
    preview_patch_file: () => ({ content: "<ModMetaData></ModMetaData>" }),
    export_patch: (payload: unknown) => {
      const request = (payload as { request: ExportPatchRequestDto }).request;
      calls.push(request);
      return options.exportPatch
        ? options.exportPatch(payload)
        : ({
            exportPath: `${request.outDir}/${render.folderName}`,
            installedPath: request.install ? `C:/RimWorld/Mods/${render.folderName}` : null,
            modsConfigBackupPath: null,
            skipped: [],
            files: render.files,
            decisionsSha256: render.decisionsSha256,
          } satisfies ExportPatchReportDto);
    },
  });

  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: { template: "<div />" } },
      {
        path: "/patches/:patchId/merge/:key",
        name: "patch-merge-editor",
        component: { template: "<div />" },
      },
    ],
  });

  const wrapper = mount(PatchExportPanel, {
    props: { patchId: PATCH_ID, exportDir: options.exportDir ?? null },
    // Attached to the real document — `focusOutDir`'s own test asserts
    // `document.activeElement`, which a detached (the default) render
    // tree never becomes.
    attachTo: document.body,
    global: {
      plugins: [createPinia(), PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
  return { wrapper, calls };
}

describe("PatchExportPanel", () => {
  afterEach(() => {
    clearMocks();
    pickFolderMock.mockReset();
    document.body.innerHTML = "";
  });

  it("focusOutDir focuses the export directory field — the patch detail page's x shortcut", async () => {
    const { wrapper } = mountPanel(renderFixture());
    await flushPromises();

    (wrapper.vm as unknown as { focusOutDir: () => void }).focusOutDir();
    await wrapper.vm.$nextTick();

    expect(document.activeElement).toBe(
      wrapper.get('[data-testid="patch-export-dir-input"]').element,
    );
  });

  it("shows the render's dependencies, entries, and files, seeding the directory field from exportDir", async () => {
    const { wrapper } = mountPanel(renderFixture(), { exportDir: "C:/exports/wallcompat" });
    await flushPromises();

    expect(
      (wrapper.get('[data-testid="patch-export-dir-input"]').element as HTMLInputElement).value,
    ).toBe("C:/exports/wallcompat");
    expect(wrapper.get('[data-testid="patch-export-dependencies"]').text()).toContain("Mod 010");
    expect(wrapper.get('[data-testid="patch-export-dependencies"]').text()).toContain("Mod 011");
    expect(
      wrapper
        .find('[data-testid="merge-mod-entry-def_override:ThingDef/Wall:[mod.010,mod.011]"]')
        .exists(),
    ).toBe(true);
    expect(wrapper.find('[data-testid="patch-export-file-About/About.xml"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="xml-preview"]').text()).toContain("ModMetaData");
  });

  it("defaults to an empty directory and a disabled Export button when there is no prior export", async () => {
    const { wrapper } = mountPanel(renderFixture());
    await flushPromises();

    expect(
      (wrapper.get('[data-testid="patch-export-dir-input"]').element as HTMLInputElement).value,
    ).toBe("");
    expect(
      (wrapper.get('[data-testid="patch-export-button"]').element as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(wrapper.find('[data-testid="patch-export-last"]').exists()).toBe(false);
  });

  it("fills the directory field from the folder picker", async () => {
    pickFolderMock.mockResolvedValue("C:/chosen/dir");
    const { wrapper } = mountPanel(renderFixture());
    await flushPromises();

    await wrapper.get('[data-testid="patch-export-choose-button"]').trigger("click");
    await flushPromises();

    expect(
      (wrapper.get('[data-testid="patch-export-dir-input"]').element as HTMLInputElement).value,
    ).toBe("C:/chosen/dir");
  });

  it("sends the exact export_patch payload and shows the returned path", async () => {
    const { wrapper, calls } = mountPanel(renderFixture());
    await flushPromises();

    await wrapper.get('[data-testid="patch-export-dir-input"]').setValue("C:/exports/target");
    await wrapper.get('[data-testid="patch-export-button"]').trigger("click");
    await flushPromises();

    expect(calls).toEqual([
      { patchId: PATCH_ID, outDir: "C:/exports/target", install: false, force: false },
    ]);
    expect(wrapper.get('[data-testid="patch-export-last-path"]').text()).toContain(
      "C:/exports/target/author_wallcompat",
    );
  });

  it("includes install in the payload once the checkbox is checked", async () => {
    const { wrapper, calls } = mountPanel(renderFixture());
    await flushPromises();

    await wrapper.get('[data-testid="patch-export-dir-input"]').setValue("C:/exports/target");
    await wrapper.get('[data-testid="patch-export-install-checkbox"]').find("input").setValue(true);
    await wrapper.get('[data-testid="patch-export-button"]').trigger("click");
    await flushPromises();

    expect(calls).toEqual([
      { patchId: PATCH_ID, outDir: "C:/exports/target", install: true, force: false },
    ]);
    expect(wrapper.get('[data-testid="patch-export-last-installed-path"]').text()).toContain(
      "Mods",
    );
  });

  it("shows the pending-activation-changes note only once Install is checked, while the working set is stale", async () => {
    const { wrapper } = mountPanel(renderFixture(), {
      pendingActiveChanges: {
        unscanned: { added: ["a.mod"], removed: [] },
        unapplied: { added: [], removed: [] },
      },
    });
    await flushPromises();

    expect(wrapper.find('[data-testid="patch-export-stale-note"]').exists()).toBe(false);

    await wrapper.get('[data-testid="patch-export-install-checkbox"]').find("input").setValue(true);

    expect(wrapper.get('[data-testid="patch-export-stale-note"]').text()).toContain("not included");
  });

  it("stores the export report's own decisionsSha256, not the sibling render query's cached value", async () => {
    const { wrapper } = mountPanel(renderFixture({ decisionsSha256: "hash-1" }), {
      exportPatch: (payload) => {
        const request = (payload as { request: ExportPatchRequestDto }).request;
        return {
          exportPath: `${request.outDir}/author_wallcompat`,
          installedPath: null,
          modsConfigBackupPath: null,
          skipped: [],
          files: [],
          // Deliberately different from the render fixture's own
          // "hash-1" — the backend recomputes the hash as part of the
          // export itself, and that returned value is the one the panel
          // must remember, not whatever `usePatchRenderQuery` still
          // happens to hold locally at that instant.
          decisionsSha256: "hash-from-report",
        } satisfies ExportPatchReportDto;
      },
    });
    await flushPromises();

    await wrapper.get('[data-testid="patch-export-dir-input"]').setValue("C:/exports/target");
    await wrapper.get('[data-testid="patch-export-button"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="patch-export-last-hash"]').text()).toContain(
      "hash-from-report",
    );
  });

  it("runs the export on Enter in the export-directory field, under the same guard as the button", async () => {
    const { wrapper, calls } = mountPanel(renderFixture());
    await flushPromises();

    const dirInput = wrapper.get('[data-testid="patch-export-dir-input"]');
    await dirInput.setValue("C:/exports/target");
    await dirInput.trigger("keydown.enter");
    await flushPromises();

    expect(calls).toEqual([
      { patchId: PATCH_ID, outDir: "C:/exports/target", install: false, force: false },
    ]);
    expect(wrapper.get('[data-testid="patch-export-last-path"]').text()).toContain(
      "C:/exports/target/author_wallcompat",
    );
  });

  it("does nothing on Enter in the export-directory field while it is empty, matching the disabled button", async () => {
    const { wrapper, calls } = mountPanel(renderFixture());
    await flushPromises();

    await wrapper.get('[data-testid="patch-export-dir-input"]').trigger("keydown.enter");
    await flushPromises();

    expect(calls).toHaveLength(0);
  });

  it("shows the running-game refusal and force-confirm button, then retries with force on 'Export anyway'", async () => {
    let attempt = 0;
    const { wrapper, calls } = mountPanel(renderFixture(), {
      exportPatch: (payload) => {
        attempt += 1;
        const request = (payload as { request: ExportPatchRequestDto }).request;
        if (attempt === 1) {
          throw { code: "rimworld_running", message: "RimWorld is running" };
        }
        return {
          exportPath: `${request.outDir}/author_wallcompat`,
          installedPath: "C:/RimWorld/Mods/author_wallcompat",
          modsConfigBackupPath: "C:/RimWorld/ModsConfig.xml.bak",
          skipped: [],
          files: [],
          decisionsSha256: "hash-1",
        } satisfies ExportPatchReportDto;
      },
    });
    await flushPromises();

    await wrapper.get('[data-testid="patch-export-dir-input"]').setValue("C:/exports/target");
    await wrapper.get('[data-testid="patch-export-install-checkbox"]').find("input").setValue(true);
    await wrapper.get('[data-testid="patch-export-button"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="patch-export-force-warning"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="patch-export-button"]').exists()).toBe(false);

    await wrapper.get('[data-testid="patch-export-force-button"]').trigger("click");
    await flushPromises();

    expect(calls).toEqual([
      { patchId: PATCH_ID, outDir: "C:/exports/target", install: true, force: false },
      { patchId: PATCH_ID, outDir: "C:/exports/target", install: true, force: true },
    ]);
    expect(wrapper.find('[data-testid="patch-export-force-warning"]').exists()).toBe(false);
  });

  it("unchecking install clears a pending force-confirm", async () => {
    const { wrapper } = mountPanel(renderFixture(), {
      exportPatch: () => {
        throw { code: "rimworld_running", message: "RimWorld is running" };
      },
    });
    await flushPromises();

    await wrapper.get('[data-testid="patch-export-dir-input"]').setValue("C:/exports/target");
    await wrapper.get('[data-testid="patch-export-install-checkbox"]').find("input").setValue(true);
    await wrapper.get('[data-testid="patch-export-button"]').trigger("click");
    await flushPromises();
    expect(wrapper.find('[data-testid="patch-export-force-warning"]').exists()).toBe(true);

    await wrapper
      .get('[data-testid="patch-export-install-checkbox"]')
      .find("input")
      .setValue(false);

    expect(wrapper.find('[data-testid="patch-export-force-warning"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="patch-export-button"]').exists()).toBe(true);
  });

  it.each([
    ["nothing to export", "this patch has no complete merge or located asset to export"],
    [
      "the out dir is inside Mods",
      "C:/RimWorld/Mods is inside the game's Mods folder; use install instead",
    ],
    [
      "the target folder is foreign",
      "C:/exports/target/author_wallcompat already exists and wasn't generated by this patch",
    ],
  ])("renders the %s refusal inline", async (_label, message) => {
    const { wrapper } = mountPanel(renderFixture(), {
      exportPatch: () => {
        throw { code: "invalid_input", message };
      },
    });
    await flushPromises();

    await wrapper.get('[data-testid="patch-export-dir-input"]').setValue("C:/exports/target");
    await wrapper.get('[data-testid="patch-export-button"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="patch-export-error"]').text()).toContain(message);
  });
});
