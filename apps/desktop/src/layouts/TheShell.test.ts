import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import TheShell from "@/layouts/TheShell.vue";
import { installMockIpc } from "@/services/ipc.mock";

const STUB = { template: "<div />" };

/**
 * `installMockIpc` (rather than leaving Tauri entirely unmocked) matters
 * here for two reasons: `shouldMockEvents` — `TheShell` unconditionally
 * calls `useTauriEvent`, whose `listen()` needs the event-plugin invoke
 * handled — and every command `TheShell`'s always-mounted children
 * actually invoke (`get_dashboard`/`get_merge_mod` from `ApplyDialog`,
 * `list_mod_names` from `useModLabel`) needs *some* registered fixture:
 * an unregistered command makes the mock throw, which — as a Pinia
 * Colada query's rejection, not a caller `.catch` — reaches Vitest as an
 * unhandled rejection that fails the whole run, not just this file (see
 * `list_mod_names`'s note in `InboxPage.test.ts`). `null` is fine for the
 * two queries nothing here reads. `get_pending_active_changes` is read by
 * `PendingChangesBanner.vue`/`PendingChangesCloseGuard.vue` (also always
 * mounted) — an all-empty diff keeps the banner hidden, matching the
 * "not stale" default every other spec assumes.
 */
function mountShell(openAppLinkCalls: string[] = []) {
  installMockIpc({
    list_mod_names: {},
    get_dashboard: null,
    get_merge_mod: null,
    get_pending_active_changes: {
      unscanned: { added: [], removed: [] },
      unapplied: { added: [], removed: [] },
    },
    open_app_link: (payload: unknown) => {
      openAppLinkCalls.push((payload as { target: string }).target);
      return null;
    },
  });
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", name: "dashboard", component: STUB },
      { path: "/inbox", name: "inbox", component: STUB },
      { path: "/merge-mod", name: "merge-mod", component: STUB },
      { path: "/patches", name: "patches", component: STUB },
      { path: "/order", name: "order", component: STUB },
      { path: "/rules", name: "rules", component: STUB },
      { path: "/mods", name: "mods", component: STUB },
      { path: "/settings", name: "settings", component: STUB },
    ],
  });

  return mount(TheShell, {
    attachTo: document.body,
    global: {
      plugins: [
        createPinia(),
        PiniaColada,
        router,
        [PrimeVue, { theme: { preset: Aura } }],
        ToastService,
      ],
      // `ApplyDialog` (always mounted, just hidden) renders a PrimeVue
      // `Dialog` through a real `<Teleport>`.
      stubs: { teleport: true },
    },
  });
}

function press(target: Element, key: string): void {
  target.dispatchEvent(new KeyboardEvent("keydown", { key, cancelable: true, bubbles: true }));
}

describe("TheShell", () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
    document.body.innerHTML = "";
  });

  it("defaults the mod-label-mode segmented control to Names", () => {
    const wrapper = mountShell();

    expect(wrapper.get('[data-testid="mod-label-mode-name"]').attributes("aria-checked")).toBe(
      "true",
    );
    expect(wrapper.get('[data-testid="mod-label-mode-id"]').attributes("aria-checked")).toBe(
      "false",
    );
  });

  it("clicking the Ids segment switches the mode, and back", async () => {
    const wrapper = mountShell();

    await wrapper.get('[data-testid="mod-label-mode-id"]').trigger("click");
    expect(wrapper.get('[data-testid="mod-label-mode-id"]').attributes("aria-checked")).toBe(
      "true",
    );
    expect(wrapper.get('[data-testid="mod-label-mode-name"]').attributes("aria-checked")).toBe(
      "false",
    );

    // Clicking the already-active segment is a no-op, not a second flip.
    await wrapper.get('[data-testid="mod-label-mode-id"]').trigger("click");
    expect(wrapper.get('[data-testid="mod-label-mode-id"]').attributes("aria-checked")).toBe(
      "true",
    );

    await wrapper.get('[data-testid="mod-label-mode-name"]').trigger("click");
    expect(wrapper.get('[data-testid="mod-label-mode-name"]').attributes("aria-checked")).toBe(
      "true",
    );
  });

  it("the t shortcut toggles the mode from anywhere in the shell", async () => {
    const wrapper = mountShell();

    press(wrapper.element, "t");
    await wrapper.vm.$nextTick();
    expect(wrapper.get('[data-testid="mod-label-mode-id"]').attributes("aria-checked")).toBe(
      "true",
    );

    press(wrapper.element, "t");
    await wrapper.vm.$nextTick();
    expect(wrapper.get('[data-testid="mod-label-mode-name"]').attributes("aria-checked")).toBe(
      "true",
    );
  });

  it("renders nav labels, the brand and the Apply button through i18n", () => {
    const wrapper = mountShell();

    expect(wrapper.get('[data-testid="nav-dashboard"]').text()).toBe("Dashboard");
    expect(wrapper.get('[data-testid="nav-inbox"]').text()).toBe("Findings");
    expect(wrapper.get('[data-testid="shell-apply-button"]').text()).toBe("Apply…");
    expect(wrapper.text()).toContain("Rimmerge");
  });

  it("highlights only the Dashboard nav item on / and nothing else once elsewhere", async () => {
    const wrapper = mountShell();
    await wrapper.vm.$router.isReady();

    expect(wrapper.get('[data-testid="nav-dashboard"]').classes()).toContain("bg-accent-soft");
    expect(wrapper.get('[data-testid="nav-inbox"]').classes()).not.toContain("bg-accent-soft");

    await wrapper.vm.$router.push("/inbox");
    await wrapper.vm.$nextTick();

    // Guards the case where a `RouterLink` to `/`
    // using non-exact `active-class` stays highlighted on every page,
    // since every child route's own matched chain includes the shell
    // layout's own `/` route record.
    expect(wrapper.get('[data-testid="nav-dashboard"]').classes()).not.toContain("bg-accent-soft");
    expect(wrapper.get('[data-testid="nav-inbox"]').classes()).toContain("bg-accent-soft");
  });

  it("clicking the sidebar's support link opens the app's own fixed support link", async () => {
    const openAppLinkCalls: string[] = [];
    const wrapper = mountShell(openAppLinkCalls);

    await wrapper.get('[data-testid="shell-support-link"]').trigger("click");

    expect(openAppLinkCalls).toEqual(["support"]);
  });

  it("the t shortcut is inert while a text input is focused", async () => {
    const wrapper = mountShell();
    const input = document.createElement("input");
    document.body.appendChild(input);
    input.focus();

    press(input, "t");
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="mod-label-mode-name"]').attributes("aria-checked")).toBe(
      "true",
    );
    input.remove();
  });
});
