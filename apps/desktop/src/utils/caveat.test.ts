import { describe, expect, it } from "vitest";
import { createI18n } from "vue-i18n";
import en from "@/locales/en.json";
import type { CaveatDto } from "@/types/generated/CaveatDto";
import { caveatLabel } from "@/utils/caveat";

const { t } = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: false,
  messages: { en },
}).global;

const label = (id: string): string => `Label(${id})`;

describe("caveatLabel", () => {
  it("names the restored map entry and its contributor", () => {
    const caveat: CaveatDto = {
      kind: "clobberedMapEntry",
      path: "wildAnimals/XBM_Theropod",
      by: "a.mod",
    };
    const text = caveatLabel(caveat, t, label, "en");
    expect(text).toContain("wildAnimals/XBM_Theropod");
    expect(text).toContain("Label(a.mod)");
    expect(text).toContain("restored");
  });

  it("names every out-of-scope owner through the label resolver, as a disjunction", () => {
    const caveat: CaveatDto = { kind: "outOfScopeOwners", mods: ["a.mod", "c.mod"] };
    const text = caveatLabel(caveat, t, label, "en");
    // The message says "any of {mods}", so the list is an `Intl.ListFormat`
    // disjunction ("A or B"), not a conjunction or a hard-coded `", "` join.
    expect(text).toContain("any of Label(a.mod) or Label(c.mod)");
  });

  it("renders failedOp with the mod and xpath", () => {
    const caveat: CaveatDto = { kind: "failedOp", modId: "a.mod", xpath: "Defs/ThingDef" };
    expect(caveatLabel(caveat, t, label, "en")).toBe(
      "Label(a.mod)'s patch operation on Defs/ThingDef matched nothing",
    );
  });

  it("renders duplicateTemplate naming every registrant", () => {
    const caveat: CaveatDto = {
      kind: "duplicateTemplate",
      name: "Base",
      owners: ["a.mod", "b.mod"],
    };
    const text = caveatLabel(caveat, t, label, "en");
    expect(text).toContain('template "Base"');
    expect(text).toContain("Label(a.mod) and Label(b.mod)");
  });

  it("pluralizes unscopedOps by its own count", () => {
    const one: CaveatDto = { kind: "unscopedOps", modId: "a.mod", count: 1 };
    const many: CaveatDto = { kind: "unscopedOps", modId: "a.mod", count: 3 };
    expect(caveatLabel(one, t, label, "en")).toBe(
      "Label(a.mod) has 1 patch operation that couldn't be scoped to a def",
    );
    expect(caveatLabel(many, t, label, "en")).toBe(
      "Label(a.mod) has 3 patch operations that couldn't be scoped to a def",
    );
  });
});
