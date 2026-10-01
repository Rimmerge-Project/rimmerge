import { describe, expect, it } from "vitest";
import { createI18n } from "vue-i18n";
import en from "@/locales/en.json";
import { describeSortProvenance, describeTieBreak } from "@/utils/sortProvenance";

const { t: translateEn } = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: false,
  messages: { en },
}).global;

describe("describeTieBreak", () => {
  it("labels rebuild and preserveCurrent", () => {
    expect(describeTieBreak("rebuild", translateEn)).toBe("rebuild");
    expect(describeTieBreak("preserveCurrent", translateEn)).toBe("preserve current");
  });
});

describe("describeSortProvenance", () => {
  it("summarizes all three settings on one line", () => {
    expect(
      describeSortProvenance(
        {
          tieBreak: "rebuild",
          useImportedPairs: false,
          useImportedPlacements: true,
        },
        translateEn,
      ),
    ).toBe("tie-break: rebuild · imported pairs: off · imported placements: on");
  });

  it("reflects preserveCurrent and both toggles on", () => {
    expect(
      describeSortProvenance(
        {
          tieBreak: "preserveCurrent",
          useImportedPairs: true,
          useImportedPlacements: true,
        },
        translateEn,
      ),
    ).toBe("tie-break: preserve current · imported pairs: on · imported placements: on");
  });
});
