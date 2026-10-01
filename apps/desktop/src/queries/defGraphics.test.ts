import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { defineComponent, ref } from "vue";

import { useDefGraphicQuery, useDefTextureQuery } from "@/queries/defGraphics";
import { installMockIpc } from "@/services/ipc.mock";
import { asDefRef, type DefRef } from "@/types/brands";
import type { DefGraphicDto } from "@/types/generated/DefGraphicDto";
import type { DefTextureDto } from "@/types/generated/DefTextureDto";

const DEF = asDefRef("ThingDef/Wall");

function mountHarness(
  defRef: DefRef | null,
  textureKey: string | null,
  handlers: Record<string, (payload: unknown) => unknown>,
) {
  installMockIpc(handlers);
  const defRefState = ref<DefRef | null>(defRef);
  const textureKeyState = ref<string | null>(textureKey);
  let graphic!: ReturnType<typeof useDefGraphicQuery>;
  let texture!: ReturnType<typeof useDefTextureQuery>;
  const Harness = defineComponent({
    setup() {
      graphic = useDefGraphicQuery(defRefState);
      texture = useDefTextureQuery(defRefState, textureKeyState);
      return {};
    },
    template: "<div />",
  });
  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(Harness, { global: { plugins: [pinia, PiniaColada] } });
  return {
    wrapper,
    defRefState,
    get graphic() {
      return graphic;
    },
    get texture() {
      return texture;
    },
  };
}

describe("def graphic queries", () => {
  afterEach(() => {
    clearMocks();
  });

  it("sends the def ref and texture key in the request payload and returns the answers", async () => {
    const requests: unknown[] = [];
    const graphicAnswer: DefGraphicDto = { kind: "noGraphic" };
    const textureAnswer: DefTextureDto = { kind: "notFound" };
    const { graphic, texture, wrapper } = mountHarness(DEF, "things/wall", {
      resolve_def_graphic: (payload) => {
        requests.push(payload);
        return graphicAnswer;
      },
      read_def_texture: (payload) => {
        requests.push(payload);
        return textureAnswer;
      },
    });

    await flushPromises();

    expect(graphic.data.value).toEqual(graphicAnswer);
    expect(texture.data.value).toEqual(textureAnswer);
    expect(requests).toEqual([
      { request: { defRef: DEF } },
      { request: { defRef: DEF, textureKey: "things/wall" } },
    ]);
    wrapper.unmount();
  });

  it("stays idle, calling nothing, while the def ref is null", async () => {
    const calls: string[] = [];
    const { wrapper } = mountHarness(null, "things/wall", {
      resolve_def_graphic: () => {
        calls.push("resolve");
        return { kind: "noGraphic" };
      },
      read_def_texture: () => {
        calls.push("read");
        return { kind: "notFound" };
      },
    });

    await flushPromises();

    expect(calls).toEqual([]);
    wrapper.unmount();
  });

  it("does not read a texture until a key is chosen", async () => {
    const calls: string[] = [];
    const { wrapper } = mountHarness(DEF, null, {
      resolve_def_graphic: () => ({ kind: "noGraphic" }),
      read_def_texture: () => {
        calls.push("read");
        return { kind: "notFound" };
      },
    });

    await flushPromises();

    expect(calls).toEqual([]);
    wrapper.unmount();
  });
});
