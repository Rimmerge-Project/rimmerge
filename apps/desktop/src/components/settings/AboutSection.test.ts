import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";

import AboutSection from "@/components/settings/AboutSection.vue";
import { createAppI18n, registerAppI18n } from "@/i18n/i18n";
import { installMockIpc } from "@/services/ipc.mock";
import type { AppLinkTargetDto } from "@/types/generated/AppLinkTargetDto";

function mountAboutSection(openAppLinkCalls: AppLinkTargetDto[] = []) {
  registerAppI18n(createAppI18n());
  installMockIpc({
    get_app_version: "1.2.3",
    open_app_link: (payload: unknown) => {
      openAppLinkCalls.push((payload as { target: AppLinkTargetDto }).target);
      return null;
    },
  });
  return mount(AboutSection, {
    global: { plugins: [createPinia(), PiniaColada] },
  });
}

describe("AboutSection", () => {
  afterEach(() => {
    clearMocks();
    vi.restoreAllMocks();
  });

  it("renders the app's own version once the query resolves", async () => {
    const wrapper = mountAboutSection();
    await vi.waitFor(() => {
      expect(wrapper.get('[data-testid="settings-about-version"]').text()).toContain("1.2.3");
    });
  });

  it("clicking each link opens the app's own fixed target, never a URL the frontend builds", async () => {
    const openAppLinkCalls: AppLinkTargetDto[] = [];
    const wrapper = mountAboutSection(openAppLinkCalls);

    await wrapper.get('[data-testid="settings-about-repo-link"]').trigger("click");
    await wrapper.get('[data-testid="settings-about-issues-link"]').trigger("click");
    await wrapper.get('[data-testid="settings-about-support-link"]').trigger("click");

    await vi.waitFor(() => {
      expect(openAppLinkCalls).toEqual(["githubRepo", "githubIssues", "support"]);
    });
  });
});
