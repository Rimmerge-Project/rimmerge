import { describe, expect, it } from "vitest";

import { isInert, ownsArrowKeys } from "@/composables/shortcutTargets";

describe("isInert", () => {
  it("is not inert inside a radio group: only its arrow keys are the group's own", () => {
    document.body.innerHTML =
      '<div role="radiogroup"><button role="radio" id="inside"></button></div><button id="outside"></button>';
    const inside = document.getElementById("inside");
    const outside = document.getElementById("outside");

    expect(isInert(inside)).toBe(false);
    expect(ownsArrowKeys(inside)).toBe(true);
    expect(ownsArrowKeys(outside)).toBe(false);
    expect(ownsArrowKeys(null)).toBe(false);
  });

  it("is inert inside a dialog and in a text field, and not for a null target", () => {
    document.body.innerHTML =
      '<div role="dialog"><button id="in-dialog"></button></div><input id="field">';

    expect(isInert(document.getElementById("in-dialog"))).toBe(true);
    expect(isInert(document.getElementById("field"))).toBe(true);
    expect(isInert(null)).toBe(false);
  });
});
