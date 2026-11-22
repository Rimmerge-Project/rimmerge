import { afterEach, describe, expect, it } from "vitest";
import { effectScope } from "vue";

import { useFocusRequests } from "@/composables/useFocusRequests";

function frames(count: number): Promise<void> {
  return new Promise((resolve) => {
    let left = count;
    const step = (): void => {
      left -= 1;
      if (left <= 0) {
        resolve();
        return;
      }
      requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  });
}

function rowElement(): HTMLElement {
  const element = document.createElement("div");
  element.tabIndex = -1;
  return element;
}

describe("useFocusRequests", () => {
  afterEach(() => {
    document.body.replaceChildren();
  });

  it("gives up after its bounded wait when the row never renders", async () => {
    const { claim, focusWhenRendered } = useFocusRequests();
    let lookups = 0;

    await focusWhenRendered(claim(), () => {
      lookups += 1;
      return undefined;
    });

    // The first look plus one per retry frame: bounded, and it resolved instead of hanging.
    expect(lookups).toBeGreaterThan(1);
    expect(lookups).toBeLessThan(100);
    expect(document.activeElement).toBe(document.body);
  });

  it("focuses the element once it appears", async () => {
    const { claim, focusWhenRendered } = useFocusRequests();
    const element = rowElement();
    let isRendered = false;

    const pending = focusWhenRendered(claim(), () => (isRendered ? element : undefined));
    await frames(2);
    document.body.append(element);
    isRendered = true;
    await pending;

    expect(document.activeElement).toBe(element);
  });

  it("cancels a waiting request when its scope is disposed", async () => {
    const scope = effectScope();
    const requests = scope.run(() => useFocusRequests());
    if (requests === undefined) throw new Error("effect scope inactive");
    const { claim, focusWhenRendered } = requests;
    const element = rowElement();
    document.body.append(element);
    let isRendered = false;

    const pending = focusWhenRendered(claim(), () => (isRendered ? element : undefined));
    scope.stop();
    isRendered = true;
    await pending;
    await frames(2);

    expect(document.activeElement).not.toBe(element);
  });
});
