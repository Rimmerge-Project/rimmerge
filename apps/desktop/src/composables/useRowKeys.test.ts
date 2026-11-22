import { afterEach, describe, expect, it } from "vitest";
import { ref } from "vue";

import { useRowKeys } from "@/composables/useRowKeys";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";

const ROWS = ["a", "b", "c"].map((modId) => ({ modId })) as unknown as ModListRowDto[];

function rowElement(modId: string): HTMLElement {
  const element = document.createElement("div");
  element.tabIndex = -1;
  element.dataset["testid"] = `mod-row-${modId}`;
  return element;
}

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

function setup(scroller: HTMLElement) {
  const noop = (): void => undefined;
  return useRowKeys({
    mods: () => ROWS,
    focusedId: ref<string | null>(null),
    isSelectable: () => false,
    hasSelection: () => false,
    rowMenu: () => "move",
    hasRowMenu: () => false,
    scrollToIndex: noop,
    getScroller: () => scroller,
    actions: {
      open: noop,
      toggleSelected: noop,
      setAnchor: noop,
      selectRange: noop,
      extendSelection: noop,
      selectAll: noop,
      clearSelection: noop,
      moveStep: noop,
      openMenu: noop,
      focusMod: noop,
    },
  });
}

describe("useRowKeys focusRow", () => {
  afterEach(() => {
    document.body.replaceChildren();
  });

  it("focuses a row that renders a few frames after the scroll", async () => {
    const scroller = document.createElement("div");
    document.body.append(scroller);
    const { focusRow } = setup(scroller);

    const pending = focusRow("c");
    await frames(3);
    scroller.append(rowElement("c"));
    await pending;

    expect(document.activeElement?.getAttribute("data-testid")).toBe("mod-row-c");
  });

  it("lets a newer call win: the older call's row, rendered late, is never focused", async () => {
    const scroller = document.createElement("div");
    document.body.append(scroller);
    scroller.append(rowElement("b"));
    const { focusRow } = setup(scroller);

    const older = focusRow("a");
    const newer = focusRow("b");
    await newer;
    scroller.append(rowElement("a"));
    await older;
    await frames(3);

    expect(document.activeElement?.getAttribute("data-testid")).toBe("mod-row-b");
  });

  it("a call for a mod that is not in the list still supersedes an older waiting call", async () => {
    const scroller = document.createElement("div");
    document.body.append(scroller);
    const { focusRow } = setup(scroller);

    const older = focusRow("a");
    await focusRow("gone");
    scroller.append(rowElement("a"));
    await older;
    await frames(3);

    expect(document.activeElement?.getAttribute("data-testid")).not.toBe("mod-row-a");
  });
});
