import { afterEach, describe, expect, it, vi } from "vitest";
import { effectScope, nextTick, ref } from "vue";

import { useCollapseToFit } from "@/composables/useCollapseToFit";

function elementWithWidths(widths: { offsetWidth?: number; clientWidth?: number }): HTMLElement {
  const element = document.createElement("div");
  Object.defineProperty(element, "offsetWidth", { get: () => widths.offsetWidth ?? 0 });
  Object.defineProperty(element, "clientWidth", { get: () => widths.clientWidth ?? 0 });
  return element;
}

/** Captures the resize callbacks the composable registers, so a test can fire them. */
function stubResizeObserver(): Array<() => void> {
  const callbacks: Array<() => void> = [];
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(callback: () => void) {
        callbacks.push(callback);
      }
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    },
  );
  return callbacks;
}

function setup(containerWidth: { clientWidth: number }, contentWidth: { offsetWidth: number }) {
  const container = ref<HTMLElement | null>(null);
  const content = ref<HTMLElement | null>(null);
  const scope = effectScope();
  const isCollapsed = scope.run(() => useCollapseToFit(container, content));
  if (!isCollapsed) {
    throw new Error("effect scope did not run");
  }
  container.value = elementWithWidths(containerWidth);
  content.value = elementWithWidths(contentWidth);
  return { isCollapsed, scope };
}

describe("useCollapseToFit", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("stays expanded while the content fits", async () => {
    const callbacks = stubResizeObserver();
    const { isCollapsed, scope } = setup({ clientWidth: 400 }, { offsetWidth: 380 });
    await nextTick();
    callbacks.forEach((callback) => {
      callback();
    });

    expect(isCollapsed.value).toBe(false);
    scope.stop();
  });

  it("collapses when the content is wider than the container", async () => {
    const callbacks = stubResizeObserver();
    const { isCollapsed, scope } = setup({ clientWidth: 376 }, { offsetWidth: 528 });
    await nextTick();
    callbacks.forEach((callback) => {
      callback();
    });

    expect(isCollapsed.value).toBe(true);
    scope.stop();
  });

  it("absorbs a pixel of rounding", async () => {
    const callbacks = stubResizeObserver();
    const { isCollapsed, scope } = setup({ clientWidth: 400 }, { offsetWidth: 401 });
    await nextTick();
    callbacks.forEach((callback) => {
      callback();
    });

    expect(isCollapsed.value).toBe(false);
    scope.stop();
  });

  it("expands again when the container grows", async () => {
    const callbacks = stubResizeObserver();
    const widths = { clientWidth: 300 };
    const container = ref<HTMLElement | null>(elementWithWidths(widths));
    const content = ref<HTMLElement | null>(elementWithWidths({ offsetWidth: 500 }));
    const scope = effectScope();
    const isCollapsed = scope.run(() => useCollapseToFit(container, content));
    if (!isCollapsed) {
      throw new Error("effect scope did not run");
    }
    await nextTick();
    callbacks.forEach((callback) => {
      callback();
    });
    expect(isCollapsed.value).toBe(true);

    widths.clientWidth = 600;
    callbacks.forEach((callback) => {
      callback();
    });

    expect(isCollapsed.value).toBe(false);
    scope.stop();
  });
});
