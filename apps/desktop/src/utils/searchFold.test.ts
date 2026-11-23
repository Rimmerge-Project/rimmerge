import { describe, expect, it } from "vitest";

import { searchFold } from "@/utils/searchFold";

describe("searchFold", () => {
  it("lowercases ASCII and leaves other text alone", () => {
    expect(searchFold("Sword 42")).toBe("sword 42");
  });

  it("folds a capital sigma at the end the same as one in the middle", () => {
    expect(searchFold("ΑΣ")).toBe("ασ");
    expect(searchFold("ΑΣΑ").startsWith(searchFold("ΑΣ"))).toBe(true);
  });

  it("writes a final sigma as a plain sigma", () => {
    expect(searchFold("κοσμος")).toBe("κοσμοσ");
    expect(searchFold("ΚΟΣΜΟΣ")).toBe(searchFold("κοσμος"));
  });

  it("keeps a character outside the basic plane whole", () => {
    expect(searchFold("a\u{1F600}B")).toBe("a\u{1F600}b");
  });
});
