import { PiniaColada, useQueryCache } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { nextTick, ref } from "vue";

import DefThumbnail from "@/components/graphics/DefThumbnail.vue";
import { SCROLL_CONTAINER_KEY } from "@/composables/scrollContainer";
import { queryKeys } from "@/queries/keys";
import { installMockIpc } from "@/services/ipc.mock";
import { asDefRef } from "@/types/brands";
import type { DefGraphicDto } from "@/types/generated/DefGraphicDto";
import { multiVariant, ONE_PIXEL_PNG, resolved, slot } from "@/utils/defGraphics.test-support";

const DEF = asDefRef("ThingDef/Wall");

/** Stands in for the browser's observer, which happy-dom lacks, so a test decides when the thumbnail is "on screen". */
class FakeIntersectionObserver {
  static instances: FakeIntersectionObserver[] = [];
  private tick = 0;
  private readonly callback: IntersectionObserverCallback;
  readonly options: IntersectionObserverInit | undefined;

  constructor(callback: IntersectionObserverCallback, options?: IntersectionObserverInit) {
    this.callback = callback;
    this.options = options;
    FakeIntersectionObserver.instances.push(this);
  }

  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }

  emit(isIntersecting: boolean): void {
    this.tick += 1;
    this.callback(
      [{ isIntersecting, time: this.tick } as IntersectionObserverEntry],
      this as unknown as IntersectionObserver,
    );
  }
}

function setShown(isIntersecting: boolean): void {
  for (const instance of FakeIntersectionObserver.instances) {
    instance.emit(isIntersecting);
  }
}

async function mountThumbnail(graphic: DefGraphicDto) {
  const calls: string[] = [];
  installMockIpc({
    list_mod_names: {},
    resolve_def_graphic: () => {
      calls.push("resolve");
      return graphic;
    },
    read_def_texture: (payload: unknown) => {
      calls.push((payload as { request: { textureKey: string } }).request.textureKey);
      return {
        kind: "image",
        dataUrl: ONE_PIXEL_PNG,
        format: "png",
        bytes: 68,
        owner: "a",
        from: "direct",
      };
    },
  });
  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(DefThumbnail, {
    props: { defRef: DEF },
    global: { plugins: [pinia, PiniaColada] },
  });
  // The visibility observer is created in a post-flush watcher.
  await nextTick();
  return { wrapper, calls, pinia };
}

describe("DefThumbnail", () => {
  beforeEach(() => {
    FakeIntersectionObserver.instances = [];
    vi.stubGlobal("IntersectionObserver", FakeIntersectionObserver);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    clearMocks();
  });

  it("measures its 200 px margin against the enclosing scroll container", async () => {
    const scroller = document.createElement("div");
    const pinia = createPinia();
    setActivePinia(pinia);
    installMockIpc({ list_mod_names: {} });
    mount(DefThumbnail, {
      props: { defRef: DEF },
      global: {
        plugins: [pinia, PiniaColada],
        provide: { [SCROLL_CONTAINER_KEY as symbol]: ref(scroller) },
      },
    });
    await nextTick();

    const [observer] = FakeIntersectionObserver.instances;
    expect(observer?.options?.root).toBe(scroller);
    expect(observer?.options?.rootMargin).toBe("200px");
  });

  it("asks the backend for nothing until it is near the viewport", async () => {
    const { wrapper, calls } = await mountThumbnail(
      resolved([slot([multiVariant("things/wall")])]),
    );
    await flushPromises();

    expect(calls).toEqual([]);
    expect(wrapper.find('[data-testid="def-texture-box"]').exists()).toBe(false);

    setShown(true);
    await flushPromises();

    expect(calls).toEqual(["resolve", "things/wall_south"]);
    expect(wrapper.find('[data-testid="def-texture-image"]').exists()).toBe(true);
  });

  it("is decorative: hidden from assistive technology, with an empty alt", async () => {
    const { wrapper } = await mountThumbnail(resolved([slot([multiVariant("things/wall")])]));
    setShown(true);
    await flushPromises();

    expect(wrapper.get('[data-testid="def-thumbnail"]').attributes("aria-hidden")).toBe("true");
    expect(wrapper.get('[data-testid="def-texture-image"]').attributes("alt")).toBe("");
  });

  it("shows a labelled glyph, not an image, for a def that shows nothing", async () => {
    const { wrapper, calls } = await mountThumbnail({ kind: "noGraphic" });
    setShown(true);
    await flushPromises();

    expect(wrapper.get('[data-testid="def-thumbnail-empty"]').attributes("title")).toBe(
      "This def shows no texture of its own.",
    );
    expect(calls).toEqual(["resolve"]);
  });

  it("releases its queries on leaving the viewport and drops the image 30 s later", async () => {
    const { wrapper, pinia } = await mountThumbnail(
      resolved([slot([multiVariant("things/wall")])]),
    );
    setShown(true);
    await flushPromises();
    const cache = useQueryCache(pinia);
    const key = queryKeys.defs.texture(DEF, "things/wall_south");
    expect(cache.getEntries({ key })).toHaveLength(1);

    vi.useFakeTimers();
    setShown(false);
    await nextTick();
    expect(wrapper.find('[data-testid="def-texture-box"]').exists()).toBe(false);

    vi.advanceTimersByTime(29_000);
    expect(cache.getEntries({ key })).toHaveLength(1);
    vi.advanceTimersByTime(2_000);
    expect(cache.getEntries({ key })).toHaveLength(0);
  });
});
