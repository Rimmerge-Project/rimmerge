import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import ScopeEditor from "@/components/patches/ScopeEditor.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ModRefDto } from "@/types/generated/ModRefDto";
import type { ModSummaryDto } from "@/types/generated/ModSummaryDto";

const MEMBERS: ModRefDto[] = [
  { modId: "a.mod", name: "Mod A" },
  { modId: "b.mod", name: "Mod B" },
];

function mod(overrides: Partial<ModSummaryDto> = {}): ModSummaryDto {
  return {
    modId: "c.mod",
    name: "Mod C",
    source: "workshop",
    tags: [],
    hardDependents: 0,
    missing: false,
    generated: null,
    workshopId: null,
    ...overrides,
  };
}

function mountEditor(props: InstanceType<typeof ScopeEditor>["$props"]) {
  return mount(ScopeEditor, {
    props,
    // Attached to the real document — `focusSearch`'s own test asserts
    // `document.activeElement`, which a detached (the default) render
    // tree never becomes.
    attachTo: document.body,
    global: { plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]] },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("ScopeEditor", () => {
  afterEach(() => {
    clearMocks();
    document.body.innerHTML = "";
  });

  it("renders a chip per current member and no shrink summary by default", () => {
    installMockIpc({ list_mods: { total: 0, items: [] } });
    const wrapper = mountEditor({ members: MEMBERS });

    expect(wrapper.get('[data-testid="scope-member-a.mod"]').text()).toContain("Mod A");
    expect(wrapper.get('[data-testid="scope-member-b.mod"]').text()).toContain("Mod B");
    expect(wrapper.find('[data-testid="scope-change-summary"]').exists()).toBe(false);
  });

  it("emits change with the member removed when its chip's remove button is clicked", async () => {
    installMockIpc({ list_mods: { total: 0, items: [] } });
    // Three members, not `MEMBERS`' own two — the remove button is
    // disabled at the two-member floor (see the dedicated tests below),
    // so removing one here needs a scope one above it.
    const THREE_MEMBERS = [...MEMBERS, { modId: "c.mod", name: "Mod C" }];
    const wrapper = mountEditor({ members: THREE_MEMBERS });

    await wrapper.get('[data-testid="scope-member-remove-a.mod"]').trigger("click");

    expect(wrapper.emitted("change")).toEqual([[[THREE_MEMBERS[1], THREE_MEMBERS[2]]]]);
  });

  it("searches list_mods, excludes generated mods and existing members, and emits change on add", async () => {
    installMockIpc({
      list_mods: {
        total: 3,
        items: [
          mod({ modId: "c.mod", name: "Mod C" }),
          mod({ modId: "a.mod", name: "Mod A" }), // already a member — excluded
          mod({
            modId: "gen.mod",
            name: "Generated",
            generated: { kind: "merge", patchId: null, scope: null },
          }), // excluded
        ],
      },
    });
    const wrapper = mountEditor({ members: MEMBERS });

    await wrapper.get('[data-testid="scope-search"]').setValue("mod");
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="scope-add-c.mod"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="scope-add-a.mod"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="scope-add-gen.mod"]').exists()).toBe(false);

    await wrapper.get('[data-testid="scope-add-c.mod"]').trigger("click");
    expect(wrapper.emitted("change")).toEqual([[[...MEMBERS, { modId: "c.mod", name: "Mod C" }]]]);
  });

  it("shows the shrink confirmation summary when scopeChange carries orphaned decisions", () => {
    installMockIpc({ list_mods: { total: 0, items: [] } });
    const wrapper = mountEditor({
      members: MEMBERS,
      scopeChange: {
        nowOrphaned: ["def_override:ThingDef/Wall:[a.mod,b.mod,c.mod]"],
        choicesNamingRemoved: [],
      },
    });

    expect(wrapper.get('[data-testid="scope-change-summary"]').text()).toContain(
      "def_override:ThingDef/Wall:[a.mod,b.mod,c.mod]",
    );
  });

  it("shows nothing when scopeChange is empty", () => {
    installMockIpc({ list_mods: { total: 0, items: [] } });
    const wrapper = mountEditor({
      members: MEMBERS,
      scopeChange: { nowOrphaned: [], choicesNamingRemoved: [] },
    });

    expect(wrapper.find('[data-testid="scope-change-summary"]').exists()).toBe(false);
  });

  it("disables the remove button at exactly two members with a title explaining the minimum", () => {
    installMockIpc({ list_mods: { total: 0, items: [] } });
    const wrapper = mountEditor({ members: MEMBERS });

    const removeButton = wrapper.get('[data-testid="scope-member-remove-a.mod"]');
    expect((removeButton.element as HTMLButtonElement).disabled).toBe(true);
    expect(removeButton.attributes("title")).toContain("at least two");
  });

  it("enables the remove button above two members, with no explanatory title", () => {
    installMockIpc({ list_mods: { total: 0, items: [] } });
    const wrapper = mountEditor({ members: [...MEMBERS, { modId: "c.mod", name: "Mod C" }] });

    const removeButton = wrapper.get('[data-testid="scope-member-remove-a.mod"]');
    expect((removeButton.element as HTMLButtonElement).disabled).toBe(false);
    expect(removeButton.attributes("title")).toBeUndefined();
  });

  it("focusSearch focuses the search box — the patch detail page's s shortcut", async () => {
    installMockIpc({ list_mods: { total: 0, items: [] } });
    const wrapper = mountEditor({ members: MEMBERS });

    (wrapper.vm as unknown as { focusSearch: () => void }).focusSearch();
    await wrapper.vm.$nextTick();

    expect(document.activeElement).toBe(wrapper.get('[data-testid="scope-search"]').element);
  });

  it("labels the search results list and marks the empty-state row as presentational, not a real result", async () => {
    installMockIpc({ list_mods: { total: 0, items: [] } });
    const wrapper = mountEditor({ members: MEMBERS });

    await wrapper.get('[data-testid="scope-search"]').setValue("zzz");
    await flush();
    await wrapper.vm.$nextTick();

    const results = wrapper.get('[data-testid="scope-search-results"]');
    expect(results.attributes("aria-label")).toBeTruthy();
    expect(results.get("li").attributes("role")).toBe("presentation");
  });
});
