import { afterEach, describe, expect, it, vi } from "vitest";
import { effectScope, nextTick, ref } from "vue";

import { useClampedText } from "@/composables/useClampedText";

function elementWithHeights(scrollHeight: number, clientHeight: number): HTMLElement {
  const element = document.createElement("p");
  Object.defineProperty(element, "scrollHeight", { value: scrollHeight, configurable: true });
  Object.defineProperty(element, "clientHeight", { value: clientHeight, configurable: true });
  return element;
}

function setup(element: HTMLElement | null) {
  const source = ref("text");
  const target = ref<HTMLElement | null>(null);
  const scope = effectScope();
  const clamped = scope.run(() => useClampedText(target, source));
  if (!clamped) {
    throw new Error("effect scope did not run");
  }
  target.value = element;
  return { source, target, clamped, scope };
}

describe("useClampedText", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("shows no toggle when the text fits", async () => {
    const { source, clamped, scope } = setup(elementWithHeights(100, 100));
    source.value = "other";
    await nextTick();

    expect(clamped.showToggle.value).toBe(false);
    scope.stop();
  });

  it("shows the toggle when the text overflows, and keeps it once expanded", async () => {
    const { source, clamped, scope } = setup(elementWithHeights(500, 100));
    source.value = "other";
    await nextTick();
    expect(clamped.showToggle.value).toBe(true);

    clamped.expanded.value = true;
    await nextTick();

    expect(clamped.showToggle.value).toBe(true);
    scope.stop();
  });

  it("re-measures when the text changes and collapses", async () => {
    const { source, target, clamped, scope } = setup(elementWithHeights(500, 100));
    source.value = "other";
    await nextTick();
    clamped.expanded.value = true;
    await nextTick();

    target.value = elementWithHeights(100, 100);
    source.value = "short";
    await nextTick();
    await nextTick();

    expect(clamped.expanded.value).toBe(false);
    expect(clamped.showToggle.value).toBe(false);
    scope.stop();
  });

  it("keeps the toggle when a resize re-measures while expanded", async () => {
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
    const heights = { scrollHeight: 500, clientHeight: 100 };
    const element = document.createElement("p");
    Object.defineProperty(element, "scrollHeight", { get: () => heights.scrollHeight });
    Object.defineProperty(element, "clientHeight", { get: () => heights.clientHeight });
    const { clamped, scope } = setup(element);
    await nextTick();
    callbacks.forEach((callback) => {
      callback();
    });
    expect(clamped.showToggle.value).toBe(true);

    clamped.expanded.value = true;
    await nextTick();
    heights.clientHeight = 500;
    callbacks.forEach((callback) => {
      callback();
    });
    await nextTick();

    expect(clamped.showToggle.value).toBe(true);
    scope.stop();
  });
});
