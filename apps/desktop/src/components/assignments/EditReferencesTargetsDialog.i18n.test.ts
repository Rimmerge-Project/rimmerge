// Regression coverage for `schemaChangeCountsText()`'s join: it must use
// a locale-aware `Intl.ListFormat` (via `formatList`), never a
// hard-coded English `", "` — a translator can reorder or reword each
// phrase, but never the literal separator between them.

import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";
import { createI18n } from "vue-i18n";

import EditReferencesTargetsDialog from "@/components/assignments/EditReferencesTargetsDialog.vue";
import { buildPluralRules } from "@/i18n/format";
import en from "@/locales/en.json";
import zhCN from "@/locales/zh-CN.json";
import { installMockIpc } from "@/services/ipc.mock";
import type { AssignmentDetailDto } from "@/types/generated/AssignmentDetailDto";
import type { AssignmentUpdateResultDto } from "@/types/generated/AssignmentUpdateResultDto";

function assignment(): AssignmentDetailDto {
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

/**
 * A zh-CN i18n instance, installed last so it wins over the app-wide
 * `en` instance `i18n/test-setup.ts` installs for every mount — same
 * shape as `SuggestionPanel.i18n.test.ts`'s own `zhI18n()`.
 * `fallbackLocale: "en"` matches `createAppI18n`'s real setting.
 */
function zhI18n() {
  return createI18n({
    legacy: false,
    locale: "zh-CN",
    fallbackLocale: "en",
    messages: { en, "zh-CN": zhCN },
    pluralRules: buildPluralRules(),
  });
}

function mountDialogUnderZhCN() {
  installMockIpc({
    list_mods: () => ({ total: 0, items: [] }),
    update_assignment: () =>
      baseUpdateResult({
        schemaChanges: {
          "example.PartAssignmentDef": {
            added: ["chanceparts", "extraparts"],
            removed: [],
            reclassified: ["parts"],
          },
        },
      }),
  });
  return mount(EditReferencesTargetsDialog, {
    props: { assignment: assignment(), visible: true },
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }], zhI18n()],
      stubs: { teleport: true },
    },
  });
}

describe("EditReferencesTargetsDialog's schema-change join under zh-CN", () => {
  afterEach(() => clearMocks());

  it("joins the added/reclassified phrases with 、/和, never an English comma", async () => {
    const wrapper = mountDialogUnderZhCN();
    await flushPromises();

    await wrapper.get('[data-testid="edit-refs-targets-save"]').trigger("click");
    await flushPromises();

    const schemaChange = wrapper.get('[data-testid="schema-change-example.PartAssignmentDef"]');
    // `Intl.ListFormat("zh-CN", { type: "conjunction" })` joins two items
    // with `和` and no Latin comma between them at all.
    expect(schemaChange.text()).toMatch(/新增.*和.*重新分类/);
    expect(schemaChange.text()).not.toContain(", ");
  });
});
