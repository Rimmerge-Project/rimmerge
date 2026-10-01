import { PiniaColada, useQueryCache } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import DefGraphicViewer from "@/components/graphics/DefGraphicViewer.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { asDefRef } from "@/types/brands";
import type { DefGraphicDto } from "@/types/generated/DefGraphicDto";
import type { DefTextureDto } from "@/types/generated/DefTextureDto";
import {
  bodyTypeVariants,
  face,
  loose,
  multiVariant,
  ONE_PIXEL_PNG,
  resolved,
  singleVariant,
  slot,
} from "@/utils/defGraphics.test-support";

const DEF = asDefRef("ThingDef/Wall");

const IMAGE: DefTextureDto = {
  kind: "image",
  dataUrl: ONE_PIXEL_PNG,
  format: "png",
  bytes: 68,
  owner: "owner.mod",
  from: "direct",
};

type Harness = {
  wrapper: VueWrapper;
  reads: string[];
  /** Invalidates every query, as `session://changed` does, so the graphic is fetched again. */
  refetch: () => Promise<void>;
};

let mounted: VueWrapper[] = [];

function mountViewer(
  graphic: DefGraphicDto | (() => DefGraphicDto),
  options: {
    defOwner?: string | null;
    texture?: DefTextureDto | ((key: string) => DefTextureDto);
    failResolve?: { code: string; message: string };
  } = {},
): Harness {
  const reads: string[] = [];
  installMockIpc({
    list_mod_names: {},
    resolve_def_graphic: () => {
      if (options.failResolve) {
        throw options.failResolve;
      }
      return typeof graphic === "function" ? graphic() : graphic;
    },
    read_def_texture: (payload: unknown) => {
      const { textureKey } = (payload as { request: { textureKey: string } }).request;
      reads.push(textureKey);
      const texture = options.texture ?? IMAGE;
      return typeof texture === "function" ? texture(textureKey) : texture;
    },
  });
  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(DefGraphicViewer, {
    props: { defRef: DEF, defOwner: options.defOwner ?? null },
    attachTo: document.body,
    global: {
      plugins: [pinia, PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
  mounted.push(wrapper);
  const refetch = async (): Promise<void> => {
    await useQueryCache(pinia).invalidateQueries();
    await flushPromises();
  };
  return { wrapper, reads, refetch };
}

const checked = (wrapper: VueWrapper): string | undefined =>
  wrapper
    .findAll('[role="radio"]')
    .find((button) => button.attributes("aria-checked") === "true")
    ?.attributes("data-facing");

describe("DefGraphicViewer", () => {
  afterEach(() => {
    for (const wrapper of mounted) {
      wrapper.unmount();
    }
    mounted = [];
    clearMocks();
  });

  it("shows the south face of a directional graphic first", async () => {
    const { wrapper, reads } = mountViewer(resolved([slot([multiVariant("things/chair")])]));
    await flushPromises();

    expect(reads).toEqual(["things/chair_south"]);
    expect(checked(wrapper)).toBe("south");
    expect(wrapper.get('[data-testid="def-texture-image"]').attributes("alt")).toBe(
      "ThingDef/Wall, South view",
    );
  });

  it("is a labelled radiogroup with one tab stop, and arrow keys move and select with wrap", async () => {
    const { wrapper, reads } = mountViewer(resolved([slot([multiVariant("things/chair")])]));
    await flushPromises();
    const group = wrapper.get('[role="radiogroup"]');
    expect(group.attributes("aria-label")).toBe("Facing");
    expect(
      wrapper.findAll('[role="radio"]').filter((button) => button.attributes("tabindex") === "0"),
    ).toHaveLength(1);

    await wrapper.get('[data-facing="south"]').trigger("keydown", { key: "ArrowRight" });
    await flushPromises();
    expect(checked(wrapper)).toBe("west");
    expect(document.activeElement?.getAttribute("data-facing")).toBe("west");

    await wrapper.get('[data-facing="west"]').trigger("keydown", { key: "ArrowRight" });
    await flushPromises();
    expect(checked(wrapper)).toBe("north");

    await wrapper.get('[data-facing="north"]').trigger("keydown", { key: "ArrowLeft" });
    await flushPromises();
    expect(checked(wrapper)).toBe("west");
    expect(reads).toContain("things/chair_north");
  });

  it("requests the clicked direction's own texture key", async () => {
    const { wrapper, reads } = mountViewer(resolved([slot([multiVariant("things/chair")])]));
    await flushPromises();

    await wrapper.get('[data-testid="def-graphic-facing-east"]').trigger("click");
    await flushPromises();

    expect(reads.at(-1)).toBe("things/chair_east");
  });

  it("flags a mirrored direction and flips its image", async () => {
    const { wrapper } = mountViewer(resolved([slot([multiVariant("things/chair")])]));
    await flushPromises();
    expect(wrapper.get('[data-testid="def-graphic-mirrored"]').text()).toBe("");

    await wrapper.get('[data-testid="def-graphic-facing-west"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="def-graphic-mirrored"]').text()).toBe("Drawn mirrored");
    expect(wrapper.get('[data-testid="def-texture-image"]').classes()).toContain("-scale-x-100");
  });

  it("hides the facing control for a single graphic", async () => {
    const { wrapper } = mountViewer(resolved([slot([singleVariant("things/rock")])]));
    await flushPromises();

    expect(wrapper.find('[role="radiogroup"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="def-texture-image"]').exists()).toBe(true);
  });

  it("steps through variants with wrap-around and announces the position politely", async () => {
    const { wrapper, reads } = mountViewer(resolved([slot(bodyTypeVariants(3))]));
    await flushPromises();
    const label = wrapper.get('[data-testid="def-graphic-variant-label"]');
    expect(label.attributes("aria-live")).toBe("polite");
    expect(label.text()).toBe("Body type: Body0 (1 of 3)");

    await wrapper.get('[data-testid="def-graphic-variant-prev"]').trigger("click");
    await flushPromises();
    expect(label.text()).toBe("Body type: Body2 (3 of 3)");
    expect(reads.at(-1)).toBe("things/body2");

    await wrapper.get('[data-testid="def-graphic-variant-next"]').trigger("click");
    await flushPromises();
    expect(label.text()).toBe("Body type: Body0 (1 of 3)");
  });

  it("gives the stepper buttons accessible names", async () => {
    const { wrapper } = mountViewer(resolved([slot(bodyTypeVariants(2))]));
    await flushPromises();

    expect(wrapper.get('[data-testid="def-graphic-variant-prev"]').attributes("aria-label")).toBe(
      "Previous variant",
    );
    expect(wrapper.get('[data-testid="def-graphic-variant-next"]').attributes("aria-label")).toBe(
      "Next variant",
    );
  });

  it("turns the variant control into a dropdown past eight variants", async () => {
    const { wrapper } = mountViewer(resolved([slot(bodyTypeVariants(9))]));
    await flushPromises();

    expect(wrapper.find('[data-testid="def-graphic-variant-select"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="def-graphic-variant-prev"]').exists()).toBe(false);
  });

  it("keeps the stepper at exactly eight variants", async () => {
    const { wrapper } = mountViewer(resolved([slot(bodyTypeVariants(8))]));
    await flushPromises();

    expect(wrapper.find('[data-testid="def-graphic-variant-prev"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="def-graphic-variant-select"]').exists()).toBe(false);
  });

  it("shows the slot dropdown only when there is more than one slot", async () => {
    const one = mountViewer(resolved([slot([singleVariant("a")])]));
    await flushPromises();
    expect(one.wrapper.find('[data-testid="def-graphic-slot"]').exists()).toBe(false);

    const two = mountViewer(
      resolved([
        slot([singleVariant("a")]),
        slot([singleVariant("b")], { source: { kind: "icon" } }),
      ]),
    );
    await flushPromises();
    expect(two.wrapper.find('[data-testid="def-graphic-slot"]').exists()).toBe(true);
  });

  it("names the providing mod, and flags it only when it is not the def's owner", async () => {
    const graphic = resolved([
      slot([
        {
          label: { kind: "only" },
          faces: {
            kind: "single",
            face: face("things/rock", { availability: loose("retexture.mod") }),
          },
          isLocated: true,
        },
      ]),
    ]);
    const different = mountViewer(graphic, { defOwner: "base.mod" });
    await flushPromises();
    expect(different.wrapper.get('[data-testid="def-graphic-provider"]').text()).toBe(
      "From retexture.mod",
    );
    expect(different.wrapper.get('[data-testid="def-graphic-override"]').text()).toContain(
      "base.mod",
    );

    const same = mountViewer(graphic, { defOwner: "retexture.mod" });
    await flushPromises();
    expect(same.wrapper.find('[data-testid="def-graphic-override"]').exists()).toBe(false);

    const unknown = mountViewer(graphic, { defOwner: null });
    await flushPromises();
    expect(unknown.wrapper.find('[data-testid="def-graphic-override"]').exists()).toBe(false);
  });

  it("says plainly that a def with no graphic shows nothing", async () => {
    const { wrapper, reads } = mountViewer({ kind: "noGraphic" });
    await flushPromises();

    expect(wrapper.get('[data-testid="def-graphic-empty"]').text()).toBe(
      "This def shows no texture of its own.",
    );
    expect(reads).toEqual([]);
  });

  it("says a humanlike race is assembled in game", async () => {
    const { wrapper } = mountViewer({ kind: "composedAtRuntime" });
    await flushPromises();

    expect(wrapper.get('[data-testid="def-graphic-empty"]').text()).toContain(
      "assembles this pawn",
    );
  });

  it("words a texture that cannot be shown, in a sentence, with no image", async () => {
    const { wrapper } = mountViewer(resolved([slot([singleVariant("a")])]), {
      texture: { kind: "notViewable", isUncertain: false },
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="def-texture-message"]').text()).toContain("asset bundle");
    expect(wrapper.find('[data-testid="def-texture-image"]').exists()).toBe(false);
  });

  it("describes a failed resolve by its error code, never the raw message", async () => {
    const { wrapper } = mountViewer(resolved([]), {
      failResolve: { code: "def_not_found", message: "RAW ENGLISH MESSAGE" },
    });
    await flushPromises();

    const message = wrapper.get('[data-testid="def-texture-error-message"]').text();
    expect(message).not.toContain("RAW ENGLISH MESSAGE");
    expect(message.length).toBeGreaterThan(0);
  });

  it("goes back to the default view when the def changes", async () => {
    const { wrapper } = mountViewer(resolved([slot([multiVariant("things/chair")])]));
    await flushPromises();
    await wrapper.get('[data-testid="def-graphic-facing-north"]').trigger("click");
    expect(checked(wrapper)).toBe("north");

    await wrapper.setProps({ defRef: asDefRef("ThingDef/Other") });
    await flushPromises();

    expect(checked(wrapper)).toBe("south");
  });

  it("keeps the chosen facing when the graphic is refetched unchanged", async () => {
    let fetches = 0;
    const { wrapper, refetch } = mountViewer(() => {
      fetches += 1;
      return resolved([slot([multiVariant("things/chair")])]);
    });
    await flushPromises();
    await wrapper.get('[data-testid="def-graphic-facing-north"]').trigger("click");

    await refetch();

    expect(fetches).toBe(2);
    expect(checked(wrapper)).toBe("north");
  });
});
