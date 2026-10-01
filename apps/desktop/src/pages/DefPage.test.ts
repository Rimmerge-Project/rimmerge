import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import DefPage from "@/pages/DefPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { DefInspectionDto } from "@/types/generated/DefInspectionDto";
import type { InspectDefRequestDto } from "@/types/generated/InspectDefRequestDto";

afterEach(() => clearMocks());

function inspection(overrides: Partial<DefInspectionDto> = {}): DefInspectionDto {
  return {
    defRef: "ThingDef/Wall",
    source: "current",
    owners: [{ modId: "owner.mod", position: 0, isGenerated: false }],
    winner: "owner.mod",
    templateAmbiguity: null,
    patchers: [
      {
        modId: "patcher.mod",
        position: 1,
        isGenerated: false,
        ops: [
          {
            class: "PatchOperationReplace",
            xpath: 'Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints',
            subPath: null,
            findModContext: [],
            mayRequire: [],
            mayRequireAnyOf: [],
            locatorFile: "patcher_wall.xml",
            isWrapped: false,
          },
        ],
        replayError: null,
        reached: true,
        caveats: [],
      },
    ],
    parents: [],
    children: [],
    completeness: { kind: "complete" },
    caveats: [],
    fields: [
      {
        path: "label",
        depth: 0,
        isListItem: false,
        value: "a wall",
        provenance: { kind: "owner", modId: "owner.mod" },
      },
    ],
    fieldsTotal: 1,
    resolvedXml: "<ThingDef>\n  <label>a wall</label>\n</ThingDef>",
    findings: [{ key: "def_override:ThingDef/Wall:[owner.mod]", status: "auto" }],
    ...overrides,
  };
}

function mountPage(
  handler: (request: InspectDefRequestDto) => DefInspectionDto,
  fixtureOverrides: Record<string, unknown> = {},
) {
  installMockIpc({
    inspect_def: (payload: unknown) =>
      handler((payload as { request: InspectDefRequestDto }).request),
    list_mod_names: {},
    search_defs: () => [],
    select_order: {},
    ...fixtureOverrides,
  });

  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/mods", name: "mods", component: { template: "<div />" } },
      { path: "/inbox", name: "inbox", component: { template: "<div />" } },
      { path: "/defs/:defRef", name: "def", component: DefPage },
    ],
  });

  const pinia = createPinia();
  setActivePinia(pinia);
  return { router, pinia };
}

describe("DefPage", () => {
  it("renders the ref, owners, patchers, and effective fields", async () => {
    const { router, pinia } = mountPage(() => inspection());
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    expect(wrapper.get('[data-testid="def-page-ref"]').text()).toBe("ThingDef/Wall");
    expect(wrapper.get('[data-testid="def-page-owners"]').text()).toContain("owner.mod");
    expect(wrapper.get('[data-testid="def-page-patchers"]').text()).toContain("patcher.mod");
    expect(wrapper.get('[data-testid="def-page-fields"]').text()).toContain("a wall");
    expect(wrapper.find('[data-testid="def-page-partial-banner"]').exists()).toBe(false);

    wrapper.unmount();
  });

  it("shows the partial banner naming the stopper's mod and reason", async () => {
    const { router, pinia } = mountPage(() =>
      inspection({
        completeness: {
          kind: "partial",
          stoppedAt: {
            kind: "replay",
            modId: "patcher.mod",
            opIndex: 0,
            error: "unsupported xpath",
          },
        },
      }),
    );
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    const banner = wrapper.get('[data-testid="def-page-partial-banner"]');
    expect(banner.text()).toContain("patcher.mod");
    expect(banner.text()).toContain("unsupported xpath");

    wrapper.unmount();
  });

  it("expanding a patcher shows its operations", async () => {
    const { router, pinia } = mountPage(() => inspection());
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="def-page-patcher-ops-patcher.mod"]').exists()).toBe(false);
    await wrapper.get('[data-testid="def-page-patcher-toggle-patcher.mod"]').trigger("click");
    const ops = wrapper.get('[data-testid="def-page-patcher-ops-patcher.mod"]');
    expect(ops.text()).toContain("PatchOperationReplace");

    wrapper.unmount();
  });

  it("toggling only-patched requests a field-filtered page", async () => {
    const requests: InspectDefRequestDto[] = [];
    const { router, pinia } = mountPage((request) => {
      requests.push(request);
      return inspection();
    });
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    await wrapper.get('[data-testid="def-page-only-patched"]').setValue(true);
    await flushPromises();

    expect(requests.some((r) => r.filter.onlyPatched)).toBe(true);

    wrapper.unmount();
  });

  it("shows 'not reached' for a patcher the fold never got to, distinct from a real replay failure", async () => {
    const { router, pinia } = mountPage(() =>
      inspection({
        patchers: [
          {
            modId: "stopper.mod",
            position: 1,
            isGenerated: false,
            ops: [],
            replayError: "unsupported xpath",
            reached: true,
            caveats: [],
          },
          {
            modId: "later.mod",
            position: 2,
            isGenerated: false,
            ops: [],
            replayError: null,
            reached: false,
            caveats: [],
          },
        ],
      }),
    );
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    const toggles = wrapper.get('[data-testid="def-page-patchers"]');
    expect(toggles.text()).toContain("replay failed");
    expect(toggles.text()).toContain("not reached");

    wrapper.unmount();
  });

  it("lists a patcher's own caveats once expanded", async () => {
    const { router, pinia } = mountPage(() =>
      inspection({
        patchers: [
          {
            modId: "patcher.mod",
            position: 1,
            isGenerated: false,
            ops: [],
            replayError: null,
            reached: true,
            caveats: [{ kind: "failedOp", modId: "patcher.mod", xpath: "Defs/ThingDef" }],
          },
        ],
      }),
    );
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    await wrapper.get('[data-testid="def-page-patcher-toggle-patcher.mod"]').trigger("click");
    const ops = wrapper.get('[data-testid="def-page-patcher-ops-patcher.mod"]');
    expect(ops.text()).toContain("patcher.mod's patch operation on Defs/ThingDef matched nothing");

    wrapper.unmount();
  });

  it("clicking the source badge asks the backend to select the other order", async () => {
    const requests: unknown[] = [];
    const { router, pinia } = mountPage(() => inspection(), {
      select_order: (payload: unknown) => {
        requests.push(payload);
        return { movedMods: [] };
      },
    });
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    await wrapper.get('[data-testid="def-page-source"]').trigger("click");
    await flushPromises();

    expect(requests).toEqual([{ source: "suggested" }]);

    wrapper.unmount();
  });

  it("words the source badge's title with the lowercase order name", async () => {
    const { router, pinia } = mountPage(() => inspection());
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    const badge = wrapper.get('[data-testid="def-page-source"]');
    expect(badge.text()).toBe("Current");
    expect(badge.attributes("title")).toBe("Viewed under the current order — click to switch");

    wrapper.unmount();
  });

  it("Escape navigates back to wherever the page was reached from", async () => {
    const { router, pinia } = mountPage(() => inspection());
    await router.push({ name: "mods" });
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await flushPromises();

    expect(router.currentRoute.value.name).toBe("mods");

    wrapper.unmount();
  });

  it("links parents and children to their own def pages", async () => {
    const { router, pinia } = mountPage(() =>
      inspection({
        parents: [
          {
            defRef: "ThingDef/@Base",
            key: { defType: "ThingDef", defName: "Base" },
            owner: "owner.mod",
          },
        ],
        children: [
          {
            defRef: "ThingDef/Reinforced",
            key: { defType: "ThingDef", defName: "Reinforced" },
            owner: "child.mod",
          },
        ],
      }),
    );
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    expect(wrapper.get('[data-testid="def-page-parents"] a').attributes("href")).toBe(
      "/defs/ThingDef%2F@Base",
    );
    expect(wrapper.get('[data-testid="def-page-children"] a').attributes("href")).toBe(
      "/defs/ThingDef%2FReinforced",
    );

    wrapper.unmount();
  });

  it("shows each owner's position and marks the winner with its own test id", async () => {
    const { router, pinia } = mountPage(() =>
      inspection({
        owners: [
          { modId: "a.mod", position: 0, isGenerated: false },
          { modId: "owner.mod", position: 1, isGenerated: false },
        ],
        winner: "owner.mod",
      }),
    );
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    const owners = wrapper.get('[data-testid="def-page-owners"]');
    expect(owners.text()).toContain("#0");
    expect(owners.text()).toContain("#1");
    expect(wrapper.find('[data-testid="def-page-winner"]').exists()).toBe(true);

    wrapper.unmount();
  });

  it("shows the ambiguous-template banner and relabels the winner as a representative, not an answer", async () => {
    const { router, pinia } = mountPage(() =>
      inspection({
        owners: [
          { modId: "first.mod", position: 0, isGenerated: false },
          { modId: "second.mod", position: 1, isGenerated: false },
        ],
        winner: "second.mod",
        templateAmbiguity: {
          registrants: ["first.mod", "second.mod"],
          resolutions: { "child.mod": "second.mod" },
        },
      }),
    );
    await router.push({ name: "def", params: { defRef: "ThingDef/@Base" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    const banner = wrapper.get('[data-testid="def-page-template-ambiguity"]');
    expect(banner.text()).toContain("2 registrations");
    expect(banner.text()).toContain("first.mod");
    expect(banner.text()).toContain("second.mod");
    expect(banner.text()).toContain("child.mod");

    const winnerMarker = wrapper.get('[data-testid="def-page-winner"]');
    expect(winnerMarker.text()).toContain("last-loaded representative");
    expect(winnerMarker.text()).not.toContain("(winner)");

    wrapper.unmount();
  });

  it("shows no ambiguous-template banner and the plain winner marker for an unambiguous def", async () => {
    const { router, pinia } = mountPage(() => inspection());
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    expect(wrapper.find('[data-testid="def-page-template-ambiguity"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="def-page-winner"]').text()).toBe("(winner)");

    wrapper.unmount();
  });

  it("shows a provenance legend so color is never the only signal", async () => {
    const { router, pinia } = mountPage(() => inspection());
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    const legend = wrapper.get('[data-testid="def-page-provenance-legend"]');
    expect(legend.text()).toContain("Owner");
    expect(legend.text()).toContain("Patched");
    expect(legend.text()).toContain("Inherited");

    wrapper.unmount();
  });

  it("describes each finding pill and titles it with the finding's own key", async () => {
    const { router, pinia } = mountPage(() =>
      inspection({
        findings: [{ key: "def_override:ThingDef/Wall:[owner.mod]", status: "needsInput" }],
      }),
    );
    await router.push({ name: "def", params: { defRef: "ThingDef/Wall" } });
    await router.isReady();

    const wrapper = mount(
      { template: "<router-view />" },
      { global: { plugins: [pinia, PiniaColada, router] } },
    );
    await flushPromises();

    const pill = wrapper.get('[data-testid="def-page-findings"] a');
    expect(pill.text()).toContain("Def override");
    expect(pill.text()).toContain("Needs input");
    expect(pill.attributes("title")).toBe("def_override:ThingDef/Wall:[owner.mod]");

    wrapper.unmount();
  });
});
