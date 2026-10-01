import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import MergeModPage from "@/pages/MergeModPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { MergeModDto } from "@/types/generated/MergeModDto";
import type { PreviewMergeModFileRequestDto } from "@/types/generated/PreviewMergeModFileRequestDto";

const COMPLETE_KEY = "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]";
const NEEDS_INPUT_KEY =
  "def_override:HeadTypeDef/HeadNormal:[exampleanim.mod,ludeon.rimworld,example.bionicsfork]";

function mergeModFixture(): MergeModDto {
  return {
    packageId: "rimmerge.merge.3f9a1c2b7d5e",
    folderName: "rimmerge_merge_3f9a1c2b7d5e",
    modsPath: "C:/RimWorld/Mods/rimmerge_merge_3f9a1c2b7d5e",
    exists: false,
    entries: [
      {
        key: COMPLETE_KEY,
        defKey: { defType: "HediffDef", defName: "BionicHeart" },
        kind: "defOverride",
        state: { kind: "complete", opCount: 5 },
        opCount: 5,
        dependsOn: ["example.bionicsfork"],
        patchFile: "Patches/rimmerge_HediffDef.xml",
        structuralGuardField: null,
      },
      {
        key: NEEDS_INPUT_KEY,
        defKey: { defType: "HeadTypeDef", defName: "HeadNormal" },
        kind: "defOverride",
        state: { kind: "needsFieldInput", unresolved: 2, total: 4 },
        opCount: 0,
        dependsOn: ["exampleanim.mod"],
        patchFile: null,
        structuralGuardField: null,
      },
    ],
    files: ["About/About.xml", "Patches/rimmerge_HediffDef.xml", "rimmerge.json"],
    sourceMods: [{ modId: "example.bionicsfork", name: "Example Bionics Fork" }],
  };
}

function fileContent(relativePath: string): string {
  if (relativePath === "About/About.xml") {
    return "<ModMetaData><packageId>rimmerge.merge.3f9a1c2b7d5e</packageId></ModMetaData>";
  }
  if (relativePath === "rimmerge.json") {
    return '{"profileHash":"3f9a1c2b7d5e"}';
  }
  return '<Patch><Operation Class="PatchOperationSequence" /></Patch>';
}

function mountPage(mergeMod: MergeModDto) {
  installMockIpc({
    get_merge_mod: mergeMod,
    preview_merge_mod_file: (payload: unknown) => {
      const request = (payload as { request: PreviewMergeModFileRequestDto }).request;
      if (!mergeMod.files.includes(request.relativePath)) {
        throw { code: "invalid_input", message: "not a file this render produces" };
      }
      return { content: fileContent(request.relativePath) };
    },
    // `useModLabel` (via `MergeModEntryList`/this page's own "Source
    // mods" section) always queries this — see the identical note in
    // `InboxPage.test.ts`.
    list_mod_names: {},
  });

  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/merge-mod", name: "merge-mod", component: MergeModPage },
      { path: "/merge/:key", name: "merge-editor", component: { template: "<div />" } },
    ],
  });

  const pinia = createPinia();
  setActivePinia(pinia);

  return mount(MergeModPage, {
    global: { plugins: [pinia, PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }]] },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("MergeModPage", () => {
  afterEach(() => {
    clearMocks();
  });

  it("lists entries with their merge state and previews About.xml by default", async () => {
    const wrapper = mountPage(mergeModFixture());
    await flush();
    await wrapper.vm.$nextTick();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="merge-mod-package-id"]').text()).toBe(
      "rimmerge.merge.3f9a1c2b7d5e",
    );
    expect(wrapper.get('[data-testid="merge-mod-folder-name"]').text()).toBe(
      "rimmerge_merge_3f9a1c2b7d5e",
    );
    expect(wrapper.get(`[data-testid="merge-mod-entry-${COMPLETE_KEY}"]`).text()).toContain(
      "merged",
    );
    expect(wrapper.get(`[data-testid="merge-mod-entry-${NEEDS_INPUT_KEY}"]`).text()).toContain(
      "2 fields need input",
    );
    expect(wrapper.get('[data-testid="xml-preview"]').text()).toContain(
      "rimmerge.merge.3f9a1c2b7d5e",
    );
    // Finding 10: the page states what the generated mod actually
    // contains, not just its package id/folder/path.
    const summary = wrapper.get('[data-testid="merge-mod-summary"]').text();
    expect(summary).toContain("2 merged defs");
    expect(summary).toContain("sourced from 1 mod");
  });

  it("switches the preview when a different file is selected", async () => {
    const wrapper = mountPage(mergeModFixture());
    await flush();
    await wrapper.vm.$nextTick();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="merge-mod-file-rimmerge.json"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="xml-preview"]').text()).toContain("profileHash");
  });

  it("shows an empty state when no Merge/ShipAsset decision exists yet", async () => {
    const wrapper = mountPage({
      packageId: "rimmerge.merge.3f9a1c2b7d5e",
      folderName: "rimmerge_merge_3f9a1c2b7d5e",
      modsPath: "C:/RimWorld/Mods/rimmerge_merge_3f9a1c2b7d5e",
      exists: false,
      entries: [],
      files: [],
      sourceMods: [],
    });
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="merge-mod-entry-list"]').exists()).toBe(false);
    expect(wrapper.findAll('[data-testid="empty-state"]')).toHaveLength(2);
    expect(wrapper.get('[data-testid="merge-mod-summary"]').text()).toContain("currently empty");
  });
});
