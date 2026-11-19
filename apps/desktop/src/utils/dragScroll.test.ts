import { describe, expect, it } from "vitest";

import { edgeScrollStep } from "@/utils/dragScroll";

describe("edgeScrollStep", () => {
  it("does not scroll in the middle of the list", () => {
    expect(edgeScrollStep(300, 100, 500)).toBe(0);
  });

  it("scrolls up near the top edge and down near the bottom edge", () => {
    expect(edgeScrollStep(110, 100, 500)).toBeLessThan(0);
    expect(edgeScrollStep(490, 100, 500)).toBeGreaterThan(0);
  });

  it("scrolls faster the closer the pointer is to the edge", () => {
    expect(Math.abs(edgeScrollStep(101, 100, 500))).toBeGreaterThan(
      Math.abs(edgeScrollStep(140, 100, 500)),
    );
  });

  it("caps the speed when the pointer is past the edge", () => {
    expect(edgeScrollStep(0, 100, 500)).toBe(-18);
    expect(edgeScrollStep(900, 100, 500)).toBe(18);
  });
});
