import { afterEach, describe, expect, it } from "vitest";
import { ref } from "vue";

import { useTreeRowKeys } from "@/composables/useTreeRowKeys";
import type { TreeRow } from "@/utils/groupedEntries";

/** Only `key` and `kind` matter to the focus wait; the rest of a row is never read by it. */
const ROWS = ["m:1:a", "m:1:b", "m:1:far"].map((key) => ({ key, kind: "mod" })) as TreeRow[];

function rowElement(key: string): HTMLElement {
  const element = document.createElement("div");
  element.tabIndex = -1;
  element.dataset["entryKey"] = key;
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
  return useTreeRowKeys({
    rows: () => ROWS,
    focusedKey: ref<string | null>(null),
    hasSelection: () => false,
    hasModMenu: () => false,
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
      setFolded: noop,
      openMenu: noop,
      focusMod: noop,
    },
  });
}

function focusedKey(): string | undefined {
  return (document.activeElement as HTMLElement | null)?.dataset["entryKey"];
}

describe("useTreeRowKeys focusKey", () => {
  afterEach(() => {
    document.body.replaceChildren();
  });

  it("focuses a far row that renders a few frames after the scroll", async () => {
    const scroller = document.createElement("div");
    document.body.append(scroller);
    const { focusKey } = setup(scroller);

    const pending = focusKey("m:1:far");
    await frames(3);
    scroller.append(rowElement("m:1:far"));
    await pending;

    expect(focusedKey()).toBe("m:1:far");
  });

  it("lets a newer call win: the older call's row, rendered late, is never focused", async () => {
    const scroller = document.createElement("div");
    document.body.append(scroller);
    scroller.append(rowElement("m:1:b"));
    const { focusKey } = setup(scroller);

    const older = focusKey("m:1:a");
    const newer = focusKey("m:1:b");
    await newer;
    scroller.append(rowElement("m:1:a"));
    await older;
    await frames(3);

    expect(focusedKey()).toBe("m:1:b");
  });

  it("a call for a key that is not shown still supersedes an older waiting call", async () => {
    const scroller = document.createElement("div");
    document.body.append(scroller);
    const { focusKey } = setup(scroller);

    const older = focusKey("m:1:a");
    await focusKey("m:9:gone");
    scroller.append(rowElement("m:1:a"));
    await older;
    await frames(3);

    expect(focusedKey()).not.toBe("m:1:a");
  });
});
