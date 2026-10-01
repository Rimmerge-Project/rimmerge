import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import PatchDetailPage from "@/pages/PatchDetailPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { PatchDetailDto } from "@/types/generated/PatchDetailDto";
import type { PatchRenderDto } from "@/types/generated/PatchRenderDto";

const { toastAddMock } = vi.hoisted(() => ({ toastAddMock: vi.fn() }));
vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: toastAddMock }) }));

const PATCH_ID = "abc123def456";
const STATS = { auto: 0, needsInput: 0, overridden: 0, resolvedBySuggested: 0 };

function patchDetail(overrides: Partial<PatchDetailDto> = {}): PatchDetailDto {
  return {
    id: PATCH_ID,
    name: "Wall compat",
    packageId: "author.wallcompat",
    displayName: "Wall Compatibility Patch",
    scope: [
      { modId: "a.mod", name: "Mod A" },
      { modId: "b.mod", name: "Mod B" },
    ],
    decisionCount: 0,
    completeCount: 0,
    needsInputCount: 0,
    exportDir: null,
    updatedAt: "2026-09-05T00:00:00Z",
    author: "Rimmerge",
    description: "Compatibility patch for Mod A, Mod B.",
    folderName: "author_wallcompat",
    orphaned: [],
    stats: STATS,
    ...overrides,
  };
}

function renderFixture(): PatchRenderDto {
  return {
    packageId: "author.wallcompat",
    folderName: "author_wallcompat",
    displayName: "Wall Compatibility Patch",
    dependencies: [],
    entries: [],
    files: [],
    skipped: [],
    decisionsSha256: "hash-1",
    caveats: [],
  };
}

async function mountDetailPage(patch: PatchDetailDto, extraFixtures: Record<string, unknown> = {}) {
  installMockIpc({
    get_patch: () => patch,
    list_mods: { total: 0, items: [] },
    list_mod_names: {},
    get_pending_active_changes: {
      unscanned: { added: [], removed: [] },
      unapplied: { added: [], removed: [] },
    },
    get_patch_render: () => renderFixture(),
    list_patch_findings: () => ({ total: 0, items: [] }),
    ...extraFixtures,
  });

  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/patches", name: "patches", component: { template: "<div />" } },
      { path: "/patches/:patchId", name: "patch-detail", component: { template: "<div />" } },
    ],
  });
  await router.push({ name: "patch-detail", params: { patchId: PATCH_ID } });

  const wrapper = mount(PatchDetailPage, {
    // Attached to the real document — the shortcut tests below dispatch
    // real keydown events and assert real DOM focus, neither of which a
    // detached (the default) render tree supports.
    attachTo: document.body,
    global: {
      plugins: [createPinia(), PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
  await flushPromises();
  await wrapper.vm.$nextTick();
  return wrapper;
}

describe("PatchDetailPage", () => {
  afterEach(() => {
    clearMocks();
    toastAddMock.mockReset();
    document.body.innerHTML = "";
  });

  it("s focuses the scope search field", async () => {
    const wrapper = await mountDetailPage(patchDetail());

    document.body.dispatchEvent(
      new KeyboardEvent("keydown", { key: "s", cancelable: true, bubbles: true }),
    );
    await wrapper.vm.$nextTick();

    expect(document.activeElement).toBe(wrapper.get('[data-testid="scope-search"]').element);
  });

  it("x focuses the export directory field", async () => {
    const wrapper = await mountDetailPage(patchDetail());

    document.body.dispatchEvent(
      new KeyboardEvent("keydown", { key: "x", cancelable: true, bubbles: true }),
    );
    await wrapper.vm.$nextTick();

    expect(document.activeElement).toBe(
      wrapper.get('[data-testid="patch-export-dir-input"]').element,
    );
  });

  it("s/x are inert while a text field already has focus", async () => {
    const wrapper = await mountDetailPage(patchDetail());

    const nameInput = wrapper.get('[data-testid="patch-name-input"]').element as HTMLInputElement;
    nameInput.focus();

    nameInput.dispatchEvent(
      new KeyboardEvent("keydown", { key: "s", cancelable: true, bubbles: true }),
    );
    await wrapper.vm.$nextTick();

    // The keystroke was typed into the focused field, not stolen as a
    // shortcut — the scope search box never gained focus.
    expect(document.activeElement).toBe(nameInput);
    expect(document.activeElement).not.toBe(wrapper.get('[data-testid="scope-search"]').element);
  });

  it("prune toast reports before minus the mutation's own returned orphaned count, not the pre-prune count outright", async () => {
    const wrapper = await mountDetailPage(patchDetail({ orphaned: ["a", "b"] }), {
      prune_patch_decisions: () => patchDetail({ orphaned: ["b"] }),
    });

    await wrapper.get('[data-testid="prune-orphaned-button"]').trigger("click");
    await flushPromises();

    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({ detail: expect.stringContaining("Removed 1 decision the") }),
    );
  });

  it("import-with-skips toast pluralizes the imported and skipped counts independently", async () => {
    const wrapper = await mountDetailPage(patchDetail(), {
      import_profile_decisions: () => ({
        imported: ["key-a"],
        skipped: [
          { key: "key-b", cause: { kind: "outOfScope" } },
          { key: "key-c", cause: { kind: "notPatchable", action: "reorder" } },
        ],
      }),
    });

    await wrapper.get('[data-testid="import-profile-decisions-button"]').trigger("click");
    await flushPromises();

    // `count` (1, singular "decision") and `skippedCount` (2, plural
    // "decisions") each pick their own plural form —
    // `patches.detail.importedWithSkips.{imported,skipped}` — rather than
    // one message sharing a single selector across both numbers.
    expect(toastAddMock).toHaveBeenCalledWith(
      expect.objectContaining({
        detail:
          "Imported 1 decision. 2 decisions skipped: it is not a finding between this patch's scope members; a patch can only hold merge, shipped-asset and ignore decisions — not Reorder",
      }),
    );
  });

  it("import-with-skips toast joins skipped reasons with a semicolon, not a list conjunction", async () => {
    await mountDetailPage(patchDetail(), {
      import_profile_decisions: () => ({
        imported: [],
        skipped: [
          { key: "key-b", cause: { kind: "outOfScope" } },
          { key: "key-c", cause: { kind: "notPatchable", action: "reorder" } },
          { key: "key-d", cause: { kind: "notPatchable", action: "accept" } },
        ],
      }),
    }).then((wrapper) =>
      wrapper.get('[data-testid="import-profile-decisions-button"]').trigger("click"),
    );
    await flushPromises();

    expect(toastAddMock).toHaveBeenLastCalledWith(
      expect.objectContaining({
        detail:
          "Imported 0 decisions. 3 decisions skipped: it is not a finding between this patch's scope members; a patch can only hold merge, shipped-asset and ignore decisions — not Reorder; a patch can only hold merge, shipped-asset and ignore decisions — not Accept",
      }),
    );
  });

  it("import-with-skips toast shows a single skipped reason without a separator", async () => {
    const wrapper = await mountDetailPage(patchDetail(), {
      import_profile_decisions: () => ({
        imported: ["key-a"],
        skipped: [{ key: "key-b", cause: { kind: "outOfScope" } }],
      }),
    });

    await wrapper.get('[data-testid="import-profile-decisions-button"]').trigger("click");
    await flushPromises();

    expect(toastAddMock).toHaveBeenLastCalledWith(
      expect.objectContaining({
        detail:
          "Imported 1 decision. 1 decision skipped: it is not a finding between this patch's scope members",
      }),
    );
  });
});
