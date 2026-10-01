import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import ModInfoPanel from "@/components/mods/ModInfoPanel.vue";
import { createAppI18n, registerAppI18n } from "@/i18n/i18n";
import { type IpcFixtures, installMockIpc } from "@/services/ipc.mock";
import type { ModInfoDto } from "@/types/generated/ModInfoDto";

registerAppI18n(createAppI18n());

const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: "/", name: "dashboard", component: { template: "<div />" } },
    { path: "/order/:modId?", name: "order-mod", component: { template: "<div />" } },
    { path: "/inbox", name: "inbox", component: { template: "<div />" } },
    { path: "/startup", name: "startup", component: { template: "<div />" } },
    { path: "/mods/:modId/details", name: "mod-detail-page", component: { template: "<div />" } },
  ],
});

const declared = {
  loadAfter: [],
  loadBefore: [],
  forceLoadAfter: [],
  forceLoadBefore: [],
  dependencies: [],
  incompatibleWith: [],
};

function activeModInfo(
  overrides: Partial<Extract<ModInfoDto, { kind: "active" }>> = {},
): ModInfoDto {
  return {
    kind: "active",
    modId: "fixture.mod",
    name: "Fixture Mod",
    authors: [],
    homepage: null,
    source: "local",
    supportedVersions: [],
    supportsGameVersion: true,
    declared,
    root: "Mods/fixture",
    loadedFolders: [],
    cost: null,
    tags: [],
    hardDependents: 0,
    softDependents: 0,
    awarenessDependents: 0,
    isFrameworkCandidate: false,
    generated: null,
    workshopId: null,
    position: 0,
    previousPosition: null,
    tier: "body",
    findingsTotal: 0,
    needsInputCount: 0,
    pending: null,
    about: { kind: "notOnDisk" },
    ...overrides,
  };
}

function defaultFixtures(overrides: IpcFixtures = {}): IpcFixtures {
  return {
    list_mod_names: {},
    read_mod_preview: { kind: "absent" },
    read_mod_icon: { kind: "absent" },
    ...overrides,
  };
}

async function mountPanel(modId: string | null, fixtures: IpcFixtures) {
  installMockIpc(defaultFixtures(fixtures));
  const wrapper = mount(ModInfoPanel, {
    props: { modId },
    global: { plugins: [createPinia(), PiniaColada, router] },
  });
  await router.isReady();
  await flushPromises();
  return wrapper;
}

describe("ModInfoPanel", () => {
  afterEach(() => {
    clearMocks();
  });

  it("renders nothing while modId is null", async () => {
    const wrapper = await mountPanel(null, {});

    expect(wrapper.find('[data-testid="mod-info-panel"]').exists()).toBe(false);
  });

  it("shows a not-found message for a mod_not_found error", async () => {
    const wrapper = await mountPanel("fixture.mod", {
      get_mod_info: () => {
        throw { code: "mod_not_found", message: "no such mod" };
      },
    });

    expect(wrapper.find('[data-testid="mod-info-panel-not-found"]').exists()).toBe(true);
  });

  it("shows the missing-mod header and its dependents for a missing mod", async () => {
    const wrapper = await mountPanel("fixture.mod", {
      get_mod_info: {
        kind: "missing",
        modId: "fixture.mod",
        requiredBy: ["dependent.mod"],
      } satisfies ModInfoDto,
    });

    expect(wrapper.find('[data-testid="mod-info-panel-missing-header"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="mod-info-panel"]').text()).toContain("dependent.mod");
  });

  it("renders an active mod's name, id and pending state", async () => {
    const wrapper = await mountPanel("fixture.mod", {
      get_mod_info: activeModInfo({ pending: "activationPending" }),
    });

    expect(wrapper.get('[data-testid="mod-info-name"]').text()).toBe("Fixture Mod");
    expect(wrapper.find('[data-testid="mod-info-pending"]').exists()).toBe(true);
  });

  it("clicking the close button emits close", async () => {
    const wrapper = await mountPanel("fixture.mod", { get_mod_info: activeModInfo() });

    await wrapper.get('[data-testid="mod-info-panel-close"]').trigger("click");

    expect(wrapper.emitted("close")).toHaveLength(1);
  });

  it("pressing Escape inside the panel emits close", async () => {
    const wrapper = await mountPanel("fixture.mod", { get_mod_info: activeModInfo() });

    await wrapper.get('[data-testid="mod-info-panel"]').trigger("keydown", { key: "Escape" });

    expect(wrapper.emitted("close")).toHaveLength(1);
  });

  it("clicking the workshop link opens the mod's own workshop link, never a URL the frontend builds", async () => {
    const calls: { modId: string; link: string }[] = [];
    const wrapper = await mountPanel("fixture.mod", {
      get_mod_info: activeModInfo({ workshopId: 12345 }),
      open_mod_link: (payload: unknown) => {
        calls.push(payload as { modId: string; link: string });
        return null;
      },
    });

    await wrapper.get('[data-testid="mod-info-workshop-link"]').trigger("click");

    expect(calls).toEqual([{ modId: "fixture.mod", link: "workshop" }]);
  });

  it("clicking an openable homepage link opens it through the same backend path", async () => {
    const calls: { modId: string; link: string }[] = [];
    const wrapper = await mountPanel("fixture.mod", {
      get_mod_info: activeModInfo({ homepage: { kind: "openable", url: "https://example.com" } }),
      open_mod_link: (payload: unknown) => {
        calls.push(payload as { modId: string; link: string });
        return null;
      },
    });

    await wrapper.get('[data-testid="mod-info-homepage-link"]').trigger("click");

    expect(calls).toEqual([{ modId: "fixture.mod", link: "homepage" }]);
  });

  it("shows a non-openable homepage as plain text, with no clickable link", async () => {
    const wrapper = await mountPanel("fixture.mod", {
      get_mod_info: activeModInfo({
        homepage: { kind: "text", text: "steam://url/CommunityFilePage/1" },
      }),
    });

    expect(wrapper.find('[data-testid="mod-info-homepage-link"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="mod-info-panel"]').text()).toContain(
      "steam://url/CommunityFilePage/1",
    );
  });

  it("the all-details link is shown for an active mod and navigates to the full page route", async () => {
    const wrapper = await mountPanel("fixture.mod", { get_mod_info: activeModInfo() });

    const link = wrapper.get('[data-testid="mod-info-all-details-link"]');
    await link.trigger("click");
    await flushPromises();

    expect(router.currentRoute.value.name).toBe("mod-detail-page");
    expect(router.currentRoute.value.params["modId"]).toBe("fixture.mod");
  });

  it("no v-html anywhere renders the description — a hostile literal string in it stays inert text", async () => {
    const wrapper = await mountPanel("fixture.mod", {
      get_mod_info: activeModInfo({
        about: {
          kind: "read",
          modVersion: null,
          modIconPath: null,
          description: {
            runs: [{ text: "<img src=x onerror=alert(1)>", bold: false, italic: false }],
            truncated: false,
          },
          homepage: null,
        },
      }),
    });

    expect(wrapper.find("img[src='x']").exists()).toBe(false);
    expect(wrapper.get('[data-testid="mod-description-text"]').text()).toContain(
      "<img src=x onerror=alert(1)>",
    );
  });

  it("renders a non-mod_not_found failure through describeCommandError, never the backend's own raw English message", async () => {
    const wrapper = await mountPanel("fixture.mod", {
      get_mod_info: () => {
        throw { code: "internal", message: "raw backend diagnostic, never shown directly" };
      },
    });

    const text = wrapper.get('[data-testid="mod-info-panel-error"]').text();
    expect(text).toBe("Something went wrong on Rimmerge's own side.");
    expect(text).not.toContain("raw backend diagnostic");
  });

  it("shows a message under the Description heading when About.xml is not on disk", async () => {
    const wrapper = await mountPanel("fixture.mod", {
      get_mod_info: activeModInfo({ about: { kind: "notOnDisk" } }),
    });

    expect(wrapper.find('[data-testid="mod-info-about-not-on-disk"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="mod-description"]').exists()).toBe(false);
  });

  it("shows an unreadable About.xml's headline and its technical detail on separate lines", async () => {
    const wrapper = await mountPanel("fixture.mod", {
      get_mod_info: activeModInfo({
        about: { kind: "unreadable", cause: "io", detail: "access is denied" },
      }),
    });

    const headline = wrapper.get('[data-testid="mod-info-about-unreadable"]').text();
    expect(headline).toContain("Couldn't read About.xml: the file could not be read");
    expect(wrapper.get('[data-testid="mod-info-about-unreadable-detail"]').text()).toBe(
      "Technical details: access is denied",
    );
    expect(headline).not.toContain("(access is denied)");
  });

  it("waits until the query resolves before showing loaded content", () => {
    installMockIpc(
      defaultFixtures({
        get_mod_info: () => new Promise(() => {}),
      }),
    );
    const wrapper = mount(ModInfoPanel, {
      props: { modId: "fixture.mod" },
      global: { plugins: [createPinia(), PiniaColada, router] },
    });

    expect(wrapper.find('[data-testid="mod-info-panel-loading"]').exists()).toBe(true);
  });
});
