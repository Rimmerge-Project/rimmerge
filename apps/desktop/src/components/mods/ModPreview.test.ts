import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";

import ModPreview from "@/components/mods/ModPreview.vue";
import { createAppI18n, registerAppI18n } from "@/i18n/i18n";
import { installMockIpc } from "@/services/ipc.mock";
import type { ModPreviewDto } from "@/types/generated/ModPreviewDto";

registerAppI18n(createAppI18n());

function mountPreview(response: ModPreviewDto) {
  installMockIpc({ list_mod_names: {}, read_mod_preview: response });
  return mount(ModPreview, {
    props: { modId: "fixture.mod" },
    global: { plugins: [createPinia(), PiniaColada] },
  });
}

describe("ModPreview", () => {
  afterEach(() => {
    clearMocks();
  });

  it("renders the image once the query resolves with kind: image", async () => {
    const wrapper = mountPreview({
      kind: "image",
      dataUrl: "data:image/png;base64,AAAA",
      format: "png",
      bytes: 4,
    });

    await vi.waitFor(() => {
      expect(wrapper.find('[data-testid="mod-preview-image"]').exists()).toBe(true);
    });
    expect(wrapper.get('[data-testid="mod-preview-image"]').attributes("src")).toBe(
      "data:image/png;base64,AAAA",
    );
  });

  it("shows the no-preview message for kind: absent", async () => {
    const wrapper = mountPreview({ kind: "absent" });

    await vi.waitFor(() => {
      expect(wrapper.find('[data-testid="mod-preview-absent"]').exists()).toBe(true);
    });
  });

  it("shows a too-large message for an unreadable preview with reason: tooLarge", async () => {
    const wrapper = mountPreview({ kind: "unreadable", reason: "tooLarge" });

    await vi.waitFor(() => {
      expect(wrapper.find('[data-testid="mod-preview-unreadable"]').exists()).toBe(true);
    });
  });

  it("shows the loading state before the query resolves", () => {
    installMockIpc({
      list_mod_names: {},
      read_mod_preview: () => new Promise(() => {}),
    });
    const wrapper = mount(ModPreview, {
      props: { modId: "fixture.mod" },
      global: { plugins: [createPinia(), PiniaColada] },
    });

    expect(wrapper.find('[data-testid="mod-preview-loading"]').exists()).toBe(true);
  });
});
