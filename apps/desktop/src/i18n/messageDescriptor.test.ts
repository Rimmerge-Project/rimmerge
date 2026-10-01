import { describe, expect, it } from "vitest";

import { descriptor, renderMessage, type Translate } from "@/i18n/messageDescriptor";

/** A `t()` that makes the key and the params visible in its output. */
const t: Translate = (key, params) => `${key}(${JSON.stringify(params ?? {})})`;

describe("renderMessage", () => {
  it("renders a nested descriptor param to text before substituting it", () => {
    const outer = descriptor("outer", { inner: descriptor("inner", { n: 1 }) });

    expect(renderMessage(t, outer)).toBe('outer({"inner":"inner({\\"n\\":1})"})');
  });

  it("passes a plain object that merely has a string key through untouched", () => {
    const lookalike = { key: "not.a.message.key" };

    expect(renderMessage(t, descriptor("outer", { item: lookalike }))).toBe(
      'outer({"item":{"key":"not.a.message.key"}})',
    );
  });
});

describe("descriptor", () => {
  it("compares equal to a plain object of the same fields", () => {
    expect(descriptor("a", { x: 1 }, 2)).toEqual({ key: "a", params: { x: 1 }, count: 2 });
  });
});
