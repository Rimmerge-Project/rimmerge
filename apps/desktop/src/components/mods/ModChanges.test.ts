import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import ModChanges from "@/components/mods/ModChanges.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ChangePageDto } from "@/types/generated/ChangePageDto";
import type { ChangeRowDto } from "@/types/generated/ChangeRowDto";

const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: "/inbox", name: "inbox", component: { template: "<div />" } },
    { path: "/defs/:defRef", name: "def", component: { template: "<div />" } },
  ],
});

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  clearMocks();
  vi.useRealTimers();
});

function ownsRow(overrides: Partial<ChangeRowDto> = {}): ChangeRowDto {
  return {
    kind: "ownsDef",
    defRef: "ThingDef/Wall",
    assetPath: null,
    otherTouchers: 1,
    opCount: 0,
    findingKeys: [],
    ...overrides,
  };
}

function page(items: ChangeRowDto[], kindCounts: Partial<ChangePageDto["kindCounts"]> = {}) {
  return {
    total: items.length,
    items,
    kindCounts: {
      ownsDef: 0,
      ownsTemplate: 0,
      patchesDef: 0,
      overridesTexture: 0,
      overridesSound: 0,
      overridesKeyedTranslation: 0,
      ...kindCounts,
    },
  } satisfies ChangePageDto;
}

function mountChanges(handler: (payload: unknown) => ChangePageDto) {
  installMockIpc({ list_mod_changes: handler });
  return mount(ModChanges, {
    props: { modId: "owner.mod" },
    global: { plugins: [createPinia(), PiniaColada, router] },
  });
}

describe("ModChanges", () => {
  it("shows an empty message when nothing matches", async () => {
    const wrapper = mountChanges(() => page([]));
    await flushPromises();

    wrapper.get('[data-testid="mod-changes-empty"]');
  });

  it("renders a def row linking to the def page", async () => {
    const wrapper = mountChanges(() => page([ownsRow()], { ownsDef: 1 }));
    await flushPromises();

    const link = wrapper.get('[data-testid="mod-changes-row-ThingDef/Wall"] a');
    expect(link.text()).toBe("ThingDef/Wall");
    expect(link.attributes("href")).toBe("/defs/ThingDef%2FWall");
  });

  it("renders an asset row with no link", async () => {
    const row = ownsRow({
      kind: "overridesTexture",
      defRef: null,
      assetPath: "Textures/x.png",
    });
    const wrapper = mountChanges(() => page([row], { overridesTexture: 1 }));
    await flushPromises();

    const cell = wrapper.get('[data-testid="mod-changes-row-Textures/x.png"]');
    expect(cell.find("a").exists()).toBe(false);
    expect(cell.text()).toContain("Textures/x.png");
  });

  it("renders every finding this row names as its own pill, described and titled by its key", async () => {
    const row = ownsRow({
      findingKeys: ["def_override:ThingDef/Wall:[a,b]", "patch_collision:ThingDef/Wall:name:[c]"],
    });
    const wrapper = mountChanges(() => page([row], { ownsDef: 1 }));
    await flushPromises();

    const links = wrapper.findAll(
      '[data-testid="mod-changes-row-ThingDef/Wall"] a[href^="/inbox"]',
    );
    expect(links).toHaveLength(2);
    expect(links[0]?.text()).toBe("Def override");
    expect(links[0]?.attributes("title")).toBe("def_override:ThingDef/Wall:[a,b]");
    expect(links[0]?.attributes("href")).toContain("def_override");
    expect(links[1]?.text()).toBe("Patch collision");
    expect(links[1]?.attributes("title")).toBe("patch_collision:ThingDef/Wall:name:[c]");
  });

  it("shows an error, not the empty-list message, when the request fails", async () => {
    const wrapper = mountChanges(() => {
      throw { code: "scan_failed", message: "disk is full" };
    });
    await flushPromises();

    wrapper.get('[data-testid="mod-changes-error"]');
    expect(wrapper.find('[data-testid="mod-changes-empty"]').exists()).toBe(false);
  });

  it("toggling a kind chip narrows the request and shows the chip's own count", async () => {
    const requests: unknown[] = [];
    const wrapper = mountChanges((payload) => {
      requests.push(payload);
      return page([ownsRow()], { ownsDef: 3 });
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="mod-changes-chip-ownsDef"]').text()).toBe("Owns (3)");

    await wrapper.get('[data-testid="mod-changes-chip-ownsDef"]').trigger("click");
    await flushPromises();

    const last = requests.at(-1) as { filter: { kinds: string[] | null } };
    expect(last.filter.kinds).toEqual(["ownsDef"]);
  });

  it("debounces the search input instead of requesting on every keystroke", async () => {
    const requests: Array<{ filter: { search: string | null } }> = [];
    const wrapper = mountChanges((payload) => {
      requests.push(payload as { filter: { search: string | null } });
      return page([ownsRow()], { ownsDef: 1 });
    });
    await flushPromises();
    const requestCountBeforeTyping = requests.length;

    const search = wrapper.get('[data-testid="mod-changes-search"]');
    await search.setValue("w");
    await search.setValue("wa");
    await search.setValue("wall");
    // No new request until the debounce settles.
    expect(requests.length).toBe(requestCountBeforeTyping);

    await vi.advanceTimersByTimeAsync(200);
    await flushPromises();

    const searchRequests = requests.slice(requestCountBeforeTyping);
    expect(searchRequests).toHaveLength(1);
    expect(searchRequests[0]?.filter.search).toBe("wall");
  });

  it("shows more past the first page", async () => {
    const first = ownsRow({ defRef: "ThingDef/A" });
    const second = ownsRow({ defRef: "ThingDef/B" });
    const wrapper = mountChanges((payload) => {
      const { filter } = payload as { filter: { offset: number } };
      return filter.offset === 0
        ? { total: 2, items: [first], kindCounts: page([]).kindCounts }
        : { total: 2, items: [second], kindCounts: page([]).kindCounts };
    });
    await flushPromises();

    expect(wrapper.find('[data-testid="mod-changes-row-ThingDef/B"]').exists()).toBe(false);
    await wrapper.get('[data-testid="mod-changes-show-more"]').trigger("click");
    await flushPromises();

    wrapper.get('[data-testid="mod-changes-row-ThingDef/B"]');
  });
});
