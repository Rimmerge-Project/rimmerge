import { afterEach, describe, expect, it, vi } from "vitest";

import { detectSystemLocale } from "@/utils/detectSystemLocale";

describe("detectSystemLocale", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("resolves the LocaleOption for the detected locale", () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["zh-CN"]);

    expect(detectSystemLocale()).toEqual({
      locale: "zh-CN",
      nativeName: "简体中文",
      isPreview: true,
    });
  });

  it("resolves a Traditional Chinese system language to zh-TW", () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["zh-HK"]);

    expect(detectSystemLocale()).toEqual({
      locale: "zh-TW",
      nativeName: "繁體中文",
      isPreview: true,
    });
  });

  it("falls back to en when nothing matches", () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["sv-SE"]);

    expect(detectSystemLocale()).toEqual({ locale: "en", nativeName: "English", isPreview: false });
  });
});
