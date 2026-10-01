import { describe, expect, it } from "vitest";

import { describeAboutUnreadable } from "@/utils/aboutUnreadable";

describe("describeAboutUnreadable", () => {
  it("distinguishes a read failure from an invalid file and carries no detail", () => {
    expect(describeAboutUnreadable("io")).toEqual({
      key: "modInfo.panel.aboutUnreadableIoHeadline",
    });
    expect(describeAboutUnreadable("xml")).toEqual({
      key: "modInfo.panel.aboutUnreadableXmlHeadline",
    });
  });
});
