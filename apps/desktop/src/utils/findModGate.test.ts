import { describe, expect, it } from "vitest";

import { describeFindModGate } from "@/utils/findModGate";

describe("describeFindModGate", () => {
  it("keeps an any-active gate and a none-active gate distinct", () => {
    expect(describeFindModGate({ kind: "anyActive", mods: ["A", "B", "C"] }, "en")).toEqual({
      key: "defPage.gate.anyActive",
      params: { mods: "A, B, or C" },
    });
    expect(describeFindModGate({ kind: "noneActive", mods: ["A", "B"] }, "en")).toEqual({
      key: "defPage.gate.noneActive",
      params: { mods: "A or B" },
    });
  });

  it("joins the mods with the locale's own word for or", () => {
    const message = describeFindModGate({ kind: "anyActive", mods: ["A", "B"] }, "pt-BR");
    expect(message.params).toEqual({ mods: "A ou B" });
  });
});
