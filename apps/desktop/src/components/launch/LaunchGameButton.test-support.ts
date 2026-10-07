// The mount helper and a scriptable IPC backend shared by the LaunchGameButton test files.

import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { defineComponent, h } from "vue";
import { createMemoryHistory, createRouter } from "vue-router";
import {
  dashboardFixture,
  EMPTY_PENDING_ACTIVE_CHANGES,
  EMPTY_RULE_SET,
  emptyMergeModFixture,
  preflightFixture,
} from "@/components/apply/ApplyDialog.test-support";
import LaunchGameButton from "@/components/launch/LaunchGameButton.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useGameLaunchStore } from "@/stores/gameLaunch";
import { useSessionStore } from "@/stores/session";
import type { ApplyRequestDto } from "@/types/generated/ApplyRequestDto";
import type { GameLaunchedDto } from "@/types/generated/GameLaunchedDto";
import type { GameLaunchStatusDto } from "@/types/generated/GameLaunchStatusDto";
import type { LaunchGameRequestDto } from "@/types/generated/LaunchGameRequestDto";

export const READY: GameLaunchStatusDto = { kind: "ready", route: "steam" };
export const NEEDS_APPLY: GameLaunchStatusDto = {
  kind: "needsApply",
  route: "steam",
  reason: "orderDiffers",
};

type RejectedCommand = { code: string; message: string };

/** What the fake backend answers and what it was asked. Mutate it to move the game state. */
export type LaunchBackend = {
  status: GameLaunchStatusDto;
  /** When set, `get_game_launch_status` rejects with this (a failed status call). */
  statusError: RejectedCommand | null;
  /** When set, `launch_game` rejects with this. */
  launchError: RejectedCommand | null;
  /** When set, `get_game_launch_status` waits for it, like a call queued behind a verify. */
  statusGate: Promise<void> | null;
  /** When set, `launch_game` waits for it. */
  launchGate: Promise<void> | null;
  /** Runs inside `launch_game`, before it answers or rejects (the order moving mid-launch). */
  onLaunch: (() => void) | null;
  statusCalls: number;
  launchCalls: LaunchGameRequestDto[];
  applyCalls: ApplyRequestDto[];
};

export function newBackend(status: GameLaunchStatusDto = READY): LaunchBackend {
  return {
    status,
    statusError: null,
    launchError: null,
    statusGate: null,
    launchGate: null,
    onLaunch: null,
    statusCalls: 0,
    launchCalls: [],
    applyCalls: [],
  };
}

/** Installs mock IPC for the button, its prompt and the `ApplyDialog` it owns. */
export function installBackend(backend: LaunchBackend): void {
  installMockIpc({
    list_rules: EMPTY_RULE_SET,
    list_mod_names: {},
    get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
    get_dashboard: dashboardFixture(0),
    get_apply_preflight: preflightFixture(),
    get_merge_mod: emptyMergeModFixture(),
    get_def_cache_carrier: { carrierModId: null },
    list_order: [],
    get_game_launch_status: async (): Promise<GameLaunchStatusDto> => {
      backend.statusCalls += 1;
      await backend.statusGate;
      if (backend.statusError) {
        throw backend.statusError;
      }
      return backend.status;
    },
    launch_game: async (payload: unknown): Promise<GameLaunchedDto> => {
      backend.launchCalls.push((payload as { request: LaunchGameRequestDto }).request);
      await backend.launchGate;
      backend.onLaunch?.();
      if (backend.launchError) {
        throw backend.launchError;
      }
      return { route: "steam" };
    },
    apply: (payload: unknown) => {
      backend.applyCalls.push((payload as { request: ApplyRequestDto }).request);
      return {
        modsConfigPath: "C:/RimWorld/ModsConfig.xml",
        backupPath: null,
        decisionsPath: "C:/Profile/rimmerge/decisions.json",
        rulesPath: "C:/Profile/rimmerge/rules.json",
        mergeModPath: null,
        mergeModBackupPath: null,
        skippedMerges: [],
      };
    },
  });
}

/** Everything mounted since the last {@link resetLaunchState}, so a test cannot leak listeners. */
const mounted: ReturnType<typeof mount>[] = [];

function mountWith(component: Parameters<typeof mount>[0], props: Record<string, unknown>) {
  const pinia = createPinia();
  setActivePinia(pinia);
  useSessionStore().setLoaded(
    {
      gameDir: "C:/RimWorld",
      workshopDir: "C:/RimWorld/workshop",
      modsConfig: "C:/RimWorld/ModsConfig.xml",
      profileDir: "C:/Profile/rimmerge",
    },
    "current",
  );
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: { render: () => null } },
      { path: "/inbox", component: { render: () => null } },
      { path: "/merge/:key", name: "merge-editor", component: { render: () => null } },
    ],
  });
  const wrapper = mount(component, {
    props,
    attachTo: document.body,
    global: {
      plugins: [pinia, router, PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      stubs: { teleport: true },
    },
  });
  mounted.push(wrapper);
  return wrapper;
}

export function mountLaunchButton(variant: "sidebar" | "strip" = "sidebar") {
  return mountWith(LaunchGameButton, { variant });
}

/** Both placements at once, as in the app (sidebar plus the Dashboard strip), on one Pinia. */
export function mountBothLaunchButtons() {
  const Both = defineComponent({
    render: () => [
      h(LaunchGameButton, { variant: "sidebar" }),
      h(LaunchGameButton, { variant: "strip" }),
    ],
  });
  return mountWith(Both, {});
}

/**
 * Unmounts every mounted button (their window and visibility listeners would otherwise answer
 * the next test's events) and ends the shared Starting window and its timer.
 */
export function resetLaunchState(): void {
  useGameLaunchStore().endStarting();
  for (const wrapper of mounted.splice(0)) {
    wrapper.unmount();
  }
}
