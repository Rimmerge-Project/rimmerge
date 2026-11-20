import { afterEach, describe, expect, it, vi } from "vitest";

import { markFirstMenuItem } from "@/composables/popupMenuFocus";

/** A focusable stand-in for PrimeVue's popup menu: the overlay and the list inside it. */
function mountMenu(): { overlay: HTMLElement; list: HTMLElement } {
  const overlay = document.createElement("div");
  overlay.setAttribute("data-pc-name", "menu");
  const list = document.createElement("ul");
  list.tabIndex = 0;
  overlay.append(list);
  document.body.append(overlay);
  return { overlay, list };
}

afterEach(() => {
  document.body.replaceChildren();
});

describe("markFirstMenuItem", () => {
  it("sends ArrowDown to the focused menu list and stops it below the page", () => {
    const { list } = mountMenu();
    const onList = vi.fn();
    const onWindow = vi.fn();
    list.addEventListener("keydown", onList);
    window.addEventListener("keydown", onWindow);
    list.focus();

    markFirstMenuItem();

    window.removeEventListener("keydown", onWindow);
    expect(onList).toHaveBeenCalledTimes(1);
    expect(onList.mock.calls[0]?.[0]).toMatchObject({ key: "ArrowDown", code: "ArrowDown" });
    expect(onWindow).not.toHaveBeenCalled();
  });

  it("sends nothing when focus is not inside the menu", () => {
    mountMenu();
    const row = document.createElement("div");
    row.tabIndex = 0;
    document.body.append(row);
    const onRow = vi.fn();
    const onWindow = vi.fn();
    row.addEventListener("keydown", onRow);
    window.addEventListener("keydown", onWindow);
    row.focus();

    markFirstMenuItem();

    window.removeEventListener("keydown", onWindow);
    expect(onRow).not.toHaveBeenCalled();
    expect(onWindow).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(row);
  });
});
