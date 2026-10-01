import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import MergeModEntryList from "@/components/merge/MergeModEntryList.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { MergeModEntryDto } from "@/types/generated/MergeModEntryDto";

const HEDIFF_ENTRY: MergeModEntryDto = {
  key: "def_override:HediffDef/BionicHeart:[a,b]",
  defKey: { defType: "HediffDef", defName: "BionicHeart" },
  kind: "defOverride",
  state: { kind: "complete", opCount: 5 },
  opCount: 5,
  dependsOn: ["b"],
  patchFile: "Patches/rimmerge_HediffDef.xml",
  structuralGuardField: null,
};

const THING_ENTRY: MergeModEntryDto = {
  key: "patch_collision:ThingDef/Wall:defName:hp:[c,d]",
  defKey: { defType: "ThingDef", defName: "Wall" },
  kind: "patchCollision",
  state: { kind: "needsFieldInput", unresolved: 1, total: 1 },
  opCount: 0,
  dependsOn: ["d"],
  patchFile: null,
  structuralGuardField: null,
};

// A pre-existing
// `Merge` decision on a def that later became guarded — the state pill
// must read the same "confirm the winner" wording the merge editor and
// apply dialog use, never "N fields need input" next to an "Open in
// editor" link leading to a header saying the opposite.
const GUARDED_ENTRY: MergeModEntryDto = {
  key: "def_override:HediffDef/BionicHeart:[a,b]",
  defKey: { defType: "HediffDef", defName: "BionicHeart" },
  kind: "defOverride",
  state: { kind: "needsFieldInput", unresolved: 9, total: 9 },
  opCount: 0,
  dependsOn: ["b"],
  patchFile: null,
  structuralGuardField: "ParentName",
};

const ASSET_ENTRY: MergeModEntryDto = {
  key: "ship_asset:ui/foo:[e,f]",
  defKey: null,
  kind: "asset",
  state: { kind: "complete", opCount: 1 },
  opCount: 1,
  dependsOn: ["f"],
  patchFile: null,
  structuralGuardField: null,
};

/**
 * `list_mod_names` needs *some* registered fixture: `useModLabel` (via
 * the new "depends on" line) always queries it, and an unregistered
 * command's rejection reaches Vitest as an unhandled rejection that
 * fails the whole run, not just this file (see the identical note in
 * `InboxPage.test.ts`).
 */
async function mountList(entries: MergeModEntryDto[], props: Record<string, unknown> = {}) {
  installMockIpc({ list_mod_names: {} });
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: { template: "<div />" } },
      { path: "/merge/:key", name: "merge-editor", component: { template: "<div />" } },
      {
        path: "/patches/:patchId/merge/:key",
        name: "patch-merge-editor",
        component: { template: "<div />" },
      },
    ],
  });
  await router.push("/");
  const wrapper = mount(MergeModEntryList, {
    props: { entries, ...props },
    global: { plugins: [createPinia(), PiniaColada, router] },
  });
  return { wrapper, router };
}

describe("MergeModEntryList", () => {
  afterEach(() => {
    clearMocks();
  });

  it("groups entries by def type and shows each one's state and op count", async () => {
    const { wrapper } = await mountList([HEDIFF_ENTRY, THING_ENTRY]);

    expect(wrapper.get(`[data-testid="merge-mod-entry-${HEDIFF_ENTRY.key}"]`).text()).toContain(
      "merged",
    );
    expect(wrapper.get(`[data-testid="merge-mod-entry-${HEDIFF_ENTRY.key}"]`).text()).toContain(
      "5 ops",
    );
    expect(wrapper.get(`[data-testid="merge-mod-entry-${THING_ENTRY.key}"]`).text()).toContain(
      "1 field needs input",
    );
  });

  it("shows a structurally-guarded entry's own 'confirm the winner' wording, not a field count", async () => {
    const { wrapper } = await mountList([GUARDED_ENTRY]);

    const row = wrapper.get(`[data-testid="merge-mod-entry-${GUARDED_ENTRY.key}"]`);
    expect(row.text()).toContain("not auto-merged — confirm the winner");
    expect(row.text()).not.toContain("need input");
  });

  it("renders an asset entry under an 'Assets' group with no open-in-editor link", async () => {
    const { wrapper } = await mountList([ASSET_ENTRY]);

    const group = wrapper.get(`[data-testid="merge-mod-entry-${ASSET_ENTRY.key}"]`);
    expect(wrapper.get("h3").text()).toBe("Assets");
    expect(group.text()).toContain("asset");
    expect(wrapper.find(`[data-testid="merge-mod-open-editor-${ASSET_ENTRY.key}"]`).exists()).toBe(
      false,
    );
  });

  it("navigates to the finding's merge editor when 'Open in editor' is clicked", async () => {
    const { wrapper, router } = await mountList([HEDIFF_ENTRY]);

    await wrapper.get(`[data-testid="merge-mod-open-editor-${HEDIFF_ENTRY.key}"]`).trigger("click");
    await flushPromises();

    expect(router.currentRoute.value.name).toBe("merge-editor");
    expect(router.currentRoute.value.params["key"]).toBe(HEDIFF_ENTRY.key);
  });

  it("uses a given editorRoute instead of the profile merge editor", async () => {
    const { wrapper, router } = await mountList([HEDIFF_ENTRY], {
      editorRoute: (key: string) => ({
        name: "patch-merge-editor",
        params: { patchId: "p1", key },
      }),
    });

    await wrapper.get(`[data-testid="merge-mod-open-editor-${HEDIFF_ENTRY.key}"]`).trigger("click");
    await flushPromises();

    expect(router.currentRoute.value.name).toBe("patch-merge-editor");
    expect(router.currentRoute.value.params).toMatchObject({
      patchId: "p1",
      key: HEDIFF_ENTRY.key,
    });
  });
});
