import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";

import TextureTile from "@/components/inbox/TextureTile.vue";
import { installMockIpc } from "@/services/ipc.mock";

function mountTile(readTexture: () => unknown) {
  installMockIpc({ list_mod_names: {}, read_texture: readTexture });
  return mount(TextureTile, {
    props: {
      modId: "a.mod",
      texturePath: "things/wall",
      canPreferWinner: true,
      canShipAsset: true,
    },
    global: { plugins: [createPinia(), PiniaColada] },
  });
}

describe("TextureTile", () => {
  afterEach(() => {
    clearMocks();
  });

  it("renders a DDS refusal by its error code, not the backend's English message", async () => {
    const wrapper = mountTile(() => {
      throw {
        code: "texture_unsupported_format",
        message: "things/wall: not a supported texture format (expected PNG or JPEG)",
      };
    });
    await flushPromises();

    const text = wrapper.get('[data-testid="texture-unavailable"]').text();
    expect(text).toBe("Texture format can't be shown");
    expect(text).not.toContain("expected PNG or JPEG");
  });

  it("never shows the text of a rejection that did not come from the backend", async () => {
    const wrapper = mountTile(() => {
      throw new Error("channel closed");
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="texture-unavailable"]').text()).not.toContain(
      "channel closed",
    );
  });
});
