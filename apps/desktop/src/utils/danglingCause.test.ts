import { describe, expect, it } from "vitest";
import { createI18n } from "vue-i18n";
import en from "@/locales/en.json";
import type { DanglingCauseDto } from "@/types/generated/DanglingCauseDto";
import { danglingCauseText } from "@/utils/danglingCause";

const { t } = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: false,
  messages: { en },
}).global;

const label = (id: string): string => `Label(${id})`;

describe("danglingCauseText", () => {
  it("names the removing mod through the label resolver", () => {
    const cause: DanglingCauseDto = { kind: "removedBy", modId: "a.mod", file: "a.xml" };
    expect(danglingCauseText(cause, t, label)).toBe("it was removed by Label(a.mod)'s own patch");
  });

  it("names the mod and folder for an unloaded-folder cause", () => {
    const cause: DanglingCauseDto = {
      kind: "onlyInUnloadedFolder",
      modId: "a.mod",
      folder: "1.4",
    };
    const text = danglingCauseText(cause, t, label);
    expect(text).toContain("Label(a.mod)");
    expect(text).toContain("1.4");
  });

  it("renders a fixed sentence for definedNowhere with no interpolation", () => {
    const cause: DanglingCauseDto = { kind: "definedNowhere" };
    expect(danglingCauseText(cause, t, label)).toBe("no installed mod, active or not, defines it");
  });

  it("renders a fixed sentence for unexplained", () => {
    const cause: DanglingCauseDto = { kind: "unexplained" };
    expect(danglingCauseText(cause, t, label)).toBe("why it's missing couldn't be determined");
  });
});
