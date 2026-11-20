import { describe, expect, it } from "vitest";

import { isTagSlug } from "@/utils/tags";

describe("isTagSlug", () => {
  it.each(["framework", "a1", "my-tag_2"])("accepts %s", (text) => {
    expect(isTagSlug(text)).toBe(true);
  });

  it.each(["", "Framework", "two words", "tag!", " tag", "tag\n"])(
    "refuses %j without rewriting it",
    (text) => {
      expect(isTagSlug(text)).toBe(false);
    },
  );
});
