import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import { createAppI18n, loadLocale } from "@/i18n/i18n";

import SetupPage from "@/pages/SetupPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { DefaultPathsDto } from "@/types/generated/DefaultPathsDto";
import type { ProjectSummaryDto } from "@/types/generated/ProjectSummaryDto";

// `primevue/usetoast` mocked so the migration-warning test below can
// assert the exact toast payload `SetupPage.vue` builds, not just that
// navigation still succeeds around it — `useToast()` must return the same
// spy-carrying object on every call, so the spy is declared via
// `vi.hoisted` (a plain closure variable wouldn't survive Vitest's mock
// hoisting).
const { toastAddSpy } = vi.hoisted(() => ({ toastAddSpy: vi.fn() }));
vi.mock("primevue/usetoast", () => ({
  useToast: () => ({ add: toastAddSpy }),
}));

// The native folder dialog, answered per test.
const { pickFolderMock } = vi.hoisted(() => ({ pickFolderMock: vi.fn() }));
vi.mock("@/services/dialogs", () => ({ pickFolder: pickFolderMock }));

const defaultsFixture: DefaultPathsDto = {
  gameDir: "C:/RimWorld",
  workshopDir: "C:/RimWorld/workshop",
  modsConfig: "C:/Profile/ModsConfig.xml",
  profileDir: "C:/Profile/rimmerge",
  warningCode: null,
  errorCode: null,
};

const summaryFixture: ProjectSummaryDto = {
  modCount: 3,
  gameVersion: "1.6.4871",
  elapsedMs: 120,
  warnings: [],
  ruleWarnings: [],
  selected: "suggested",
};

// `submit()` fires `run_launch_network_checks` fire-and-forget right
// after a successful load — every fixture below that also carries
// `load_project` needs this registered too, or the mock throws on an
// unmatched command (an unhandled rejection this test file never awaits,
// since the call is intentionally voided).
const runLaunchNetworkChecksFixture = {
  updateCheck: { kind: "skipped", reason: "awaitingFirstRun" },
  databaseRefresh: [],
};

const noInstallFixture: DefaultPathsDto = {
  gameDir: null,
  workshopDir: null,
  modsConfig: "C:/Profile/ModsConfig.xml",
  profileDir: "C:/Profile/rimmerge",
  warningCode: null,
  errorCode: {
    kind: "gameDirNotFound",
    candidates: [
      "Q:\\Steam\\steamapps\\common\\RimWorld",
      "W:\\SteamGames\\steamapps\\common\\RimWorld",
    ],
  },
};

async function settle(wrapper: { vm: { $nextTick: () => Promise<void> } }): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
}

function mountSetupPage(i18nOverride?: ReturnType<typeof createAppI18n>) {
  const pinia = createPinia();
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/setup", name: "setup", component: SetupPage },
      { path: "/", name: "dashboard", component: { template: "<div>dashboard</div>" } },
    ],
  });

  const wrapper = mount(SetupPage, {
    global: {
      plugins: [
        pinia,
        router,
        PiniaColada,
        [PrimeVue, { theme: { preset: Aura } }],
        ToastService,
        ...(i18nOverride ? [i18nOverride] : []),
      ],
    },
  });

  return { wrapper, pinia, router };
}

describe("SetupPage", () => {
  afterEach(() => {
    clearMocks();
    toastAddSpy.mockClear();
    pickFolderMock.mockReset();
  });

  it("prefills the folder inputs from get_default_paths", async () => {
    installMockIpc({ get_default_paths: defaultsFixture });

    const { wrapper } = mountSetupPage();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect((wrapper.get('[data-testid="game-dir-input"]').element as HTMLInputElement).value).toBe(
      defaultsFixture.gameDir,
    );
    expect(
      (wrapper.get('[data-testid="mods-config-input"]').element as HTMLInputElement).value,
    ).toBe(defaultsFixture.modsConfig);
  });

  it("loads the project and navigates to the dashboard on success", async () => {
    installMockIpc({
      get_default_paths: defaultsFixture,
      run_launch_network_checks: runLaunchNetworkChecksFixture,
      load_project: summaryFixture,
    });

    const { wrapper, pinia, router } = mountSetupPage();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="load-project-button"]').trigger("click");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();
    await router.isReady();

    const session = useSessionStore(pinia);
    expect(session.loaded).toBe(true);
    expect(session.selected).toBe("suggested");
    expect(router.currentRoute.value.name).toBe("dashboard");
  });

  it("still navigates to the dashboard when the load succeeds with rule warnings, after toasting them", async () => {
    // `rim_session::Session::rule_load_warnings` (a `rules.json` migration
    // warning) would otherwise be silently dropped once `submit` navigates
    // straight past this page — this pins that reading `summary.ruleWarnings`
    // and building the toast doesn't itself throw and block the navigation.
    installMockIpc({
      get_default_paths: defaultsFixture,
      run_launch_network_checks: runLaunchNetworkChecksFixture,
      load_project: {
        ...summaryFixture,
        ruleWarnings: [{ kind: "droppedClusterRules", ruleIds: ["#0"] }],
      },
    });

    const { wrapper, pinia, router } = mountSetupPage();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="load-project-button"]').trigger("click");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();
    await router.isReady();

    const session = useSessionStore(pinia);
    expect(session.loaded).toBe(true);
    expect(router.currentRoute.value.name).toBe("dashboard");
    expect(toastAddSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        severity: "warn",
        detail: expect.stringContaining("cluster"),
      }),
    );
  });

  it("shows a single 'Scan finished with N notes' toast and stores the notes, without one toast per note", async () => {
    installMockIpc({
      get_default_paths: defaultsFixture,
      run_launch_network_checks: runLaunchNetworkChecksFixture,
      load_project: {
        ...summaryFixture,
        warnings: [
          { modId: "some.mod", message: "Languages/English/Keyed/keys.xml: skipped: bad xml" },
          { modId: "other.mod", message: "Defs/Buildings.xml: skipped: bad xml" },
        ],
      },
    });

    const { wrapper, pinia, router } = mountSetupPage();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="load-project-button"]').trigger("click");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();
    await router.isReady();

    expect(toastAddSpy).toHaveBeenCalledTimes(1);
    expect(toastAddSpy).toHaveBeenCalledWith(
      expect.objectContaining({ severity: "info", summary: "Scan finished with 2 notes" }),
    );
    const session = useSessionStore(pinia);
    expect(session.scanNotes).toHaveLength(2);
  });

  it("shows no toast when the scan has no notes", async () => {
    installMockIpc({
      get_default_paths: defaultsFixture,
      run_launch_network_checks: runLaunchNetworkChecksFixture,
      load_project: summaryFixture,
    });

    const { wrapper, pinia, router } = mountSetupPage();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="load-project-button"]').trigger("click");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();
    await router.isReady();

    expect(toastAddSpy).not.toHaveBeenCalled();
    const session = useSessionStore(pinia);
    expect(session.scanNotes).toEqual([]);
  });

  it("auto-reloads from the known paths when landing with ?autoReload=1 after a caught backend panic", async () => {
    // Simulates `main.ts`'s own session-lost handler: a project was
    // loaded, then `markLostForReload()` marked it unloaded while keeping
    // the paths — the same state a caught backend panic leaves the store
    // in, whatever page the user happened to be on when it happened.
    installMockIpc({
      get_default_paths: defaultsFixture,
      run_launch_network_checks: runLaunchNetworkChecksFixture,
      load_project: summaryFixture,
    });
    const lostPaths = {
      gameDir: "C:/PreviousGame",
      workshopDir: "C:/PreviousWorkshop",
      modsConfig: "C:/PreviousProfile/ModsConfig.xml",
      profileDir: "C:/PreviousProfile",
    };
    const pinia = createPinia();
    const session = useSessionStore(pinia);
    session.setLoaded(lostPaths, "current");
    session.markLostForReload();
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: "/setup", name: "setup", component: SetupPage },
        { path: "/", name: "dashboard", component: { template: "<div>dashboard</div>" } },
      ],
    });
    await router.push({ name: "setup", query: { autoReload: "1" } });

    const wrapper = mount(SetupPage, {
      global: {
        plugins: [
          pinia,
          router,
          PiniaColada,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
      },
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();
    await router.isReady();

    // Reloaded from the *previous* paths, never `get_default_paths`'s —
    // an auto-reload must never silently substitute a different install.
    expect((wrapper.get('[data-testid="game-dir-input"]').element as HTMLInputElement).value).toBe(
      lostPaths.gameDir,
    );
    expect(session.loaded).toBe(true);
    expect(router.currentRoute.value.name).toBe("dashboard");
  });

  it("does nothing extra when ?autoReload=1 is present but nothing was ever loaded", async () => {
    // Defensive case `main.ts` never actually produces (it only sets the
    // query flag when `markLostForReload` returned real paths) — pinned
    // anyway so a future caller of this page can't silently auto-submit
    // an empty form.
    installMockIpc({ get_default_paths: defaultsFixture });
    const pinia = createPinia();
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: "/setup", name: "setup", component: SetupPage },
        { path: "/", name: "dashboard", component: { template: "<div>dashboard</div>" } },
      ],
    });
    await router.push({ name: "setup", query: { autoReload: "1" } });

    const wrapper = mount(SetupPage, {
      global: {
        plugins: [
          pinia,
          router,
          PiniaColada,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
      },
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();
    await router.isReady();

    expect(router.currentRoute.value.name).toBe("setup");
    expect(wrapper.find('[data-testid="setup-progress"]').exists()).toBe(false);
  });

  // SPEC CHANGE: the banner used to print the backend's English CLI text
  // (`error`, with `--game-dir` hints). `get_default_paths` now sends
  // `errorCode` only; the page words it and lists the checked locations.
  it("shows the no-install-found error in the page's own words, each checked location as a code element, and leaves the game folder empty", async () => {
    installMockIpc({ get_default_paths: noInstallFixture });

    const { wrapper } = mountSetupPage();
    await settle(wrapper);

    const banner = wrapper.get('[data-testid="setup-detection-error"]');
    const locations = banner.findAll("li code").map((code) => code.text());
    expect(locations).toEqual([
      "Q:\\Steam\\steamapps\\common\\RimWorld",
      "W:\\SteamGames\\steamapps\\common\\RimWorld",
    ]);
    expect(banner.text()).toContain("Locations checked:");
    expect(banner.text()).not.toContain("--game-dir");
    expect((wrapper.get('[data-testid="game-dir-input"]').element as HTMLInputElement).value).toBe(
      "",
    );
    // ModsConfig and the profile do not depend on the install, so they fill anyway.
    expect(
      (wrapper.get('[data-testid="mods-config-input"]').element as HTMLInputElement).value,
    ).toBe("C:/Profile/ModsConfig.xml");
    expect(wrapper.get('[data-testid="load-project-button"]').attributes("disabled")).toBeDefined();
  });

  it("renders gameDirNotFound in the active locale with each checked location as a code element and no English backend sentence", async () => {
    installMockIpc({ get_default_paths: noInstallFixture });
    const i18n = createAppI18n();
    await loadLocale(i18n, "pt-BR");
    i18n.global.locale.value = "pt-BR";

    const { wrapper } = mountSetupPage(i18n);
    await settle(wrapper);

    const banner = wrapper.get('[data-testid="setup-detection-error"]');
    expect(banner.findAll("li code")).toHaveLength(2);
    expect(banner.text()).toContain("Q:\\Steam\\steamapps\\common\\RimWorld");
    expect(banner.text()).not.toContain("no RimWorld install found");
    expect(banner.text()).not.toContain("--game-dir");
    expect(banner.text()).not.toContain("technical details");
  });

  it("clears the detection error and fills workshop, ModsConfig and profile once the user browses to a game folder", async () => {
    const requests: unknown[] = [];
    installMockIpc({
      get_default_paths: (payload: unknown) => {
        const { request } = payload as { request: { gameDir: string | null } };
        requests.push(request);
        if (request.gameDir === null) {
          return noInstallFixture;
        }
        return {
          gameDir: request.gameDir,
          workshopDir: "D:/Steam/steamapps/workshop/content/294100",
          modsConfig: "C:/Profile/ModsConfig.xml",
          profileDir: "C:/Profile/rimmerge",
          warningCode: null,
          errorCode: null,
        } satisfies DefaultPathsDto;
      },
    });
    pickFolderMock.mockResolvedValue("D:/Steam/steamapps/common/RimWorld");

    const { wrapper } = mountSetupPage();
    await settle(wrapper);
    expect(wrapper.find('[data-testid="setup-detection-error"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="load-project-button"]').attributes("disabled")).toBeDefined();

    await wrapper
      .findAll("button")
      .find((button) => button.text() === "Browse…")
      ?.trigger("click");
    await settle(wrapper);
    await settle(wrapper);

    expect(requests).toEqual([
      { gameDir: null },
      { gameDir: "D:/Steam/steamapps/common/RimWorld" },
    ]);
    expect(wrapper.find('[data-testid="setup-detection-error"]').exists()).toBe(false);
    expect((wrapper.get('[data-testid="game-dir-input"]').element as HTMLInputElement).value).toBe(
      "D:/Steam/steamapps/common/RimWorld",
    );
    expect(
      (wrapper.get('[data-testid="workshop-dir-input"]').element as HTMLInputElement).value,
    ).toBe("D:/Steam/steamapps/workshop/content/294100");
    expect(
      (wrapper.get('[data-testid="profile-dir-input"]').element as HTMLInputElement).value,
    ).toBe("C:/Profile/rimmerge");
    expect(
      wrapper.get('[data-testid="load-project-button"]').attributes("disabled"),
    ).toBeUndefined();
  });

  it("keeps the checked-locations list while the user types a game folder, until the answer is committed", async () => {
    installMockIpc({
      get_default_paths: (payload: unknown) => {
        const { request } = payload as { request: { gameDir: string | null } };
        return request.gameDir === null ? noInstallFixture : defaultsFixture;
      },
    });

    const { wrapper } = mountSetupPage();
    await settle(wrapper);
    await wrapper.get('[data-testid="game-dir-input"]').setValue("Q:\\Steam\\steamapps\\common");

    expect(wrapper.findAll('[data-testid="setup-detection-error"] li code')).toHaveLength(2);

    await wrapper.get('[data-testid="game-dir-input"]').trigger("blur");
    await settle(wrapper);
    await settle(wrapper);

    expect(wrapper.find('[data-testid="setup-detection-error"]').exists()).toBe(false);
  });

  it("keeps a workshop folder the user changed when the game folder is committed", async () => {
    installMockIpc({
      get_default_paths: (payload: unknown) => {
        const { request } = payload as { request: { gameDir: string | null } };
        return {
          ...defaultsFixture,
          gameDir: request.gameDir ?? defaultsFixture.gameDir,
          workshopDir: request.gameDir === null ? defaultsFixture.workshopDir : "E:/derived",
        };
      },
    });

    const { wrapper } = mountSetupPage();
    await settle(wrapper);
    await wrapper.get('[data-testid="workshop-dir-input"]').setValue("F:/my-workshop");
    await wrapper.get('[data-testid="game-dir-input"]').setValue("G:/Elsewhere");
    await wrapper.get('[data-testid="game-dir-input"]').trigger("blur");
    await settle(wrapper);
    await settle(wrapper);

    expect(
      (wrapper.get('[data-testid="workshop-dir-input"]').element as HTMLInputElement).value,
    ).toBe("F:/my-workshop");
  });

  it("shows the non-fatal warning beside the prefilled fields, without blocking Load", async () => {
    // A pinned or env-supplied install directory that isn't one: the
    // fields still fill in (the drive may just not be mounted yet) and
    // Load stays enabled — the scan that follows says far more about why
    // it failed than a refusal here could.
    installMockIpc({
      get_default_paths: {
        ...defaultsFixture,
        warningCode: { kind: "notAnInstall", gameDir: "C:/RimWorld" },
      },
    });

    const { wrapper } = mountSetupPage();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="setup-path-warning"]').text()).toContain(
      "does not look like a RimWorld install",
    );
    expect(
      wrapper.get('[data-testid="load-project-button"]').attributes("disabled"),
    ).toBeUndefined();
  });

  it("drops the warning once the user edits the game directory it was about", async () => {
    installMockIpc({
      get_default_paths: {
        ...defaultsFixture,
        warningCode: { kind: "notAnInstall", gameDir: "C:/RimWorld" },
      },
    });

    const { wrapper } = mountSetupPage();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();
    expect(wrapper.find('[data-testid="setup-path-warning"]').exists()).toBe(true);

    await wrapper.get('[data-testid="game-dir-input"]').setValue("C:/Somewhere/Else");
    await wrapper.vm.$nextTick();

    expect(
      wrapper.find('[data-testid="setup-path-warning"]').exists(),
      "the warning is about the resolved path, not whatever is in the box now",
    ).toBe(false);
  });

  it("hides the detection message when the paths resolved", async () => {
    installMockIpc({ get_default_paths: defaultsFixture });

    const { wrapper } = mountSetupPage();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="setup-detection-error"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="setup-path-warning"]').exists()).toBe(false);
  });

  it("saves the three install paths as defaults, never the derived profile directory", async () => {
    const saveCalls: unknown[] = [];
    installMockIpc({
      get_default_paths: defaultsFixture,
      save_app_config: (payload: unknown) => {
        saveCalls.push(payload);
        return null;
      },
    });

    const { wrapper } = mountSetupPage();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="save-defaults-button"]').trigger("click");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(saveCalls).toEqual([
      {
        config: {
          gameDir: defaultsFixture.gameDir,
          workshopDir: defaultsFixture.workshopDir,
          modsConfig: defaultsFixture.modsConfig,
        },
      },
    ]);
    expect(toastAddSpy).toHaveBeenCalledWith(
      expect.objectContaining({ severity: "success", summary: "Saved as defaults" }),
    );
  });

  it("surfaces a failure to save the defaults instead of claiming success", async () => {
    installMockIpc({
      get_default_paths: defaultsFixture,
      save_app_config: () => {
        throw { code: "internal", message: "LOCALAPPDATA is not set" };
      },
    });

    const { wrapper } = mountSetupPage();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="save-defaults-button"]').trigger("click");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="setup-error"]').text()).toContain("LOCALAPPDATA is not set");
    expect(toastAddSpy).not.toHaveBeenCalledWith(
      expect.objectContaining({ summary: "Saved as defaults" }),
    );
  });

  it("shows an error message when loading the project fails", async () => {
    installMockIpc({
      get_default_paths: defaultsFixture,
      run_launch_network_checks: runLaunchNetworkChecksFixture,
      load_project: () => {
        throw { code: "scan_failed", message: "the game directory does not exist" };
      },
    });

    const { wrapper } = mountSetupPage();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="load-project-button"]').trigger("click");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="setup-error"]').text()).toContain(
      "the game directory does not exist",
    );
  });
});
